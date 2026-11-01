//! Fixed-helper feasibility code, not an operational native-process backend.
use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use std::{
    io,
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant as WallInstant},
};
use tokio::{
    net::unix::pipe::{Receiver, Sender},
    signal::unix::{Signal as ChildSignal, SignalKind, signal},
    time::{Instant, sleep, timeout_at},
};

pub const READY: &[u8] = b"RBIT_ECHO_READY\n";
const ECHO: &str = "printf 'RBIT_ECHO_READY\n'; exec /bin/cat";
const BLOCKED: &str = "printf 'RBIT_BLOCKED_READY\n'; printf '\\000'; while :; do /bin/sleep 1; done";
#[cfg(target_os = "linux")]
const EXIT_FIRST: &str =
    "exec 3<&0; /bin/sh -c 'printf \"RBIT_DESCENDANT_READY\\n\"; exec /bin/cat' <&3 & printf 'RBIT_LEADER_EXITING\n'; exit 0";

/// No caller-controlled script, path, environment or argument enters a helper.
#[derive(Debug)]
pub enum Helper {
    Echo,
    Blocked,
    Protocol,
    BadReady,
    PartialReady,
    Flood,
    #[cfg(target_os = "linux")]
    ExitFirst,
}
impl Helper {
    fn command(self) -> io::Result<Command> {
        let mut command = match self {
            Self::Protocol | Self::BadReady | Self::Flood => Command::new(std::env::current_exe()?),
            _ => Command::new("/bin/sh"),
        };
        command.env_clear().current_dir("/");
        match self {
            Self::Protocol | Self::BadReady | Self::PartialReady | Self::Flood => {
                let mode = match self {
                    Self::Protocol => "good",
                    Self::BadReady => "bad_ready",
                    Self::PartialReady => "partial_ready",
                    Self::Flood => "flood",
                    _ => unreachable!("fixed fixture modes only"),
                };
                if matches!(self, Self::PartialReady) {
                    // No interpolation: executable and every argument are quoted
                    // positional parameters of this fixed shell bootstrap.
                    command.args(["-c", "trap '' TERM; exec \"$@\"", "rbit-fixed-term-bootstrap"]).arg(std::env::current_exe()?);
                }
                command
                    .args(["--exact", "protocol_fixture::protocol_child", "--ignored", "--nocapture", "--test-threads=1"])
                    .env("RBIT_FIXED_PROTOCOL_MODE", mode);
            }
            Self::Echo => {
                command.args(["-c", ECHO]);
            }
            Self::Blocked => {
                command.args(["-c", BLOCKED]);
            }
            #[cfg(target_os = "linux")]
            Self::ExitFirst => {
                command.args(["-c", EXIT_FIRST]);
            }
        }
        Ok(command)
    }
}

pub fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn timeout() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "owned helper cleanup/IO deadline")
}
fn check(end: Instant) -> io::Result<()> {
    if Instant::now() >= end { Err(timeout()) } else { Ok(()) }
}
fn errno(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}

fn owned_error(stage: &str, pgid: Pid, error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{stage} for owned helper group {pgid}: {error}"))
}

#[derive(Debug)]
pub struct Input(Sender);
impl Input {
    pub async fn write(&self, bytes: &[u8], end: Instant) -> io::Result<usize> {
        timeout_at(end, async {
            loop {
                check(end)?;
                tokio::task::consume_budget().await;
                check(end)?;
                self.0.writable().await?;
                check(end)?;
                match self.0.try_write(bytes) {
                    Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => {}
                    result => {
                        check(end)?;
                        return result;
                    }
                }
            }
        })
        .await
        .map_err(|_| timeout())?
    }
}
#[derive(Debug)]
pub struct Output(Receiver);
impl Output {
    pub async fn read(&self, bytes: &mut [u8], end: Instant) -> io::Result<usize> {
        timeout_at(end, async {
            loop {
                check(end)?;
                tokio::task::consume_budget().await;
                check(end)?;
                self.0.readable().await?;
                check(end)?;
                match self.0.try_read(bytes) {
                    Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => {}
                    result => {
                        check(end)?;
                        return result;
                    }
                }
            }
        })
        .await
        .map_err(|_| timeout())?
    }
    pub async fn exact(&self, bytes: &mut [u8], end: Instant) -> io::Result<()> {
        let mut offset = 0;
        while offset < bytes.len() {
            let count = self.read(&mut bytes[offset..], end).await?;
            if count == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            offset += count;
        }
        Ok(())
    }
    pub async fn ack(&self, expected: &[u8]) -> io::Result<()> {
        let mut bytes = vec![0; expected.len()];
        self.exact(&mut bytes, deadline()).await?;
        if bytes != expected {
            return Err(io::Error::other("fixed helper acknowledgement mismatch"));
        }
        Ok(())
    }
    pub async fn eof(&self) -> io::Result<()> {
        let mut byte = [0; 1];
        if self.read(&mut byte, deadline()).await? != 0 {
            return Err(io::Error::other("unexpected helper output before EOF"));
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct Pipes {
    pub input: Input,
    pub output: Output,
    pub error: Output,
}

/// Only an owned std Child supplies this PGID. It is never accepted from a pipe,
/// administrator, filesystem or database. No Tokio Child/wait future is created.
#[derive(Debug)]
pub struct Leader {
    child: Option<Child>,
    pgid: Pid,
    // Safe registration installs a caught SIGCHLD action before the first child.
    // No test in this binary installs another handler or a wildcard reaper.
    _sigchild: ChildSignal,
}
impl Leader {
    pub fn spawn(helper: Helper) -> io::Result<(Self, Pipes)> {
        let sigchild = signal(SignalKind::child())?;
        let mut child = helper
            .command()?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|error| io::Error::new(error.kind(), format!("owned helper spawn: {error}")))?;
        let raw = match i32::try_from(child.id()) {
            Ok(pid) if pid > 1 => pid,
            _ => {
                let _ = child.kill();
                bounded_reap(&mut child);
                return Err(io::Error::other("invalid owned child identity"));
            }
        };
        let mut leader = Self { child: Some(child), pgid: Pid::from_raw(raw), _sigchild: sigchild };
        let pipes = (|| -> io::Result<Pipes> {
            let child = leader.child.as_mut().ok_or_else(|| io::Error::other("missing owned child"))?;
            let input = Input(Sender::from_owned_fd(child.stdin.take().ok_or_else(|| io::Error::other("missing stdin"))?.into())?);
            let output = Output(Receiver::from_owned_fd(child.stdout.take().ok_or_else(|| io::Error::other("missing stdout"))?.into())?);
            let error = Output(Receiver::from_owned_fd(child.stderr.take().ok_or_else(|| io::Error::other("missing stderr"))?.into())?);
            Ok(Pipes { input, output, error })
        })()
        .map_err(|error| owned_error("pipe setup", leader.pgid, &error))?;
        Ok((leader, pipes))
    }
    pub fn is_owned(&self) -> bool {
        self.child.is_some()
    }
    #[cfg(target_os = "linux")]
    pub async fn observe_exit_without_reaping(&mut self) -> io::Result<()> {
        use nix::sys::wait::{Id, WaitPidFlag, WaitStatus, waitid};
        let end = deadline();
        loop {
            let observed = match waitid(Id::Pid(self.pgid), WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT) {
                Ok(status) => status,
                Err(Errno::ECHILD) => {
                    // A foreign reaper/disposition violated the test precondition.
                    // Without waitable ownership, even Drop must not signal PGID.
                    self.child.take();
                    return Err(io::Error::other("lost waitable leader; cleanup unresolved"));
                }
                Err(error) => return Err(errno(error)),
            };
            match observed {
                WaitStatus::Exited(pid, 0) if pid == self.pgid => return Ok(()),
                WaitStatus::StillAlive => {}
                _ => return Err(io::Error::other("unexpected nonreaping leader status")),
            }
            if Instant::now() >= end {
                return Err(timeout());
            }
            sleep(Duration::from_millis(10)).await;
        }
    }
    /// Graceful escalation signal while the owned leader remains unreaped.
    /// No wait is performed here or during the caller's bounded grace window.
    pub fn terminate_group(&self) -> io::Result<()> {
        if self.child.is_none() {
            return Err(io::Error::other("leader already reaped"));
        }
        killpg(self.pgid, Signal::SIGTERM).map_err(errno)
    }
    /// The only force signal occurs while the exclusively owned leader is still
    /// unreaped. Once `try_wait` reaps it, this object can never signal that PGID.
    pub async fn force_and_reap(&mut self) -> io::Result<(ExitStatus, GroupObservation)> {
        if self.child.is_none() {
            return Err(io::Error::other("leader already reaped"));
        }
        match killpg(self.pgid, Signal::SIGKILL) {
            Ok(()) | Err(Errno::ESRCH) => {}
            Err(error) => return Err(owned_error("force signal", self.pgid, &errno(error))),
        }
        self.reap_owned().await
    }
    /// Graceful caller must first establish both pipe EOFs and protocol Stop.
    /// No real group signal is permitted after this reaps the owned leader.
    pub async fn reap_owned(&mut self) -> io::Result<(ExitStatus, GroupObservation)> {
        let end = deadline();
        loop {
            let observed = self.child.as_mut().ok_or_else(|| io::Error::other("missing owned leader"))?.try_wait();
            match observed {
                Ok(Some(status)) => {
                    self.child.take();
                    return Ok((status, GroupObservation(self.pgid)));
                }
                Ok(None) => {}
                Err(error) if error.raw_os_error() == Some(Errno::ECHILD as i32) => {
                    self.child.take();
                    return Err(io::Error::other("lost waitable leader; cleanup unresolved"));
                }
                Err(error) => return Err(owned_error("leader reap", self.pgid, &error)),
            }
            if Instant::now() >= end {
                return Err(timeout());
            }
            sleep(Duration::from_millis(10)).await;
        }
    }
}
/// Presence-only observation. No method can send a real signal after leader reap.
/// Reuse can cause conservative failure, never authority to kill a new process.
#[derive(Debug)]
pub struct GroupObservation(Pid);
impl GroupObservation {
    pub async fn absent(&self) -> io::Result<()> {
        wait_for_absence(deadline(), || killpg(self.0, None::<Signal>))
            .await
            .map_err(|error| owned_error("post-reap presence observation", self.0, &error))
    }
}

async fn wait_for_absence(end: Instant, mut observe: impl FnMut() -> Result<(), Errno>) -> io::Result<()> {
    let mut permission_unknown = false;
    loop {
        if Instant::now() >= end {
            return Err(if permission_unknown { errno(Errno::EPERM) } else { timeout() });
        }
        let observation = observe();
        permission_unknown = matches!(observation, Err(Errno::EPERM));
        if Instant::now() >= end {
            return Err(if permission_unknown { errno(Errno::EPERM) } else { timeout() });
        }
        match observation {
            Err(Errno::ESRCH) => return Ok(()),
            // A permission error does not establish absence. Darwin can also
            // report it while an existing group has no signalable members.
            // Keep observing under the original deadline; only ESRCH succeeds.
            Ok(()) | Err(Errno::EPERM) => {}
            Err(error) => return Err(errno(error)),
        }
        sleep(Duration::from_millis(10)).await;
    }
}

fn bounded_reap(child: &mut Child) {
    let end = WallInstant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if WallInstant::now() < end => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                eprintln!("fixed helper cleanup incomplete; feasibility test cannot certify ownership");
                return;
            }
        }
    }
}
impl Drop for Leader {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Failure/unwind fallback, never converts a failed test into a pass.
            // No successful reap occurred; authority remains tied to this owned leader.
            let _ = killpg(self.pgid, Signal::SIGKILL);
            bounded_reap(&mut child);
        }
    }
}

#[cfg(test)]
mod observations {
    use super::*;

    #[tokio::test]
    async fn permission_unknown_requires_a_later_actual_absence() {
        let mut calls = 0;
        wait_for_absence(deadline(), || {
            calls += 1;
            if calls == 1 { Err(Errno::EPERM) } else { Err(Errno::ESRCH) }
        })
        .await
        .unwrap();
        assert_eq!(calls, 2);
    }

    #[tokio::test]
    async fn permanent_permission_unknown_fails_at_the_deadline() {
        let mut calls = 0;
        let error = wait_for_absence(Instant::now() + Duration::from_millis(10), || {
            calls += 1;
            Err(Errno::EPERM)
        })
        .await
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(calls > 0);
    }

    #[tokio::test]
    async fn permission_unknown_followed_by_presence_never_establishes_absence() {
        let mut calls = 0;
        let error = wait_for_absence(deadline(), || {
            calls += 1;
            match calls {
                1 => Err(Errno::EPERM),
                2 => Ok(()),
                _ => Err(Errno::EINVAL),
            }
        })
        .await
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(calls, 3);
    }
}
