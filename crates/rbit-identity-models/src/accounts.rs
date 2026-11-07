//! Account DTOs and static persistence ports without password operations.

mod creation;
mod provider;
mod records;
mod repository;

pub use creation::{AccountCreationRepository, CreatedAccountSnapshot};
pub use provider::{LinkedProviderSnapshot, ProviderAccountSnapshot, ProviderCreationRepository, ProviderLoginRepository};
pub use records::{LoginRecord, NewUser, SignUp};
pub use repository::AccountRepository;
