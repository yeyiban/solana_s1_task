use anchor_lang::prelude::*;
use anchor_spl::{
    metadata::{
        mpl_token_metadata::types::DataV2, update_metadata_accounts_v2, Metadata,
        UpdateMetadataAccountsV2,MetadataAccount
    },
    token_interface::Mint,
};

pub fn handler(
    ctx: Context<UpdateMetadataContext>,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    // 设置更新信息
    UpdateMetadataAccountsV2 {
        metadata: ctx.accounts.metadata.to_account_info(),
        update_authority: ctx.accounts.update_authority.to_account_info(),
    };
    let new_metadata = DataV2 {
        name,
        symbol,
        uri,
        seller_fee_basis_points: 0,
        creators: None,
        collection: None,
        uses: None,
    };
    let cpi_accounts = UpdateMetadataAccountsV2 {
        metadata: ctx.accounts.metadata.to_account_info(),
        update_authority: ctx.accounts.update_authority.to_account_info(),
    };
    let cpi_program = ctx.accounts.metadata_program.to_account_info();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    // 第一个参数为cpi_context
    // 第二个参数为是否更新metadata更新权限 这里不更新 就输入none 如果需要更新就在参数里面添加一个 new_authority 类型为 Pubkey 的参数
    // 第三个参数为新的信息
    update_metadata_accounts_v2(cpi_context, None, Some(new_metadata), None, None)?;
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateMetadataContext<'info> {
    // 签名人 也就是在创建 metadata 账户的时候的 update_authority 权限拥有者
    // 目前程序设置的是mint账户的发布人
    #[account(mut)]
    pub update_authority: Signer<'info>,
    // 元数据账户
    #[account(mut,
        seeds = [b"metadata", metadata_program.key().as_ref(), mint.key().as_ref()],
        seeds::program = metadata_program.key(),
        bump,)]
    pub metadata: Account<'info, MetadataAccount>,
    // 元数据程序
    pub metadata_program: Program<'info, Metadata>,
    // mint程序
    pub mint: InterfaceAccount<'info, Mint>,
}
