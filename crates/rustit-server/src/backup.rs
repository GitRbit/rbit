//! `rustit backup`: a consistent snapshot of the database, key/value store and repositories.
//!
//! Milestone 1 workstream O owns this file.

use std::path::PathBuf;

use rustit_core::Config;

pub async fn run(config: Config, output: PathBuf) -> anyhow::Result<()> {
    let _ = (config, output);
    anyhow::bail!("backups are not implemented yet")
}
