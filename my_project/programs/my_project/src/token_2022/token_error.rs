use anchor_lang::prelude::*;

#[error_code]
pub enum Token2022Error {
    #[msg("Name too long (max 32 characters)")]
    NameTooLong,

    #[msg("Symbol too long (max 10 characters)")]
    SymbolTooLong,

    #[msg("URI too long (max 200 characters)")]
    UriTooLong,

    #[msg("Unauthorized update authority")]
    UnauthorizedUpdateAuthority,

    #[msg("Metadata extension not initialized")]
    MetadataNotInitialized,

    #[msg("Invalid metadata pointer")]
    InvalidMetadataPointer,

    #[msg("Extension initialization failed")]
    ExtensionInitializationFailed,

    #[msg("Amount is less than 0")]
    AmountNotAllow,

    #[msg("You have not enough tokens")]
    NotEnoughAmount,

    #[msg("You are not the owner to this account")]
    NotOwner,

    #[msg("Metadata deserialization failed")]
    MetadataDeserializationFailed,
}