//! `[editor]`: editing and uploading files in the browser.
//!
//! ```toml
//! [editor]
//! # Largest file that may be uploaded through the web interface, in bytes.
//! max_upload_bytes = 52428800
//! # Files one upload (one commit) may add.
//! max_upload_files = 100
//! # Upper bound for one upload; uploads bypass the 30-second page timeout.
//! upload_timeout_seconds = 600
//! ```
//!
//! Text edits are limited by `[repository] max_render_bytes` instead: the editor is only
//! offered for files the browser would display, and new or edited text may be at most that
//! large. Uploaded files stream into a scratch repository under `<data_dir>/tmp/merges`
//! (see `rbit_git::edit`), never into memory.
//!
//! Milestone 3 workstream E (editing files in the browser) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorConfig {
    /// Largest uploaded file, in bytes.
    pub max_upload_bytes: u64,
    /// Files per upload.
    pub max_upload_files: usize,
    /// Upper bound, in seconds, for one upload request.
    pub upload_timeout_seconds: u64,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self { max_upload_bytes: 50 * 1024 * 1024, max_upload_files: 100, upload_timeout_seconds: 10 * 60 }
    }
}
