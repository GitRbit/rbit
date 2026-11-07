//! Actions application authority independent of SQL, runtime and runner transport.
//!
//! Existing concrete job/task/specification types cross static associated-type ports;
//! adapters retain atomic storage selection and workflow/trust policy compilation.
//! No tasks, queues, dynamic service registry or second record representation live here.

pub mod claims;
