//! All of rbit-ssh's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

mod git_over_ssh;
mod permission_matrix;
// The permission regression world shared with rbit-services and rbit-web.
#[path = "../../rbit-services/tests/support/permission_matrix.rs"]
mod permission_world;
mod protection;
mod shutdown;
