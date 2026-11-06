//! Atomic fixture rendezvous and bounded observations; timeouts are failures.

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::{path::Path, time::Duration};

pub(super) const WAIT: Duration = Duration::from_secs(15);

pub(super) fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

pub(super) async fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    until(|| Ok(path.exists())).await?;
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub(super) async fn until(mut condition: impl FnMut() -> Result<bool>) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        while !condition()? {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("Linux fixture observation timed out; completion is unproved")?
}
