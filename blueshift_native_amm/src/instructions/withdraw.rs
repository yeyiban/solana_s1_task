use crate::instructions::helper::{
    AccountCheck, AssociatedTokenAccount, AssociatedTokenAccountCheck, MintInterface,
    ProgramAccount, SignerAccount,
};
use crate::{AmmState, Config};
use pinocchio::cpi::{Seed, Signer};
use pinocchio::sysvars::clock::Clock;
use pinocchio::sysvars::Sysvar;
use pinocchio::{error::ProgramError, AccountView, ProgramResult};
use pinocchio_token::instructions::{Burn, MintTo, Transfer};
use pinocchio_token::state::{Mint, TokenAccount};
use solana_address::Address;

pub struct WithdrawAccounts<'a> {
    // 将代币提取到 AMM 流动性中的用户。
    pub user: &'a AccountView,
    // 表示池流动性的 Mint 账户。
    pub mint_lp: &'a AccountView,
    // 存储所有存入池中的 X 代币的代币账户。
    pub vault_x: &'a AccountView,
    // 存储所有存入池中的 Y 代币的代币账户。
    pub vault_y: &'a AccountView,
    // 用户的 X 代币关联账户。这是用户的 X 代币将从池中转移到的目标账户。
    pub user_x_ata: &'a AccountView,
    // 用户的 Y 代币关联账户。这是用户的 Y 代币将从池中转移到的目标账户。
    pub user_y_ata: &'a AccountView,
    // 用户的 LP 代币关联账户。这是 LP 代币将被销毁的来源账户。
    pub user_lp_ata: &'a AccountView,
    // AMM 池的配置账户。
    pub config: &'a AccountView,
    // SPL 代币程序账户。这是执行代币操作（如转账和铸造）所需的。
    pub token_program: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for WithdrawAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        let [user, mint_lp, vault_x, vault_y, user_x_ata, user_y_ata, user_lp_ata, config, token_program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        SignerAccount::check(user)?;
        MintInterface::check(mint_lp)?;
        ProgramAccount::check(config)?;

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

pub struct WithdrawInstructionData {
    // 用户希望销毁的 LP 代币数量。
    pub amount: u64,
    // 用户愿意提取的最小 Token X 数量。
    pub min_x: u64,
    // 用户愿意提取的最小 Token Y 数量。
    pub min_y: u64,
    // 此订单的过期时间。确保交易必须在一定时间内完成非常重要。
    pub expiration: i64,
}

impl TryFrom<&[u8]> for WithdrawInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<u64>() * 4 {
            return Err(ProgramError::InvalidArgument);
        };
        let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let min_x = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let min_y = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let expiration = i64::from_le_bytes(data[24..32].try_into().unwrap());

        // Instruction Checks
        if amount <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if min_x <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if min_y <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if Clock::get()?.unix_timestamp > expiration {
            // 超时
            return Err(ProgramError::InvalidInstructionData);
        }
        Ok(Self {
            amount,
            min_x,
            min_y,
            expiration,
        })
    }
}

pub struct Withdraw<'a> {
    pub accounts: WithdrawAccounts<'a>,
    pub instruction_data: WithdrawInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountView])> for Withdraw<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountView])) -> Result<Self, Self::Error> {
        let accounts = WithdrawAccounts::try_from(accounts)?;
        let instruction_data = WithdrawInstructionData::try_from(data)?;
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Withdraw<'a> {
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
        // 计算需要从vault转给用户的代币数量
        let (x, y) = match mint_lp.supply() == self.instruction_data.amount {
            true => (vault_x.amount(), vault_y.amount()),
            false => {
                let amounts = Self::xy_withdraw_amounts_from_l(
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

        // Check for slippage
        if !(x >= self.instruction_data.min_x && y >= self.instruction_data.min_y) {
            return Err(ProgramError::InvalidArgument);
        }
        // 1.把对应x,y代币转移到用户的ata账户
        // 构建config签名
        let seed_bytes = config.seed().to_le_bytes();
        let mut seed_array = [0u8; 8];
        seed_array.copy_from_slice(&seed_bytes);
        let bump = config.config_bump();
        let config_seeds = [
            Seed::from(b"config"),
            Seed::from(&seed_array), // 正确的 seed bytes
            Seed::from(config.mint_x().as_ref()),
            Seed::from(config.mint_y().as_ref()),
            Seed::from(&bump), // 使用 config 中存储的 bump
        ];

        let config_signer = [Signer::from(&config_seeds)];
        Transfer {
            from: self.accounts.vault_x,
            to: self.accounts.user_x_ata,
            authority: self.accounts.config,
            amount: x,
        }
        .invoke_signed(&config_signer)?;

        Transfer {
            from: self.accounts.vault_y,
            to: self.accounts.user_y_ata,
            authority: self.accounts.config,
            amount: y,
        }
        .invoke_signed(&config_signer)?;

        // 向用户的代币账户铸造相应数量的 LP 代币

        Burn {
            mint: self.accounts.mint_lp,
            account: self.accounts.user_lp_ata,
            amount: self.instruction_data.amount,
            authority: self.accounts.user,
        }
        .invoke()?;
        Ok(())
    }

    // Get amount of X and Y to withdraw from liquidity token amount
    // 这个是根据git上直接搬过来的 错误类型改了一下
    // x: 库存的x代币数量
    // y: 库存的y代币数量
    // l: 流动性代币lp的流通量
    // a: 用户希望销毁的 LP 代币数量
    // precision: 精度
    fn xy_withdraw_amounts_from_l(
        x: u64,
        y: u64,
        l: u64,
        a: u64,
        precision: u32,
    ) -> Result<(u64, u64), ProgramError> {
        // ((lp代币总量 - 用户希望销毁的lp代币数量) * 精度) / 流通代币总量
        // 上面的约等于是 用户希望销毁的代币数量后 总的流通代币占流通总量的占比 * 精度
        // 简单来说就是总流动性的剩余比例
        let ratio = ((l - a) as u128)
            .checked_mul(precision as u128)
            .ok_or(ProgramError::InvalidArgument)?
            .checked_div(l as u128)
            .ok_or(ProgramError::InvalidArgument)?;
        // 库存代币x的总量 - (库存x代币总量 * 总流动性的剩余比例 / 精度)
        let withdraw_x = (x as u128)
            .checked_sub(
                (x as u128)
                    .checked_mul(ratio)
                    .ok_or(ProgramError::InvalidArgument)?
                    .checked_div(precision as u128)
                    .ok_or(ProgramError::InvalidArgument)?,
            )
            .ok_or(ProgramError::InvalidArgument)? as u64;
        // 库存代币y的总量 - (库存y代币总量 * 总流动性的剩余比例 / 精度)
        let withdraw_y = (y as u128)
            .checked_sub(
                (y as u128)
                    .checked_mul(ratio)
                    .ok_or(ProgramError::InvalidArgument)?
                    .checked_div(precision as u128)
                    .ok_or(ProgramError::InvalidArgument)?,
            )
            .ok_or(ProgramError::InvalidArgument)? as u64;
        // 总结下来就是根据用户希望销毁的代币占总流通量的比例 直接从库存的x,y代币中直接按比例提取。
        Ok((withdraw_x, withdraw_y))
    }
}
