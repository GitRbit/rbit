//! Runners (§11.9, §12.6): registration-token terms, who manages runners, `register` and
//! `declare`, label pins and changes, and the in-memory bookkeeping rules (last seen,
//! recent claim keys). Credentials, the runner cache, storage and audit are composition's,
//! reached through the static ports of [`protocol`].

mod policy;
mod protocol;

pub use self::policy::{
    DEFAULT_REGISTRATION_LIFETIME, MAX_REGISTRATION_LIFETIME, ManagedTarget, Management, REMEMBERED_KEYS, RecentKeys, RegistrationOptions,
    SEEN_GRANULARITY_SECONDS, intervals, labels_changed, listed_for_repository, management, seen_is_due, shown_online, unpinned_label,
};
pub use self::protocol::{
    Declared, Registration, RunnerDeclaration, RunnerLabels, RunnerRefusal, RunnerRegistration, RunnerRegistry, UNKNOWN_CREDENTIAL,
    UNKNOWN_REGISTRATION, declared,
};
