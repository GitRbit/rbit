//! Ownership and admission for password/session effects, separate from common reads.

mod enforcement;
mod ledger;
mod scope;

pub use scope::{SessionEffectScope, SessionEffectsError, SessionEffectsReport, SessionMutationContext, SessionMutationPermit};

pub use enforcement::{
    EnforcementError, IdentityPasswordChangeGuard, PasswordChangeEvidence, PasswordChangeEvidenceSource, PasswordSessionEnforcement,
    SessionFinalizer, SessionRevocationEffects,
};
