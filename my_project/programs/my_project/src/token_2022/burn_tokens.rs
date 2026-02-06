use crate::token_2022::token_error::Token2022Error;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{burn_checked, BurnChecked, Mint, Token2022, TokenAccount};

pub fn handler(ctx: Context<BurnTokensContext>, amount: u64) -> Result<()> {
    require!(amount > 0, Token2022Error::AmountNotAllow);
    require!(
        amount <= ctx.accounts.from_ata.amount,
        Token2022Error::NotEnoughAmount
    );
    let cpi_accounts = BurnChecked {
        mint: ctx.accounts.mint.to_account_info(),
        from: ctx.accounts.from_ata.to_account_info(),
        authority: ctx.accounts.burn_authority.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    let decimals = ctx.accounts.mint.decimals;
    burn_checked(cpi_context, amount, decimals)?;

    Ok(())
}

#[derive(Accounts)]
pub struct BurnTokensContext<'info> {
    // 销毁权限拥有者
    #[account(mut)]
    pub burn_authority: Signer<'info>,
    /// CHECK:铸币账户
    #[account(
    mut,
    mint::authority = burn_authority,
    mint::freeze_authority = burn_authority,)]
    pub mint: InterfaceAccount<'info, Mint>,
    /// CHECK: 来源ata账户
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = burn_authority,
        associated_token::token_program = token_program,
    )]
    pub from_ata: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}
