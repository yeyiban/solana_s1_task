use anchor_lang::prelude::*;
use anchor_spl::metadata::mpl_token_metadata::accounts::Metadata;
use anchor_spl::token_interface::Mint;
use crate::token_2022::token_error::Token2022Error;

pub fn handler(ctx: Context<GetTokenInfo>) -> Result<TokenInfo> {
    // 获取Mint信息
    let mint = ctx.accounts.mint.clone().into_inner();

    // 获取元数据（如果存在）
    // 解析metadata账户
    let metadata_data = ctx.accounts.metadata.try_borrow_data()
        .map_err(|_| error!(Token2022Error::MetadataDeserializationFailed))?;
    let metadata = Metadata::deserialize(&mut &metadata_data[..])
        .map_err(|_| error!(Token2022Error::MetadataDeserializationFailed))?;

    msg!("metadata:{:p}", &metadata);

    Ok(TokenInfo {
        decimals: mint.decimals,
        total_supply: mint.supply,
        name: metadata.name,
        symbol: metadata.symbol,
        uri: metadata.uri,
        mint_address: ctx.accounts.mint.key(),
        metadata_address: ctx.accounts.metadata.key(),
    })
}

#[derive(Accounts)]
pub struct GetTokenInfo<'info> {
    /// CHECK: 铸币账户
    pub mint: InterfaceAccount<'info, Mint>,
    /// CHECK: 元信息账户
    #[account(
        seeds = [b"metadata", metadata_program.key().as_ref(), mint.key().as_ref()],
        seeds::program = metadata_program.key(),
        bump,
    )]
    pub metadata: AccountInfo<'info>,
    // 元信息程序
    pub metadata_program: Program<'info, anchor_spl::metadata::Metadata>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct TokenInfo {
    pub decimals: u8,
    pub total_supply: u64,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub mint_address: Pubkey,
    pub metadata_address: Pubkey,
}