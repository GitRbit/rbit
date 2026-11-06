//! Private, atomic filesystem rendezvous between separately contained native fixtures.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{path::Path, time::Duration};

pub(crate) const WAIT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Identity {
    pub pid: u32,
    pub start_time: u64,
}

impl Identity {
    pub fn capture(pid: u32) -> Result<Self> {
        let info = processkit::process_info(pid)?.context("fixture process disappeared before identity capture")?;
        Ok(Self { pid, start_time: info.start_time().context("Windows process identity has no creation time")? })
    }

    pub fn alive(&self) -> Result<bool> {
        Ok(processkit::process_is_alive(self.pid, Some(self.start_time))?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Ready {
    pub parent: Identity,
    pub descendants: Vec<Identity>,
    pub inherited_members: Vec<usize>,
    pub before_nested_assignment: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Evidence {
    pub scenario: String,
    #[serde(flatten)]
    pub containment: Containment,
    #[serde(flatten)]
    pub completion: Completion,
    pub root_wait_alone_insufficient: bool,
    pub admission_closed: bool,
    pub unrelated_survived: bool,
    pub disposition: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Containment {
    pub nested_host_compatible: bool,
    pub before_nested_inherited: bool,
    pub root_handle_noninheritance_behavior: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Completion {
    pub members_empty: bool,
    pub child_reaped: bool,
    pub pumps_destroyed: bool,
}

pub(crate) fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

pub(crate) async fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    tokio::time::timeout(WAIT, async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Ok::<T, anyhow::Error>(serde_json::from_slice(&std::fs::read(path)?)?)
    })
    .await
    .context("fixture rendezvous timed out")?
}

pub(crate) async fn until(mut condition: impl FnMut() -> Result<bool>) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        while !condition()? {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("native process proof did not complete")?
}
