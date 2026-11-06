//! Feature-owned account names and email values. Construction preserves existing rules.

mod account_name;
mod email;

pub use account_name::AccountName;
pub use email::Email;

#[cfg(test)]
mod tests;
