//! Identity-owned workflows against an explicit repository port; real SQL parity remains
//! covered by the unchanged service/storage acceptance suite.

#[path = "account_workflows/authentication.rs"]
mod authentication;
#[path = "account_workflows/fixture.rs"]
mod fixture;
#[path = "account_workflows/original_proof.rs"]
mod original_proof;
#[path = "account_workflows/registration.rs"]
mod registration;

use fixture::*;
