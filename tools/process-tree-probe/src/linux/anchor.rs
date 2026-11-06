//! A std Child retains the group leader until the final group signal is complete.

use super::identity::Identity;
use anyhow::{Context, Result, ensure};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, getpgrp, kill_process_group, waitid};
use std::process::{Child, ExitStatus};

#[derive(Debug)]
struct SignalAuthority {
    anchor_owned: bool,
}
impl SignalAuthority {
    fn check(&self) -> Result<()> {
        ensure!(self.anchor_owned, "group signal refused: anchor already consumed or lost");
        Ok(())
    }
    fn revoke(&mut self) {
        self.anchor_owned = false;
    }
}

#[derive(Debug)]
pub(super) struct AnchoredGroup {
    child: Option<Child>,
    pid: Pid,
    authority: SignalAuthority,
}
impl AnchoredGroup {
    pub fn new(child: Child) -> Result<Self> {
        let pid = Pid::from_raw(i32::try_from(child.id())?).context("invalid child PID")?;
        ensure!(pid != Pid::INIT && pid != getpgrp(), "unsafe process group target");
        Ok(Self { child: Some(child), pid, authority: SignalAuthority { anchor_owned: true } })
    }
    pub fn identity(&self) -> Result<Identity> {
        Identity::capture(self.pid.as_raw_nonzero().get())
    }
    pub fn take_stdout(&mut self) -> Result<std::process::ChildStdout> {
        self.child.as_mut().and_then(|child| child.stdout.take()).context("fixture stdout missing")
    }
    /// Never reaps. ECHILD is lost authority, never evidence of an empty group.
    pub fn exited_waitable(&mut self) -> Result<bool> {
        self.authority.check()?;
        loop {
            match waitid(WaitId::Pid(self.pid), WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG) {
                Ok(status) => return Ok(status.is_some()),
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => {
                    self.authority.revoke();
                    return Err(error.into());
                }
            }
        }
    }
    pub fn signal(&self, signal: Signal) -> Result<()> {
        self.authority.check()?;
        kill_process_group(self.pid, signal)?;
        Ok(())
    }
    /// Consumes all group-signal authority BEFORE wait can release the leader PID.
    pub fn reap(mut self) -> Result<ExitStatus> {
        self.authority.revoke();
        Ok(self.child.take().context("anchor child missing")?.wait()?)
    }
}
impl Drop for AnchoredGroup {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Failure cleanup may request a kill only while the unreaped anchor is
            // still owned. A failed kill/wait is not a successful completion claim.
            if self.authority.check().is_ok() {
                let _ = kill_process_group(self.pid, Signal::Kill);
            }
            self.authority.revoke();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_or_lost_anchor_refuses_a_reused_numeric_group_before_any_signal() {
        let mut authority = SignalAuthority { anchor_owned: true };
        let mut calls = 0;
        if authority.check().is_ok() {
            calls += 1;
        }
        authority.revoke();
        // The numeric PGID might now identify an unrelated process. No caller can
        // recreate authority from that number, and checks never invoke kill.
        for _ in 0..100 {
            assert!(authority.check().is_err());
        }
        assert_eq!(calls, 1);
    }
}
