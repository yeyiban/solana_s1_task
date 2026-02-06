use crate::instructions::helper::{
    AccountCheck, AssociatedTokenAccount, AssociatedTokenAccountCheck, AssociatedTokenAccountInit,
    MintInterface, ProgramAccount, SignerAccount,
};
use crate::{AmmState, Config};
use pinocchio::cpi::{Seed, Signer};
use pinocchio::{
    error::ProgramError,
    sysvars::{clock::Clock, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_token::instructions::{MintTo, Transfer};
use pinocchio_token::state::{Mint, TokenAccount};

pub struct DepositAccounts<'a> {
    // 将代币存入 AMM 流动性的用户。
    pub user: &'a AccountView,
    // 代表池流动性的铸币账户。
    pub mint_lp: &'a AccountView,
    // 存储所有存入池中的 X 代币的代币账户。
    pub vault_x: &'a AccountView,
    // 存储所有存入池中的 Y 代币的代币账户。
    pub vault_y: &'a AccountView,
    // 用户的 X 代币关联账户。这是用户的 X 代币将从中转移到池中的源账户。
    pub user_x_ata: &'a AccountView,
    // 用户的 Y 代币关联账户。这是用户的 Y 代币将从中转移到池中的源账户。
    pub user_y_ata: &'a AccountView,
    // 用户的 LP 代币关联账户。这是铸造 LP 代币的目标账户。
    pub user_lp_ata: &'a AccountView,
    // AMM 池的配置账户。
    pub config: &'a AccountView,
    // SPL 代币程序账户。
    pub token_program: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for DepositAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        let [user, mint_lp, vault_x, vault_y, user_x_ata, user_y_ata, user_lp_ata, config, token_program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        SignerAccount::check(user)?;
        ProgramAccount::check(config)?;
        MintInterface::check(mint_lp)?;
        Ok(Self {
            user,
            mint_lp,
            vault_x,
            vault_y,
            user_x_ata,
            user_y_ata,
            user_lp_ata,
            config,
            token_program,
        })
    }
}

pub struct DepositInstructionData {
    // 用户希望接收的 LP 代币数量
    pub amount: u64,
    // 用户愿意存入的最大 Token X 数量
    pub max_x: u64,
    // 用户愿意存入的最大 Token Y 数量
    pub max_y: u64,
    // 此订单的过期时间。确保交易必须在一定时间内完成非常重要。
    pub expiration: i64,
}

impl<'a> TryFrom<&'a [u8]> for DepositInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        // u64 和 i64 的长度是一样的
        if data.len() != size_of::<u64>() * 4 {
            return Err(ProgramError::InvalidInstructionData);
        }
        let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let max_x = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let max_y = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let expiration = i64::from_le_bytes(data[24..32].try_into().unwrap());
        // Instruction Checks
        if amount <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if max_x <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if max_y <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if Clock::get()?.unix_timestamp > expiration {
            // 超时
            return Err(ProgramError::InvalidInstructionData);
        }
        Ok(Self {
            amount,
            max_x,
            max_y,
            expiration,
        })
    }
}

pub struct Deposit<'a> {
    pub accounts: DepositAccounts<'a>,
    pub instruction_data: DepositInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountView])> for Deposit<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountView])) -> Result<Self, Self::Error> {
        let accounts = DepositAccounts::try_from(accounts)?;
        let instruction_data = DepositInstructionData::try_from(data)?;

        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Deposit<'a> {
    pub fn process(&self) -> ProgramResult {
        // 校验用户的ata账户地址有没有问题
        let config = Config::load(self.accounts.config)?;
        if config.state() != (AmmState::Initialized as u8) {
            return Err(ProgramError::InvalidArgument);
        }
        AssociatedTokenAccount::check(
            self.accounts.user_x_ata,
            self.accounts.user,
            config.mint_x(),
            self.accounts.token_program,
        )?;
        AssociatedTokenAccount::check(
            self.accounts.user_y_ata,
            self.accounts.user,
            config.mint_y(),
            self.accounts.token_program,
        )?;
        AssociatedTokenAccount::check(
            self.accounts.user_lp_ata,
            self.accounts.user,
            self.accounts.mint_lp.address(),
            self.accounts.token_program,
        )?;

        // 校验 金库的地址有没有为题
        // 其实 vault并不需要被初始化 因为这个地址是被计算出来的 只要计算出来 就可以标记里面有多少钱
        // Check if the vault_x is valid
        let (vault_x, _) = Address::find_program_address(
            &[
                self.accounts.config.address().as_ref(),
                self.accounts.token_program.address().as_ref(),
                config.mint_x().as_ref(),
            ],
            &pinocchio_associated_token_account::ID,
        );
        if vault_x.ne(self.accounts.vault_x.address()) {
            return Err(ProgramError::InvalidAccountData);
        }
        let (vault_y, _) = Address::find_program_address(
            &[
                self.accounts.config.address().as_ref(),
                self.accounts.token_program.address().as_ref(),
                config.mint_y().as_ref(),
            ],
            &pinocchio_associated_token_account::ID,
        );
        if vault_y.ne(self.accounts.vault_y.address()) {
            return Err(ProgramError::InvalidAccountData);
        }
        // 把账户从 AccountView 转化成功能账户
        let mint_lp = unsafe { Mint::from_account_view_unchecked(self.accounts.mint_lp)? };
        let vault_x = unsafe { TokenAccount::from_account_view_unchecked(self.accounts.vault_x)? };
        let vault_y = unsafe { TokenAccount::from_account_view_unchecked(self.accounts.vault_y)? };

        // 计算用户需要支付的 x y 代币的数量
        let (x, y) = match mint_lp.supply() == 0 && vault_x.amount() == 0 && vault_y.amount() == 0 {
            // 这里就是当vault_x 和 vault_y 和 lp_token供应量全部都是0 也就是首次注入流动性
            // 在这个还没有暴露的情况下 默认可以认为这就是定下 x * y 的值 也可以认为初始定下了 x y 和 lp代币的兑换比率
            true => (self.instruction_data.max_x, self.instruction_data.max_y),
            false => {
                let amounts = Self::xy_deposit_amounts_from_l(
                    vault_x.amount(),
                    vault_y.amount(),
                    mint_lp.supply(),
                    self.instruction_data.amount,
                    6,
                )
                .map_err(|_| ProgramError::InvalidArgument)?;
                (amounts.0, amounts.1)
            }
        };
        // 将用户的代币账户中的金额转移到金库
        Transfer {
            from: self.accounts.user_x_ata,
            to: self.accounts.vault_x,
            authority: self.accounts.user,
            amount: x,
        }
        .invoke()?;

        Transfer {
            from: self.accounts.user_y_ata,
            to: self.accounts.vault_y,
            authority: self.accounts.user,
            amount: y,
        }
        .invoke()?;

        // 向用户的代币账户铸造相应数量的 LP 代币
        // 构建config签名
        let seed_bytes = config.seed().to_le_bytes();
        let mut seed_array = [0u8; 8];
        seed_array.copy_from_slice(&seed_bytes);
        let bump = config.config_bump();
        let config_seeds = [
            Seed::from(b"config"),
            Seed::from(&seed_array),  // 正确的 seed bytes
            Seed::from(config.mint_x().as_ref()),
            Seed::from(config.mint_y().as_ref()),
            Seed::from(&bump),  // 使用 config 中存储的 bump
        ];

        let config_signer = [Signer::from(&config_seeds)];
        MintTo {
            mint: self.accounts.mint_lp,
            account: self.accounts.user_lp_ata,
            mint_authority: self.accounts.config,
            amount: self.instruction_data.amount,
        }
        .invoke_signed(&config_signer)?;

        Ok(())
    }

    // 这个是根据git上直接搬过来的 错误类型改了一下
    // x: 库存的x代币数量
    // y: 库存的y代币数量
    // l: 流动性代币lp的流通量
    // a: 用户希望接收的 LP 代币数量
    // precision: 精度
    fn xy_deposit_amounts_from_l(
        x: u64,
        y: u64,
        l: u64,
        a: u64,
        precision: u32,
    ) -> Result<(u64, u64), ProgramError> {
        // ((已经流通的lp数量 + 用户希望接收的lp数量) * 精度) / 已经流动的lp数量
        // 这里计算得到的是 用户如果想要拿到希望数量的lp所需要支付的对应x和y代币占库存的占比
        // 由于分母比分子大 所以这里的ratio必定大于1
        let ratio = (l as u128)
            .checked_add(a as u128)
            .ok_or(ProgramError::InvalidInstructionData)?
            .checked_mul(precision as u128)
            .ok_or(ProgramError::InvalidInstructionData)?
            .checked_div(l as u128)
            .ok_or(ProgramError::InvalidInstructionData)?;
        // ((库存代币x数量 * 比例) / 精度) - 库存x代币的数量 = 用户需要支付的x代币数量
        let deposit_x = (x as u128)
            .checked_mul(ratio)
            .ok_or(ProgramError::InvalidInstructionData)?
            .checked_div(precision as u128)
            .ok_or(ProgramError::InvalidInstructionData)?
            .checked_sub(x as u128)
            .ok_or(ProgramError::InvalidInstructionData)? as u64;
        // ((库存代币y数量 * 比例) / 精度) - 库存y代币的数量 = 用户需要支付的y代币数量
        let deposit_y = (y as u128)
            .checked_mul(ratio)
            .ok_or(ProgramError::InvalidInstructionData)?
            .checked_div(precision as u128)
            .ok_or(ProgramError::InvalidInstructionData)?
            .checked_sub(y as u128)
            .ok_or(ProgramError::InvalidInstructionData)? as u64;
        Ok((deposit_x, deposit_y))
    }
}
