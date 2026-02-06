use crate::token_2022::token_error::Token2022Error;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{transfer_checked, Token2022, TokenAccount, TransferChecked, Mint};

pub fn handler(ctx: Context<TransferTokens>, amount: u64) -> Result<()> {
    // 校验
    require_keys_eq!(
        ctx.accounts.from_ata.owner,
        ctx.accounts.authority.key(),
        Token2022Error::NotOwner
    );
    require_gt!(amount, 0, Token2022Error::AmountNotAllow);
    require!(
        amount <= ctx.accounts.from_ata.amount,
        Token2022Error::NotEnoughAmount
    );
    // 构建交易参数
    let token_program = ctx.accounts.token_program.to_account_info();
    let transfer = TransferChecked {
        from: ctx.accounts.from_ata.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.to_ata.to_account_info(),
        authority: ctx.accounts.authority.to_account_info(),
    };
    let cpi_context = CpiContext::new(token_program, transfer);
    let decimals = ctx.accounts.mint.decimals;
    transfer_checked(cpi_context, amount, decimals)?;
    Ok(())
}

#[derive(Accounts)]
pub struct TransferTokens<'info> {
    // 权限账户 也就是当前from_ata账户的钱包地址
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK:来源
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = authority,
        associated_token::token_program = token_program,
    )]
    pub from_ata: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: 去处 这里考虑如果接收人没有ata的情况 转账人付接收方 ATA 创建费
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = mint,
        associated_token::authority = to,
        associated_token::token_program = token_program,
    )]
    pub to_ata: InterfaceAccount<'info, TokenAccount>,
    // 接收人钱包地址
    pub to: SystemAccount<'info>,
    /// CHECK:铸币账户
    pub mint: InterfaceAccount<'info, Mint>,
    // TOKEN 2022程序
    pub token_program: Program<'info, Token2022>,
    // 系统账户
    pub system_program: Program<'info, System>,
    // 创建ATA必须的账户
    pub associated_token_program: Program<'info, AssociatedToken>,
}
