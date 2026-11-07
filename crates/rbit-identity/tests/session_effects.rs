//! Deterministic ownership/cancellation gates, independent of real storage timing.

#[path = "session_effects/enforcement.rs"]
mod enforcement;
#[path = "session_effects/scope.rs"]
mod scope;

#[path = "session_effects/value.rs"]
mod value;
