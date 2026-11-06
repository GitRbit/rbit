//! Install inherited kill-on-close containment before creating any process/runtime.

use anyhow::{Context, Result};
use std::sync::OnceLock;
use win32job::{ExtendedLimitInfo, Job};

static ROOT: OnceLock<Job> = OnceLock::new();

pub(crate) fn install() -> Result<()> {
    anyhow::ensure!(ROOT.get().is_none(), "root Job installed twice");
    let mut limits = ExtendedLimitInfo::new();
    // All other flags start clear: neither breakaway flag is enabled.
    limits.limit_kill_on_job_close();
    let job = Job::create_with_limit_info(&limits).context("create root Job")?;
    // On failure no child was spawned; fail before runtime/fixture admission.
    job.assign_current_process().context("root Job nesting/assignment failed")?;
    ROOT.set(job).map_err(|_| anyhow::anyhow!("root Job installed twice"))?;
    // Never drop this while running: its kill-on-close membership includes us.
    // OS process termination closes this noninheritable handle even on abrupt death.
    Ok(())
}

pub(crate) fn members() -> Result<Vec<usize>> {
    Ok(ROOT.get().context("root containment not initialized")?.query_process_id_list()?)
}
