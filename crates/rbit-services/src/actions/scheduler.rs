//! `FetchTask`: the tasks version that answers idle polls without touching SQLite, claiming
//! a waiting job, minting its task token and assembling the task (payload, context,
//! secrets, variables, needs).
//!
//! Workstream R (runner protocol) owns this file.
