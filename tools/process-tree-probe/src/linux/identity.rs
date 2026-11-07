//! Handshake-bound fixture identities and stable Linux pidfd capabilities.

use anyhow::{Context, Result, ensure};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fd::OwnedFd,
    process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Identity {
    pub pid: i32,
    pub pgid: i32,
    pub start_time: u64,
}
impl Identity {
    pub fn capture(pid: i32) -> Result<Self> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
        // comm is parenthesized and may contain spaces or ')' characters.
        let fields: Vec<_> = stat.rsplit_once(')').context("process stat comm missing")?.1.split_whitespace().collect();
        Ok(Self {
            pid,
            pgid: fields.get(2).context("process stat pgrp missing")?.parse()?,
            start_time: fields.get(19).context("process stat starttime missing")?.parse()?,
        })
    }
    pub fn pid(&self) -> Result<Pid> {
        Pid::from_raw(self.pid).context("fixture PID must be positive")
    }
}

#[derive(Debug)]
pub(super) struct FixtureProcess {
    pub identity: Identity,
    fd: OwnedFd,
}
impl FixtureProcess {
    /// The fixture stays behind its startup handshake while this capability opens.
    pub fn open(identity: Identity) -> Result<Self> {
        let fd = pidfd_open(identity.pid()?, PidfdFlags::empty())?;
        ensure!(Identity::capture(identity.pid)? == identity, "fixture identity changed before pidfd adoption");
        Ok(Self { identity, fd })
    }
    pub fn exited(&self) -> Result<bool> {
        let mut polls = [PollFd::new(&self.fd, PollFlags::IN)];
        poll(&mut polls, Some(&Timespec { tv_sec: 0, tv_nsec: 0 }))?;
        ensure!(!polls[0].revents().intersects(PollFlags::ERR | PollFlags::NVAL), "pidfd poll failed");
        Ok(polls[0].revents().contains(PollFlags::IN))
    }
    pub fn kill(&self) -> Result<()> {
        match pidfd_send_signal(&self.fd, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
