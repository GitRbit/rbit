use super::*;
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
async fn pending<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending(), "expected genuine pending ownership acquisition");
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn queued_cancellation_is_known_clean_but_acquired_drop_is_sticky_uncertainty() {
    let domain = Domain::new();
    let writer = domain.writer();
    let first = writer.begin().await.unwrap();
    let mut queued = Box::pin(writer.begin());
    pending(queued.as_mut()).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 1, uncertain: false });
    drop(queued);
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false });
    first.complete();
    drop(domain.drain(deadline()).await.unwrap());
    drop(writer.begin().await.unwrap());
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true });
    // Uncertainty never blocks existing passive writers, and success never clears it.
    writer.begin().await.unwrap().complete();
    assert!(matches!(domain.drain(deadline()).await, Err(DrainError::Uncertain)));
}

#[tokio::test]
async fn quiescence_retains_serialization_and_waits_for_registered_writers() {
    let domain = Domain::new();
    let writer = domain.writer();
    let quiet = domain.drain(deadline()).await.unwrap();
    let mut write = Box::pin(writer.begin());
    pending(write.as_mut()).await;
    assert_eq!(domain.snapshot().queued, 1);
    drop(quiet);
    let mutation = write.await.unwrap();
    let mut drain = Box::pin(domain.drain(deadline()));
    pending(drain.as_mut()).await;
    mutation.complete();
    let quiet = drain.await.unwrap();
    let mut next = Box::pin(writer.begin());
    pending(next.as_mut()).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 1, uncertain: false });
    drop(next); // No enrolled work started while quiescent permit was held.
    drop(quiet);
    drop(domain.drain(deadline()).await.unwrap());
}

#[tokio::test]
async fn drain_between_two_writers_releases_serial_for_the_registered_queued_writer() {
    let domain = Domain::new();
    let writer = domain.writer();
    let first = writer.begin().await.unwrap();
    let mut drain = Box::pin(domain.drain(deadline()));
    pending(drain.as_mut()).await;
    let mut second = Box::pin(writer.begin());
    pending(second.as_mut()).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 1, uncertain: false });
    first.complete();
    // Drain wins FIFO, sees queued work, and must release before awaiting it.
    pending(drain.as_mut()).await;
    let second = second.await.unwrap();
    second.complete();
    drop(drain.await.unwrap());
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false });
}

#[tokio::test]
async fn bounded_queue_refuses_extra_work_without_poison_or_unbounded_registration() {
    let domain = Domain::new();
    let writer = domain.writer();
    let quiet = domain.drain(deadline()).await.unwrap();
    let mut waiting = Vec::new();
    for _ in 0..MAX_MUTATIONS {
        let mut future = Box::pin(writer.begin());
        pending(future.as_mut()).await;
        waiting.push(future);
    }
    assert_eq!(domain.snapshot().queued, MAX_MUTATIONS);
    assert!(matches!(writer.begin().await, Err(Capacity)));
    drop(waiting);
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false });
    drop(quiet);
    writer.begin().await.unwrap().complete();
    drop(domain.drain(deadline()).await.unwrap());
}

#[tokio::test]
async fn expired_and_waiting_deadlines_never_return_quiescence() {
    let domain = Domain::new();
    assert!(matches!(domain.drain(Instant::now()).await, Err(DrainError::Deadline)));
    let mutation = domain.writer().begin().await.unwrap();
    let end = Instant::now() + Duration::from_millis(20);
    assert!(matches!(domain.drain(end).await, Err(DrainError::Deadline)));
    assert!(Instant::now() >= end);
    mutation.complete();
    drop(domain.drain(deadline()).await.unwrap());
}

#[tokio::test]
async fn poisoned_counter_mutex_is_uncertain_and_cannot_be_certified_clean() {
    let domain = Domain::new();
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _state = domain.inner.state.lock().unwrap();
        panic!("injected counter-lock panic");
    }));
    assert!(poisoned.is_err());
    assert!(domain.snapshot().uncertain);
    domain.writer().begin().await.unwrap().complete();
    assert!(matches!(domain.drain(deadline()).await, Err(DrainError::Uncertain)));
}
