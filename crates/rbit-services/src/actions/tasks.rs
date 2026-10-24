//! `UpdateTask`: step states, outputs, heartbeats and results; a final result finishes the
//! task, its job, unblocks dependents and settles the run. Cancellation reaches the runner
//! in the answer.
//!
//! Workstream R (runner protocol) owns this file.
