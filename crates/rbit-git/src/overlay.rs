//! Object overlays: throw-away bare repositories under `<tmp_dir>/overlays` whose
//! `objects/info/alternates` list two repositories' object directories, so commits of a
//! fork and its parent can be compared without writing to either.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
