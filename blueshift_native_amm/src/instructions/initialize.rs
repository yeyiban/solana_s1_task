use crate::{
    instructions::helper::{AccountCheck, ProgramAccount, ProgramAccountInit, SignerAccount},
    Config,
};
use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::{instructions::InitializeMint2, state::Mint};
use std::mem::MaybeUninit;

pub struct InitializeAccounts<'a> {
    // config 账户的创建者。这不一定也必须是其权限持有者。
    pub initializer: &'a AccountView,
    // 代表池流动性的铸币账户。
    pub mint_lp: &'a AccountView,
    // 正在初始化的配置账户。
    pub config: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        let [initializer, mint_lp, config, _, _] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        SignerAccount::check(initializer)?;
        Ok(Self {
            initializer,
            mint_lp,
            config,
        })
    }
}

#[repr(C, packed)]
pub struct InitializeInstructionData {
    // 用于PDA（程序派生地址）种子推导的随机数。这允许创建唯一的池实例。
    pub seed: u64,
    // 以基点表示的交换费（1基点=0.01%）。此费用在每次交易中收取，并分配给流动性提供者。
    pub fee: u16,
    // 池中代币X的SPL代币铸造地址。
    pub mint_x: [u8; 32],
    // 池中代币Y的SPL代币铸造地址。
    pub mint_y: [u8; 32],
    // 用于推导 config 账户PDA的bump种子。
    pub config_bump: [u8; 1],
    // 用于推导 lp_mint 账户PDA的bump种子。
    pub lp_bump: [u8; 1],
    // 将拥有AMM管理权限的公钥。
    pub authority: [u8; 32],
}

impl<'a> TryFrom<&'a [u8]> for InitializeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        const INITIALIZE_DATA_LEN_WITH_AUTHORITY: usize = size_of::<InitializeInstructionData>();
        const INITIALIZE_DATA_LEN: usize =
            INITIALIZE_DATA_LEN_WITH_AUTHORITY - size_of::<[u8; 32]>();

        match data.len() {
            INITIALIZE_DATA_LEN_WITH_AUTHORITY => {
                Ok(unsafe { (data.as_ptr() as *const Self).read_unaligned() })
            }
            INITIALIZE_DATA_LEN => {
                // If the authority is not present, we need to build the buffer and add it at the end before transmuting to the struct
                let mut raw: MaybeUninit<[u8; INITIALIZE_DATA_LEN_WITH_AUTHORITY]> =
                    MaybeUninit::uninit();
                let raw_ptr = raw.as_mut_ptr() as *mut u8;
                unsafe {
                    // Copy the provided data
                    core::ptr::copy_nonoverlapping(data.as_ptr(), raw_ptr, INITIALIZE_DATA_LEN);
                    // Add the authority to the end of the buffer
                    core::ptr::write_bytes(raw_ptr.add(INITIALIZE_DATA_LEN), 0, 32);
                    // Now transmute to the struct
                    Ok((raw.as_ptr() as *const Self).read_unaligned())
                }
            }
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }
}

pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
    pub instruction_data: InitializeInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountView])> for Initialize<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountView])) -> Result<Self, Self::Error> {
        let accounts = InitializeAccounts::try_from(accounts)?;
        let instruction_data = InitializeInstructionData::try_from(data)?;

        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Initialize<'a> {
    pub fn process(&self) -> ProgramResult {
        let seed_binding = self.instruction_data.seed.to_le_bytes();
        let config_seeds = [
            Seed::from(b"config"),
            Seed::from(&seed_binding),
            Seed::from(&self.instruction_data.mint_x),
            Seed::from(&self.instruction_data.mint_y),
            Seed::from(&self.instruction_data.config_bump),
        ];

        let mint_lp_seeds = [
            Seed::from(b"mint_lp"),
            Seed::from(self.accounts.config.address().as_ref()),
            Seed::from(&self.instruction_data.lp_bump),
        ];

        // Initialize the config
        let config_lamports = Rent::get()?.try_minimum_balance(Config::LEN)?;

        // Create signer with seeds slice
        let config_signer = [Signer::from(&config_seeds)];
        // 创建 config
        // Create the account
        CreateAccount {
            from: self.accounts.initializer,
            to: self.accounts.config,
            lamports: config_lamports,
            space: Config::LEN as u64,
            owner: &crate::ID,
        }
        .invoke_signed(&config_signer)?;
        // Populate the escrow account
        let mut config = Config::load_mut(self.accounts.config)?;

        // 填充config数据
        config.set_inner(
            self.instruction_data.seed,
            Address::from(self.instruction_data.authority),
            Address::from(self.instruction_data.mint_x),
            Address::from(self.instruction_data.mint_y),
            self.instruction_data.fee,
            self.instruction_data.config_bump,
        )?;
        // 创建 mint_lp
        let lp_lamports = Rent::get()?.try_minimum_balance(Mint::LEN)?;

        // Create signer with seeds slice
        let lp_signer = [Signer::from(&mint_lp_seeds)];
        // Create the account
        CreateAccount {
            from: self.accounts.initializer,
            to: self.accounts.mint_lp,
            lamports: lp_lamports,
            space: Mint::LEN as u64,
            // 这个地方 owner 必须是 token_program 的地址或者 token_2022_program 的地址
            owner: &pinocchio_token::ID,
        }
        .invoke_signed(&lp_signer)?;
        // 初始化铸币账户
        InitializeMint2 {
            mint: self.accounts.mint_lp,
            decimals: 6,
            mint_authority: self.accounts.config.address(),
            freeze_authority: Some(self.accounts.config.address()),
        }
        .invoke()?;

        Ok(())
    }
}
