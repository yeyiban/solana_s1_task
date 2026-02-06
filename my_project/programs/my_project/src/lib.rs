// 引入 anchor 框架的预导入模块
use anchor_lang::prelude::*;

mod token_2022;

use token_2022::*;

declare_id!("BmDK5JuNcvVHotELgRkFQjsbPZ4SLdenvksU4cueWSZE");

// 指令处理逻辑
#[program]
pub mod my_token {
    use super::*;

    pub fn initialize_mint(
        ctx: Context<InitializeMint>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        initialize_mint::handler(ctx, name, symbol, uri)
    }

    pub fn mint_tokens(ctx: Context<MintTokensAccounts>, amount: u64) -> Result<()> {
        mint_tokens::handler(ctx, amount)
    }

    pub fn transfer_tokens(ctx: Context<TransferTokens>, amount: u64) -> Result<()> {
        transfer::handler(ctx, amount)
    }

    pub fn approve(ctx: Context<ApproveContext>, amount: u64) -> Result<()> {
        approve::handler(ctx, amount)
    }

    pub fn update_metadata(
        ctx: Context<UpdateMetadataContext>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        update_metadata::handler(ctx, name, symbol, uri)
    }

    pub fn get_token_info(ctx: Context<GetTokenInfo>) -> Result<TokenInfo> {
        get_token_full_info::handler(ctx)
    }

    pub fn burn_tokens(ctx: Context<BurnTokensContext>, amount: u64) -> Result<()> {
        burn_tokens::handler(ctx, amount)
    }
}
