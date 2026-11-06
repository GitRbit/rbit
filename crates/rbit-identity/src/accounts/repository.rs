//! Static persistence port; implementations own transaction/race authority.

use std::future::Future;

use super::{LoginRecord, NewUser};
use crate::User;

pub trait AccountRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn count_users(&self) -> impl Future<Output = Result<i64, Self::Error>> + Send;
    fn find_login(&self, login: &str) -> impl Future<Output = Result<Option<LoginRecord>, Self::Error>> + Send;

    /// Creates an explicitly provisioned user in one account/login transaction.
    fn create_user(&self, new: NewUser) -> impl Future<Output = Result<User, Self::Error>> + Send;
    /// Creates an administrator-provisioned user requiring an initial password change.
    fn create_user_with_initial_password(&self, new: NewUser) -> impl Future<Output = Result<User, Self::Error>> + Send;
    /// Inside the write transaction, chooses the first administrator and rechecks closed
    /// registration. `new.is_admin` is ignored. A preflight count is never authority.
    fn register_user(&self, new: NewUser, registration_open: bool) -> impl Future<Output = Result<User, Self::Error>> + Send;
}
