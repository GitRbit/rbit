//! Task logs: `UpdateLog` appends with its acknowledgement protocol, finalising (gzip),
//! reads by step and byte offset, retention and the instance log budget. Files live under
//! `<data_dir>/actions/logs`.
//!
//! Workstream R (runner protocol) owns this file.
