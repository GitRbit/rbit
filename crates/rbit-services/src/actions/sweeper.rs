//! The 60-second sweep: tasks without heartbeat (zombies), tasks running too long, jobs no
//! runner took, expired logs and runs, stale registration tokens.
//!
//! Workstream R (runner protocol) owns this file.
