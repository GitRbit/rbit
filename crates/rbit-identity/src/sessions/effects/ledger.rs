use rbit_contracts::AccountId;
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

/// Tracks committed password changes while a self-service request is in flight.
/// Notification is synchronous: a guard never claims an event that might arrive later.
#[derive(Debug, Default)]
pub(super) struct PasswordChangeLedger {
    accounts: Mutex<HashMap<AccountId, LedgerEntry>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LedgerEntry {
    /// Guarded requests still running.
    in_flight: u32,
    /// Committed changes while requests were in flight, not yet attributed.
    deferred: u32,
}

/// What a finishing guard must do.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct FinishOutcome {
    pub(super) end_others: bool,
    pub(super) end_all: bool,
}

impl PasswordChangeLedger {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<AccountId, LedgerEntry>> {
        self.accounts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn begin(&self, user: AccountId) {
        self.lock().entry(user).or_default().in_flight += 1;
    }

    pub(super) fn finish(&self, user: AccountId, changed_here: bool) -> FinishOutcome {
        let mut accounts = self.lock();
        let entry = accounts.entry(user).or_default();
        entry.in_flight = entry.in_flight.saturating_sub(1);
        let changed_here = changed_here && entry.deferred > 0;
        if changed_here {
            entry.deferred -= 1;
        }
        let end_all = entry.in_flight == 0 && entry.deferred > 0;
        if end_all {
            entry.deferred = 0;
        }
        if *entry == LedgerEntry::default() {
            accounts.remove(&user);
        }
        FinishOutcome { end_others: changed_here, end_all }
    }

    /// A password change committed; `true` means "end every session now".
    pub(super) fn on_event(&self, user: AccountId) -> bool {
        let mut accounts = self.lock();
        let Some(entry) = accounts.get_mut(&user) else {
            return true;
        };
        let end_all = if entry.in_flight > 0 {
            entry.deferred += 1;
            false
        } else {
            true
        };
        if *entry == LedgerEntry::default() {
            accounts.remove(&user);
        }
        end_all
    }
}

#[cfg(test)]
mod tests;
