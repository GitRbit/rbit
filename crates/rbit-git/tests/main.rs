//! All of rbit-git's integration tests, built as one test binary: linking a binary per file
//! costs far more than compiling the tests themselves.

mod archive;
mod blame;
mod changes;
mod compare;
mod edit;
mod insights;
mod merge;
mod overlay;
mod pinned;
mod properties;
mod push;
mod remote;
mod storage;
