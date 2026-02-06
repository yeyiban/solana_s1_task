use crate::instructions::helper::{
    AccountCheck, AssociatedTokenAccount, AssociatedTokenAccountCheck, SignerAccount,
};
use crate::{AmmState, Config};
use pinocchio::{
    error::ProgramError,
    sysvars::{clock::Clock, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio::cpi::{Seed, Signer};
use pinocchio_token::{instructions::Transfer, state::TokenAccount};
use solana_program_log::log;

pub struct SwapAccounts<'a> {
    // 将代币交换到 AMM 流动性中的用户。
    pub user: &'a AccountView,
    // 用户的代币 X 关联账户。此账户将接收或发送代币 X 到池中。
    pub user_x_ata: &'a AccountView,
    // 用户的代币 Y 关联账户。此账户将接收或发送代币 Y 到池中。
    pub user_y_ata: &'a AccountView,
    // 持有所有存入池中的代币 X 的代币账户。
    pub vault_x: &'a AccountView,
    // 持有所有存入池中的代币 Y 的代币账户。
    pub vault_y: &'a AccountView,
    // AMM 池的配置账户。存储所有相关的池参数和状态。
    pub config: &'a AccountView,
    // SPL 代币程序账户。执行代币操作（如转账和铸造）所需。
    pub token_program: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for SwapAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        let [user, user_x_ata, user_y_ata, vault_x, vault_y, config, token_program] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        SignerAccount::check(user)?;
        Ok(Self {
            user,
            user_x_ata,
            user_y_ata,
            vault_x,
            vault_y,
            config,
            token_program,
        })
    }
}

pub struct SwapInstructionData {
    // 此交换是从代币 X 到代币 Y 或反之进行的；需要正确对齐账户。
    pub is_x: bool,
    // 用户愿意用来交换另一种代币的代币数量。
    pub amount: u64,
    // 用户愿意在交换 amount 时接收的最小代币数量。
    pub min: u64,
    // 此订单的过期时间。确保交易必须在一定时间内完成非常重要。
    pub expiration: i64,
}

impl TryFrom<&[u8]> for SwapInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        log!("初始化数据");
        if data.len() != size_of::<u64>() * 3 + size_of::<bool>() {
            return Err(ProgramError::InvalidArgument);
        }
        let is_x = data.get(0) != Some(&0u8);
        let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
        let min = u64::from_le_bytes(data[9..17].try_into().unwrap());
        let expiration = i64::from_le_bytes(data[17..25].try_into().unwrap());

        // Instruction Checks
        if amount <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if min <= 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
        if Clock::get()?.unix_timestamp > expiration {
            // 超时
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self {
            is_x,
            amount,
            min,
            expiration,
        })
    }
}

pub struct Swap<'a> {
    pub accounts: SwapAccounts<'a>,
    pub instruction_data: SwapInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountView])> for Swap<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountView])) -> Result<Self, Self::Error> {
        let accounts = SwapAccounts::try_from(accounts)?;
        let instruction_data = SwapInstructionData::try_from(data)?;
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Swap<'a> {
    pub fn process(&self) -> ProgramResult {
        // 根据题目要求 所有的ata都已经在指令外初始化了
        // 否则的话 对于用户接收代币的ata需要使用init_if_needed的处理防止账户不存在
        // 校验用户的ata账户地址有没有问题
        log!("开始校验");
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
        // Deserialize the token accounts
        let vault_x = unsafe { TokenAccount::from_account_view_unchecked(self.accounts.vault_x)? };
        let vault_y = unsafe { TokenAccount::from_account_view_unchecked(self.accounts.vault_y)? };
        // Swap Calculations
        let mut curve = ConstantProduct::init(
            vault_x.amount(),
            vault_y.amount(),
            vault_x.amount(),
            config.fee(),
            None,
        )
        .map_err(|_| ProgramError::Custom(1))?;
        let p = match self.instruction_data.is_x {
            true => LiquidityPair::X,
            false => LiquidityPair::Y,
        };
        // 0- deposit 1-fee 2-withdraw
        let swap_result = curve
            .swap(p, self.instruction_data.amount, self.instruction_data.min)
            .map_err(|_| ProgramError::Custom(1))?;
        // Check for correct values
        // 不允许支付金额或者提现金额为0
        if swap_result.0 == 0 || swap_result.2 == 0 {
            return Err(ProgramError::InvalidArgument);
        }
        // 进行交易
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
        // 因为支付的是代币 所以需要使用 pinocchio_token 里面的 Transfer
        if self.instruction_data.is_x {
            // 如果是 支付x 获取 y
            // 从用户的 x ata里面转移 amount 个代币到 金库
            Transfer {
                from: self.accounts.user_x_ata,
                to: self.accounts.vault_x,
                authority: self.accounts.user,
                amount: self.instruction_data.amount,
            }
            .invoke()?;
            Transfer {
                from: self.accounts.vault_y,
                to: self.accounts.user_y_ata,
                authority: self.accounts.config,
                amount: swap_result.2,
            }
            .invoke_signed(&config_signer)?;
        } else {
            // 如果是 支付y 获取 x
            // 从用户的 y ata里面转移 amount 个代币到 金库
            Transfer {
                from: self.accounts.user_y_ata,
                to: self.accounts.vault_y,
                authority: self.accounts.user,
                amount: self.instruction_data.amount,
            }
            .invoke()?;
            Transfer {
                from: self.accounts.vault_x,
                to: self.accounts.user_x_ata,
                authority: self.accounts.config,
                amount: swap_result.2,
            }
            .invoke_signed(&config_signer)?;
        }
        Ok(())
    }
}

/// 下面的代码都是从根据task的要求引入的git上复制过来的
/// 根据自己的理解写了一些注释
// 这里应该是校验数组中的全部数据是否包含 0
macro_rules! assert_non_zero {
    ($array:expr) => {
        if $array.contains(&0u64) {
            return Err(ProgramError::InvalidArgument);
        }
    };
}

// 这个宏的意思是比较两个2个数的大小 如果 第一个数比第二个数的就报错
macro_rules! swap_slippage {
    ($x:expr, $x_min:expr) => {
        if $x < $x_min {
            return Err(ProgramError::InvalidArgument);
        }
    };
}

#[derive(Debug)]
pub enum LiquidityPair {
    X,
    Y,
}

// x-代币x的金额 y-代币y的金额 l-代币x的金额 fee-费率 precision-精度
// 这里 l 为代币x的金额其实是因为在初始化里面会有个逻辑 当代币x的数量为0的时候会取x和y的最大值
#[derive(Debug)]
pub struct ConstantProduct {
    x: u64,   // Balance of Token X
    y: u64,   // Balance of Token Y
    l: u64,   // LP Token Balance
    fee: u16, // Fee in basis points, ie: 100 = 1%
    precision: u32,
}
impl ConstantProduct {
    // Create a new Constant Product Curve
    pub fn init(
        x: u64,
        y: u64,
        l: u64,
        fee: u16,
        precision: Option<u8>,
    ) -> Result<ConstantProduct, ProgramError> {
        // Assert non-zero values of X and Y
        assert_non_zero!([x, y]);
        // 给一个默认的精度
        let precision = match precision {
            Some(p) => 10u32
                .checked_pow(p as u32)
                .ok_or(ProgramError::InvalidArgument)?,
            None => 1_000_000,
        };

        // If L is zero, make it the higher value of either X or Y, as this will have less rounding errors
        let l = match l > 0 {
            true => l,
            false => x.max(y),
        };

        Ok(ConstantProduct {
            x,
            y,
            l,
            fee,
            precision,
        })
    }

    // p: x代表 支付x 获取y；y代表 支付y 获取x
    // a: 愿意支付的代币数量
    // min: 愿意接受的最小代币数量
    // Swap asset X for asset Y or vice versa with slippage protection
    pub fn swap(
        &mut self,
        p: LiquidityPair,
        a: u64,
        min: u64,
    ) -> Result<(u64, u64, u64), ProgramError> {
        // 支付金额 * (10_000 - 费率) / 10_000
        // 这里是计算出扣掉费率之后 实际能用来买新代币的代币数量
        // 这里的 10_000 就是 x * y 的值 也就是 k
        let a2 = (a as u128)
            .checked_mul((10_000 - self.fee) as u128)
            .ok_or(ProgramError::InvalidArgument)?
            .checked_div(10_000)
            .ok_or(ProgramError::InvalidArgument)? as u64;

        // 返回的是新的x值 新的y值和 支付给用户的代币数量
        let (new_x, new_y, withdraw) = match p {
            // 支付x 获取y
            LiquidityPair::X => {
                // 金库的代币x数量 + 买代币的数量 = 如果交易成功最新的x代币数量
                let x2 = self
                    .x
                    .checked_add(a2)
                    .ok_or(ProgramError::InvalidArgument)?;
                // 获取y2的数量
                let y2 = Self::y2_from_x_swap_amount(self.x, self.y, a2)?;
                let delta_y = Self::delta_y_from_x_swap_amount(self.x, self.y, a2)?;
                (x2, y2, delta_y)
            }
            // 支付y 获取x
            LiquidityPair::Y => {
                // 计算出新的x代币的数量
                let x2 = Self::x2_from_y_swap_amount(self.x, self.y, a)?;
                // 金库的代币x数量 + 买代币的数量 = 如果交易成功最新的x代币数量
                let y2 = self.y.checked_add(a).ok_or(ProgramError::InvalidArgument)?;
                let delta_x = Self::delta_x_from_y_swap_amount(self.x, self.y, a)?;
                (x2, y2, delta_x)
            }
        };
        // 如果提现的数据小于用户愿意获取的最小值 报错返回
        swap_slippage!(withdraw, min);
        // 费率也就是支付给AMM的钱 就是 用户支付的代币金额 - 用户实际用于支付的代币金额
        let fee = a.checked_sub(a2).ok_or(ProgramError::InvalidArgument)?;
        self.x = new_x;
        self.y = new_y;

        Ok((a, fee, withdraw))
    }

    // x-之前的代币x的数量 y-之前的代币y的数量 a-新代币x的数量
    fn y2_from_x_swap_amount(x: u64, y: u64, a: u64) -> Result<u64, ProgramError> {
        Self::x2_from_y_swap_amount(y, x, a)
    }

    // 这里是一个公用方法 x转y 和 y转x 都走这个方法 但是这里的x和y传的值不一样
    // 这里等于是计算出原有的k值 然后把新的 y 值计算出来 然后再用 k 除以最新的y值 得到新的 x值
    // Calculate new value of X after depositing Y
    // When we swap amount A of Y for X, we must calculate the new balance of X from invariant K
    // Y₂ = Y₁ + Amount
    // X₂ = K / Y₂
    pub fn x2_from_y_swap_amount(x: u64, y: u64, a: u64) -> Result<u64, ProgramError> {
        let k = Self::k_from_xy(x, y)?;
        let x_new = (y as u128)
            .checked_add(a as u128)
            .ok_or(ProgramError::InvalidArgument)?;
        Ok(k.checked_div(x_new).ok_or(ProgramError::InvalidArgument)? as u64)
    }

    // 就是旧的代币数量减去新的代币数量 也就是用户买到的代币数量
    // Calculate the withdraw amount of X from swapping in Y
    // ΔX = X₁ - X₂
    pub fn delta_x_from_y_swap_amount(x: u64, y: u64, a: u64) -> Result<u64, ProgramError> {
        Ok(x.checked_sub(Self::x2_from_y_swap_amount(x, y, a)?)
            .ok_or(ProgramError::InvalidArgument)?)
    }

    // Calculate difference in Y from swapping in X
    // ΔY = Y₁ - Y₂
    pub fn delta_y_from_x_swap_amount(x: u64, y: u64, a: u64) -> Result<u64, ProgramError> {
        Self::delta_x_from_y_swap_amount(y, x, a)
    }

    // 这里就是校验 x 和 y都 不为0 之后 获取 x*y 也就是k的值 然后返回
    // Static Invariant calculation
    pub fn k_from_xy(x: u64, y: u64) -> Result<u128, ProgramError> {
        assert_non_zero!([x, y]);
        Ok((x as u128).checked_mul(y as u128).unwrap())
    }
}
