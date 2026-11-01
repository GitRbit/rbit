//! Test-only ownership probe. It is not a runtime manager or admission policy.
use super::support::{Helper, Leader, Output, Pipes, deadline};
use nix::sys::signal::Signal;
use std::sync::Arc;
use std::{
    future::{Future, poll_fn},
    io,
    os::unix::process::ExitStatusExt,
    process::ExitStatus,
    task::Poll,
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, oneshot, watch},
    task::JoinHandle,
    time::timeout,
};

#[derive(Debug)]
struct Caller {
    _response: oneshot::Receiver<()>,
    cancel: watch::Sender<bool>,
}
impl Drop for Caller {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
    }
}

/// Uncertainty/unwind closes admission before its permit can be released.
#[derive(Debug)]
struct Admission {
    semaphore: Arc<Semaphore>,
    _permit: OwnedSemaphorePermit,
    settled: bool,
}
impl Drop for Admission {
    fn drop(&mut self) {
        if !self.settled {
            self.semaphore.close();
        }
    }
}

#[derive(Debug)]
struct Supervisor {
    task: Option<JoinHandle<io::Result<ExitStatus>>>,
    admission: Arc<Semaphore>,
}
impl Supervisor {
    async fn join(&mut self) -> io::Result<ExitStatus> {
        let task = self.task.as_mut().ok_or_else(|| io::Error::other("owner already joined"))?;
        if let Ok(joined) = timeout(Duration::from_secs(30), &mut *task).await {
            let result = joined.map_err(|_| io::Error::other("owner task failed"))?;
            if result.is_err() {
                self.admission.close();
            }
            self.task.take();
            result
        } else {
            // Failure-only containment: this never counts as settlement.
            self.admission.close();
            task.abort();
            if timeout(Duration::from_secs(5), &mut *task).await.is_ok() {
                self.task.take();
            }
            Err(io::Error::other("owner join deadline; cleanup unresolved"))
        }
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            self.admission.close();
            // Assertion/unwind fallback cannot await or certify task cleanup.
            task.abort();
        }
    }
}

#[derive(Debug)]
struct Checkpoints {
    pending: oneshot::Receiver<()>,
    precleanup: oneshot::Receiver<()>,
    release: Option<oneshot::Sender<()>>,
}
impl Drop for Checkpoints {
    fn drop(&mut self) {
        // Do not strand the finite, explicitly injected pre-cleanup gate.
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

fn start(admission: &Arc<Semaphore>) -> io::Result<(Caller, Supervisor, Checkpoints)> {
    let permit =
        admission.clone().try_acquire_owned().map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "probe admission unavailable"))?;
    let guard = Admission { semaphore: admission.clone(), _permit: permit, settled: false };
    let (cancel, control) = watch::channel(false);
    let (response, receive) = oneshot::channel();
    let (pending, pending_read) = oneshot::channel();
    let (precleanup, at_precleanup) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let task = tokio::spawn(own(guard, control, response, pending, precleanup, released));
    Ok((
        Caller { _response: receive, cancel },
        Supervisor { task: Some(task), admission: admission.clone() },
        Checkpoints { pending: pending_read, precleanup: at_precleanup, release: Some(release) },
    ))
}

async fn wait_for_cancellation(output: &Output, mut cancel: watch::Receiver<bool>, pending: oneshot::Sender<()>) -> io::Result<()> {
    let mut byte = [0; 1];
    let read = output.read(&mut byte, deadline());
    tokio::pin!(read);
    let mut pending = Some(pending);
    let observed = poll_fn(|cx| match read.as_mut().poll(cx) {
        Poll::Pending => {
            if let Some(pending) = pending.take() {
                let _ = pending.send(());
            }
            Poll::Pending
        }
        ready @ Poll::Ready(_) => ready,
    });
    tokio::select! {
        changed = cancel.changed() => {
            let _ = changed;
            if *cancel.borrow() { Ok(()) } else { Err(io::Error::other("probe control closed without cancellation")) }
        }
        _ = observed => Err(io::Error::other("expected pending pipe until caller cancellation")),
    }
}

async fn settle(leader: &mut Leader, pipes: &Pipes) -> io::Result<ExitStatus> {
    let (status, group) = leader.force_and_reap().await?;
    group.absent().await?;
    pipes.output.eof().await.map_err(|error| io::Error::new(error.kind(), format!("waiter stdout EOF: {error}")))?;
    pipes.error.eof().await.map_err(|error| io::Error::new(error.kind(), format!("waiter stderr EOF: {error}")))?;
    Ok(status)
}

async fn own(
    mut admission: Admission,
    cancel: watch::Receiver<bool>,
    response: oneshot::Sender<()>,
    pending: oneshot::Sender<()>,
    precleanup: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
) -> io::Result<ExitStatus> {
    let (mut leader, pipes) = Leader::spawn(Helper::Blocked)?;
    pipes.output.ack(b"RBIT_BLOCKED_READY\n").await?;
    let mut prefix = [0; 1];
    pipes.output.exact(&mut prefix, deadline()).await?;
    let stage = async {
        wait_for_cancellation(&pipes.output, cancel, pending).await?;
        let _ = precleanup.send(());
        // Explicit test injection, not simulated process cleanup.
        timeout(Duration::from_secs(5), release)
            .await
            .map_err(|_| io::Error::other("pre-cleanup gate deadline"))?
            .map_err(|_| io::Error::other("pre-cleanup gate closed"))
    }
    .await;
    // Even a failed gate/cancellation observation attempts real settlement.
    let status = settle(&mut leader, &pipes).await?;
    stage?;
    admission.settled = true;
    let _ = response.send(()); // Lost caller response does not change settlement.
    Ok(status)
}

async fn checkpoint(receiver: &mut oneshot::Receiver<()>) {
    timeout(Duration::from_secs(5), receiver).await.expect("checkpoint deadline").expect("checkpoint owner disappeared");
}

#[tokio::test]
async fn lost_waiter_keeps_admission_through_actual_process_settlement_in_fixed_cycles() {
    let admission = Arc::new(Semaphore::new(1)); // Harness choice, not a host policy.
    for _ in 0..2 {
        let (caller, mut owner, mut checkpoints) = start(&admission).unwrap();
        checkpoint(&mut checkpoints.pending).await;
        assert!(matches!(admission.clone().try_acquire_owned(), Err(tokio::sync::TryAcquireError::NoPermits)));
        assert_eq!(start(&admission).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        drop(caller); // Receiver loss requests cancellation; it cannot abort owner.
        checkpoint(&mut checkpoints.precleanup).await;
        assert!(!admission.is_closed());
        assert!(matches!(admission.clone().try_acquire_owned(), Err(tokio::sync::TryAcquireError::NoPermits)));
        assert_eq!(start(&admission).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        checkpoints.release.take().unwrap().send(()).unwrap();
        assert_eq!(owner.join().await.unwrap().signal(), Some(Signal::SIGKILL as i32));
        assert!(!admission.is_closed());
        assert_eq!(admission.available_permits(), 1);
    }
}

#[tokio::test]
async fn failed_injected_gate_closes_admission_even_after_attempted_real_cleanup() {
    let admission = Arc::new(Semaphore::new(1));
    let (caller, mut owner, mut checkpoints) = start(&admission).unwrap();
    checkpoint(&mut checkpoints.pending).await;
    drop(caller);
    checkpoint(&mut checkpoints.precleanup).await;
    // Deliberate test-coordination failure, not a simulated OS cleanup success.
    drop(checkpoints.release.take().unwrap());
    let error = owner.join().await.unwrap_err();
    assert_eq!(error.to_string(), "pre-cleanup gate closed");
    assert!(admission.is_closed());
    assert!(matches!(admission.clone().try_acquire_owned(), Err(tokio::sync::TryAcquireError::Closed)));
    assert_eq!(start(&admission).unwrap_err().kind(), io::ErrorKind::WouldBlock);
}
