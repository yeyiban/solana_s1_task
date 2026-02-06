use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{mint_to, Mint, MintTo, Token2022, TokenAccount};
use crate::token_2022::token_error::Token2022Error;

pub fn handler(ctx: Context<MintTokensAccounts>, amount: u64) -> Result<()> {
    // 校验铸造金额
    require_gt!(amount, 0, Token2022Error::AmountNotAllow);
    // 注意接受者为 ata 账户
    let mint_info = MintTo {
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.ata.to_account_info(),
        authority: ctx.accounts.authority.to_account_info(),
    };
    let account_info = ctx.accounts.token_program.to_account_info();
    let cpi_context = CpiContext::new(account_info, mint_info);
    // 铸币
    mint_to(cpi_context, amount)?;
    Ok(())
}

#[derive(Accounts)]
pub struct MintTokensAccounts<'info> {
    // 签名账户 也就是有权限铸造的账户
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: 铸币账户
    #[account(mut, mint::authority = authority)]
    pub mint: InterfaceAccount<'info, Mint>,
    // 接受者账户
    pub receiver: SystemAccount<'info>,

    /// CHECK: 接受者ATA账户
    #[account(
    init_if_needed,
    payer = authority,
    associated_token::mint = mint,
    associated_token::authority = receiver,
    associated_token::token_program = token_program,)]
    pub ata: InterfaceAccount<'info, TokenAccount>,
    // token程序
    pub token_program: Program<'info, Token2022>,
    // 创建ATA账户需要
    pub associated_token_program: Program<'info, AssociatedToken>,
    // 因为可能需要创建账户 所以引入
    pub system_program: Program<'info, System>,
}
