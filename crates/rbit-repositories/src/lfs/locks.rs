//! Lock permissions and paging.

use rbit_contracts::AccountId;
use rbit_identity_models::User;

/// Locks per page when the client does not ask for fewer.
pub const MAX_LOCKS_PER_PAGE: u32 = 100;

/// The requested page size, between 1 and [`MAX_LOCKS_PER_PAGE`].
pub fn page_size(limit: Option<u32>) -> u32 {
    limit.unwrap_or(MAX_LOCKS_PER_PAGE).clamp(1, MAX_LOCKS_PER_PAGE)
}

/// The lock id a page starts after; an empty cursor is none.
pub fn parse_cursor(cursor: Option<&str>) -> Result<Option<i64>, String> {
    match cursor.filter(|cursor| !cursor.is_empty()) {
        None => Ok(None),
        Some(cursor) => cursor.parse().map(Some).map_err(|_| "the cursor is not valid".to_owned()),
    }
}

/// A listing's lock id filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockIdFilter {
    Any,
    Id(i64),
    /// A malformed id matches no lock.
    Malformed,
}

/// The listing filter for a requested lock id; an empty id filters nothing.
pub fn list_lock_id(id: Option<&str>) -> LockIdFilter {
    match id.filter(|id| !id.is_empty()) {
        None => LockIdFilter::Any,
        Some(id) => id.parse().map_or(LockIdFilter::Malformed, LockIdFilter::Id),
    }
}

/// The user who may change locks: write access and an account, or `None` (forbidden).
pub fn locker(can_write: bool, user: Option<&User>) -> Option<&User> {
    user.filter(|_| can_write)
}

/// Owners remove their own locks; administrators anyone's with `force`.
pub fn may_unlock(owner: AccountId, user: &User, force: bool, can_admin: bool) -> bool {
    owner == user.id() || (force && can_admin)
}

/// Trims the one extra lock fetched beyond `limit` and returns the next page's cursor.
pub fn next_cursor<T>(locks: &mut Vec<T>, limit: u32, id: impl Fn(&T) -> i64) -> Option<String> {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if locks.len() <= limit {
        return None;
    }
    locks.truncate(limit);
    locks.last().map(|lock| id(lock).to_string())
}
