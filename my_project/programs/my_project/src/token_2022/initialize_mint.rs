use crate::token_2022::token_error::Token2022Error;
use anchor_lang::prelude::*;
use anchor_spl::{
    metadata::{CreateMetadataAccountsV3,
               create_metadata_accounts_v3,
               Metadata,
               MetadataAccount,
               mpl_token_metadata::types::DataV2,
    },
    token_interface::{ Mint, Token2022},
};

pub fn handler(
    ctx: Context<InitializeMint>,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    msg!("Initializing Token-2022 mint with metadata...");
    // 验证字符串长度
    require!(name.len() <= 32, Token2022Error::NameTooLong);
    require!(symbol.len() <= 10, Token2022Error::SymbolTooLong);
    require!(uri.len() <= 200, Token2022Error::UriTooLong);
    // 记录日志
    msg!(
            "Mint created: {} (decimals: 6, authority: {})",
            ctx.accounts.mint.key(),
            ctx.accounts.payer.key()
    );

    // 创建元数据
    // 对于solana来说 是 token 还是NFT没有区别
    // 在代码中的区别就是精度 NFT进度为0也就是不可以为小数 固定为整数
    // 另外就是发行量，如果是单个NFT就是发行量为1
    let metadata = DataV2 {
        name,
        symbol,
        uri,
        // 卖家费用基点。这个值表示二级市场交易时卖家需要支付的费用。通常以基点为单位，0 表示不收取任何费用。
        seller_fee_basis_points: 0,
        // NFT 的创作者信息。这个字段是一个可选的数组，存储了一个或多个创作者的账户信息以及他们的创作份额。在本例中，设为 None，即不指定具体的创作者。
        creators: None,
        // 该 NFT 是否属于某个集合。None 表示它不属于任何特定集合。如果指定了集合，则可以对该集合内的所有 NFT 进行批量操作。
        collection: None,
        // NFT 的使用场景。可以为 NFT 定义使用规则，例如游戏中的道具次数、访问某些活动的权限等。在本例中，设置为 None，表示没有特定的使用规则。
        uses: None,
    };

    let metadata_program = ctx.accounts.metadata_program.to_account_info();
    let create_metadata_accounts = CreateMetadataAccountsV3 {
        metadata: ctx.accounts.metadata.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        mint_authority: ctx.accounts.payer.to_account_info(),
        payer: ctx.accounts.payer.to_account_info(),
        update_authority: ctx.accounts.payer.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        rent: ctx.accounts.rent.to_account_info(),
    };
    let cpi_context = CpiContext::new(metadata_program, create_metadata_accounts, /* &[&[&[u8]]] */);
    create_metadata_accounts_v3(cpi_context, metadata, true, true, None)?;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeMint<'info> {
    ///  CHECK:铸币账户 这里铸币和冻结权限给了签名人 但是也可以指定给其他人
    // 由于需要创建账户 所以需要引入System账户
    #[account(
        init,
        seeds = [b"mint", payer.key().as_ref()],
        bump,
        payer = payer,
        mint::decimals = 6,
        mint::authority = payer,
        mint::freeze_authority = payer,
        mint::token_program = token_program,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    // token元数据账户
    #[account(
        mut,
        seeds = [b"metadata", metadata_program.key().as_ref(), mint.key().as_ref()],
        seeds::program = metadata_program.key(),
        bump,
    )]
    pub metadata: Account<'info, MetadataAccount>,

    // Token程序
    pub token_program: Program<'info, Token2022>,
    // 签名人
    #[account(mut, signer)]
    pub payer: Signer<'info>,
    // 系统账户
    pub system_program: Program<'info, System>,
    // 元数据程序
    pub metadata_program: Program<'info, Metadata>,
    // 租金程序
    pub rent: Sysvar<'info, Rent>,
}
