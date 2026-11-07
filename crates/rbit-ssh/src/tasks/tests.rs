//! Reproduce external library tasks and cancelled wrappers independently of network timing.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Resource(Arc<AtomicUsize>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct Handler {
    _resource: Resource,
    _token: tokio_util::task::task_tracker::TaskTrackerToken,
}

#[tokio::test]
async fn handler_destruction_remains_required_after_its_connection_waiter_exits() {
    let tasks = SshTasks::new();
    let token = tasks.handler_token().unwrap();
    let dropped = Arc::new(AtomicUsize::new(0));
    let resource = Resource(Arc::clone(&dropped));
    let (release, resume) = tokio::sync::oneshot::channel();
    // Model russh's internally spawned task: its JoinHandle may be detached, but
    // the handler token must still prove destruction of retained capabilities.
    let handler = Handler { _resource: resource, _token: token };
    tokio::spawn(async move {
        let _handler = handler;
        let _ = resume.await;
    });
    let mut connections = JoinSet::new();
    tasks.connection(&mut connections, async {});
    connections.join_next().await.unwrap().unwrap();
    tasks.close();
    assert!(tokio::time::timeout(std::time::Duration::ZERO, tasks.wait()).await.is_err());
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    tasks.wait().await;
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn aborted_connections_are_joined_and_reply_children_are_destroyed() {
    let tasks = SshTasks::new();
    let dropped = Arc::new(AtomicUsize::new(0));
    let connection = Resource(Arc::clone(&dropped));
    let reply = Resource(Arc::clone(&dropped));
    let mut connections = JoinSet::new();
    tasks.connection(&mut connections, async move {
        let _connection = connection;
        std::future::pending::<()>().await;
    });
    tasks.reply(async move {
        let _reply = reply;
        std::future::pending::<()>().await;
    });
    connections.abort_all();
    tasks.close();
    while connections.join_next().await.is_some() {}
    tasks.wait().await;
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cancelled_waiters_do_not_admit_more_work_or_claim_existing_work_drained() {
    let tasks = SshTasks::new();
    let (release, resume) = tokio::sync::oneshot::channel();
    let stop = tasks.cancellation();
    drop(
        tasks
            .spawn(async move {
                stop.cancelled().await;
                let _ = resume.await;
            })
            .unwrap(),
    );
    tasks.close();
    assert!(tokio::time::timeout(std::time::Duration::ZERO, tasks.wait()).await.is_err());
    assert!(tasks.handler_token().is_none());
    assert!(
        tasks
            .spawn(async {
                panic!("closed scope admitted a child");
            })
            .is_none()
    );
    release.send(()).unwrap();
    tasks.wait().await;
}

#[tokio::test]
async fn dropping_listener_guard_requests_stop_but_waits_real_children() {
    let tasks = SshTasks::new();
    let guard = tasks.stop_on_drop();
    let stop = tasks.cancellation();
    let (release, resume) = tokio::sync::oneshot::channel();
    drop(
        tasks
            .spawn(async move {
                stop.cancelled().await;
                let _ = resume.await;
            })
            .unwrap(),
    );
    drop(guard);
    assert!(tokio::time::timeout(std::time::Duration::ZERO, tasks.wait()).await.is_err());
    release.send(()).unwrap();
    tasks.wait().await;
}

#[tokio::test]
async fn counted_connection_can_adopt_existing_library_session_during_close() {
    let tasks = SshTasks::new();
    let parent = tasks.handler_token().unwrap();
    let (release, resume) = tokio::sync::oneshot::channel();
    tasks.close();
    // Admission is closed, but an already-counted parent must transfer ownership
    // of a library task it already started rather than detach its join handle.
    tasks.session(async move {
        let _ = resume.await;
    });
    drop(parent);
    assert!(tokio::time::timeout(std::time::Duration::ZERO, tasks.wait()).await.is_err());
    release.send(()).unwrap();
    tasks.wait().await;
}
