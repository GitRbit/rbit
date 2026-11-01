//! Pure state/ownership tests, never SQL/IO settlement or executable proof.
use super::*;
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(2);
fn deadline() -> Instant {
    Instant::now() + WAIT
}
async fn pending<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending(), "expected genuine pending acquisition/drain");
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn suspended_enrollment_releases_serial_but_never_creates_a_clean_gap() {
    let domain = Domain::new();
    let writer = domain.writer();
    let suspended = writer.begin().await.unwrap().suspend();
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false, writers_stopped: false });
    let other = timeout(WAIT, writer.begin()).await.unwrap().unwrap();
    assert_eq!(domain.snapshot().active, 2);
    other.complete();
    let mut drain = Box::pin(domain.drain(deadline()));
    pending(drain.as_mut()).await;
    assert_eq!(domain.snapshot().active, 1);
    // Drain must release its temporarily acquired serial permit before waiting,
    // otherwise the existing enrollment could never resume to settle.
    let resumed = timeout(WAIT, suspended.resume()).await.unwrap();
    assert_eq!(domain.snapshot().active, 1);
    pending(drain.as_mut()).await;
    resumed.complete();
    drop(timeout(WAIT, drain).await.unwrap().unwrap());
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: false });
}

#[tokio::test]
async fn terminal_stop_refuses_old_new_tickets_but_allows_same_acquired_resume() {
    let domain = Domain::new();
    let writer = domain.writer();
    let first = writer.begin().await.unwrap();
    let mut old = Box::pin(writer.begin());
    pending(old.as_mut()).await;
    let suspended = first.suspend(); // Old queued future can now be granted but is unpolled.
    let shutdown = domain.stop_writers();
    let mut drained = Box::pin(shutdown.drain(deadline()));
    pending(drained.as_mut()).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 1, uncertain: false, writers_stopped: true });
    let mut resume = Box::pin(suspended.resume());
    pending(resume.as_mut()).await;
    assert!(matches!(old.await, Err(BeginError::Stopped)));
    let resumed = timeout(WAIT, resume).await.unwrap();
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false, writers_stopped: true });
    assert!(matches!(writer.begin().await, Err(BeginError::Stopped)));
    pending(drained.as_mut()).await;
    resumed.complete();
    drop(timeout(WAIT, drained).await.unwrap().unwrap());
    assert!(matches!(domain.writer().begin().await, Err(BeginError::Stopped)));
}

#[tokio::test]
async fn canceled_pending_resume_is_uncertain_without_new_ticket_or_double_decrement() {
    let domain = Domain::new();
    let writer = domain.writer();
    let suspended = writer.begin().await.unwrap().suspend();
    let blocker = writer.begin().await.unwrap();
    let mut resume = Box::pin(suspended.resume());
    pending(resume.as_mut()).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 2, queued: 0, uncertain: false, writers_stopped: false });
    drop(resume);
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: true, writers_stopped: false });
    blocker.complete();
    assert_eq!(domain.snapshot().active, 0);
    writer.begin().await.unwrap().suspend().resume().await.complete();
    assert!(domain.snapshot().uncertain, "later acknowledged resumed work cannot erase lost continuation");
    assert!(matches!(domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    let stopped = domain.stop_writers();
    assert!(matches!(stopped.drain(deadline()).await, Err(DrainError::Uncertain)));
}

#[tokio::test]
async fn ignored_suspended_unpolled_resume_and_resumed_error_are_all_uncertain() {
    for mode in 0..3 {
        let domain = Domain::new();
        let suspended = domain.writer().begin().await.unwrap().suspend();
        match mode {
            0 => drop(suspended),
            1 => drop(suspended.resume()), // The unpolled future still owns the acquired enrollment.
            _ => drop(suspended.resume().await),
        }
        assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        domain.writer().begin().await.unwrap().complete();
        assert!(matches!(domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    }
}

#[tokio::test]
async fn both_drain_deadlines_leave_suspended_ownership_counted_until_actual_completion() {
    let domain = Domain::new();
    let suspended = domain.writer().begin().await.unwrap().suspend();
    let end = Instant::now() + Duration::from_millis(20);
    assert!(matches!(domain.drain(end).await, Err(DrainError::Deadline)));
    assert!(Instant::now() >= end);
    assert_eq!(domain.snapshot().active, 1);
    let stopped = domain.stop_writers();
    let end = Instant::now() + Duration::from_millis(20);
    assert!(matches!(stopped.drain(end).await, Err(DrainError::Deadline)));
    assert!(Instant::now() >= end);
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false, writers_stopped: true });
    suspended.resume().await.complete();
    assert!(matches!(stopped.drain(Instant::now()).await, Err(DrainError::Deadline)));
    drop(stopped.drain(deadline()).await.unwrap());
}

#[tokio::test]
async fn coalesced_follower_acknowledgement_keeps_leader_active_and_serial_until_last() {
    let domain = Domain::new();
    let writer = domain.writer();
    let leader = writer.begin().await.unwrap().suspend();
    let follower = writer.begin().await.unwrap().suspend();
    assert_eq!(domain.snapshot().active, 2);
    let leader = leader.resume().await;
    let stopped = domain.stop_writers();
    let mut quiet = Box::pin(domain.drain(deadline()));
    pending(quiet.as_mut()).await;
    let mut drained = Box::pin(stopped.drain(deadline()));
    pending(drained.as_mut()).await;
    // Pure metadata model of an already acknowledged zero-IO batch: a trusted
    // host would verify bound domain/account and actual KV commit before this.
    follower.complete();
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false, writers_stopped: true });
    pending(quiet.as_mut()).await;
    pending(drained.as_mut()).await;
    leader.complete();
    drop(timeout(WAIT, quiet).await.unwrap().unwrap());
    drop(timeout(WAIT, drained).await.unwrap().unwrap());
    assert_eq!(domain.snapshot().active, 0);
}

#[tokio::test]
async fn suspension_retains_original_capacity_and_full_domain_resume_needs_no_second_slot() {
    let domain = Domain::new();
    let writer = domain.writer();
    let mut pending_operations = Vec::new();
    for _ in 0..MAX_MUTATIONS {
        pending_operations.push(writer.begin().await.unwrap().suspend());
    }
    assert_eq!(domain.snapshot(), Snapshot { active: MAX_MUTATIONS, queued: 0, uncertain: false, writers_stopped: false });
    assert!(matches!(writer.begin().await, Err(BeginError::Capacity)));
    let leader = timeout(WAIT, pending_operations.pop().unwrap().resume()).await.unwrap();
    assert_eq!(domain.snapshot().active, MAX_MUTATIONS);
    // Same pure trusted metadata acknowledgement contract as the follower test.
    for follower in pending_operations {
        follower.complete();
    }
    assert_eq!(domain.snapshot().active, 1);
    leader.complete();
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: false });
    drop(domain.drain(deadline()).await.unwrap());
}
