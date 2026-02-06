use pinocchio::{entrypoint, error::ProgramError, AccountView, Address, ProgramResult};

entrypoint!(process_instruction);

pub mod instructions;
pub use instructions::*;
use solana_address::declare_id;

// 22222222222222222222222222222222222222222222

declare_id!("22222222222222222222222222222222222222222222");

fn process_instruction(
    _program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let (discriminator, data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    match discriminator {
        0 => Deposit::try_from((data, accounts))?.process(),
        1 => Withdraw::try_from(accounts)?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
