use anchor_lang::prelude::*;
use anchor_spl::token_2022::approve;
use anchor_spl::token_interface::{Approve, Mint, Token2022, TokenAccount};

/// 允许某个第三方（delegate）从你的代币账户（source）中支出最多 amount 的代币。
pub fn handler(ctx: Context<ApproveContext>, amount: u64) -> Result<()> {
    let cpi_accounts = Approve {
        // 被授权操作的账户
        to: ctx.accounts.source_ata.to_account_info(),
        // 被授权的地址（第三方）
        delegate: ctx.accounts.delegate.to_account_info(),
        // 账户所有者必须签名
        authority: ctx.accounts.owner.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    approve(cpi_context, amount)?;
    Ok(())
}

#[derive(Accounts)]
pub struct ApproveContext<'info> {
    // 签名人也就是授权人
    #[account(mut, signer)]
    pub owner: Signer<'info>,
    /// CHECK: 来源账户
    #[account(mut,
    associated_token::mint = mint,
    associated_token::authority = owner,
    associated_token::token_program = token_program,)]
    pub source_ata: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: 铸币账户
    pub mint: InterfaceAccount<'info, Mint>,
    // token2022程序
    pub token_program: Program<'info, Token2022>,
    /// CHECK: 被授权的用户
    pub delegate: SystemAccount<'info>,
}
