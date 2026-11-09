//! Branch protection: rules per branch pattern, the pure decisions made from them, and
//! the management and gate applications that load rules through ports.
//!
//! Rules:
//! * A rule names a [`BranchPattern`] and only concerns `refs/heads/*`.
//! * Every matching rule applies and the strictest value wins: any denial denies, the
//!   highest approval count counts, and pusher restrictions intersect.
//! * `allow_admin_bypass` lets repository administrators pass *that* rule; a matching rule
//!   without it still applies to them. Nothing bypasses the reserved `refs/pull/`
//!   namespace. Refusal texts never say who is allowed.
//! * `require_status_checks` adds the rule's required contexts to what merging needs; it
//!   does not affect pushes.
//! * Only repository administrators manage rules. Allowed users must have write access,
//!   allowed teams must belong to the owning organisation; the store enforces the rule
//!   limit, pattern uniqueness and team ownership atomically.
//!
//! Identifiers and contexts are generic: these rules compare them, never interpret them.

mod application;
mod decision;
mod pattern;
mod port;
mod rules;

pub use application::{Principal, ProtectionGate, RuleDraft, RuleManagement, RuleScope};
pub use decision::{
    BranchChange, Bypass, MergeRules, PULL_REFS, PushDecision, PushVerdict, Pusher, RefChange, Refusal, decide_push, is_protected,
    may_delete, merge_rules, reasons,
};
pub(crate) use pattern::namespace_refusal;
pub use pattern::{BranchPattern, MAX_PATTERN_LEN, PatternError};
pub use port::{RuleDirectory, RuleRepository, StoredInput, StoredRule, Writer};
pub use rules::{
    MAX_ALLOWED, MAX_REQUIRED_APPROVALS, MAX_REQUIRED_CONTEXTS, MAX_RULES, ProtectionId, ProtectionRule, RuleChange, RuleError, RuleInput,
    RuleSettings, summary,
};

#[cfg(test)]
mod tests;
