//! Actions application authority independent of SQL, runtime and runner transport.
//!
//! Existing concrete job/task/specification types cross static associated-type ports;
//! adapters retain atomic storage selection and workflow/trust policy compilation.
//! No tasks, queues, dynamic service registry or second record representation live here.
//! [`trust`] owns pull request trust and what a stored run is granted (fork policy).

pub mod archives;
pub mod artifacts;
pub mod caches;
pub mod claims;
pub mod downloads;
pub mod environments;
pub mod logs;
pub mod secrets;
pub mod trust;
