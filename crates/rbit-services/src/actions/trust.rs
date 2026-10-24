//! Whether a pull request run is trusted (§11.1 of the Actions design), from the actor's
//! current access, the repository's trusted users and earlier merged pull requests; the pure
//! decision is `rbit_core::actions::classify_pull_request`.
//!
//! Workstream T (triggers and runs) owns this file.
