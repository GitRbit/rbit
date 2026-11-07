#![cfg(feature = "applications")]
//! Deletion authority, moved snapshot identity and the original postcommit cancellation gap.
#[path = "support/thread_fixtures.rs"]
mod fixtures;
use rbit_collaboration::{
    issues::{IssueDeletion, IssueDeletionError, IssueDeletionPort},
    threads::{Issue, IssueId, IssueKind, IssueText},
};
use std::{
    future::{Future, poll_fn},
    sync::Mutex,
    task::Poll,
};
use tokio::sync::Notify;
struct Port {
    row: Mutex<Option<Issue>>,
    calls: Mutex<Vec<&'static str>>,
    published: Mutex<Vec<Issue>>,
    pause_audit: bool,
    gate: Notify,
    fail: bool,
}
impl Port {
    fn new(row: Option<Issue>) -> Self {
        Self {
            row: Mutex::new(row),
            calls: Mutex::new(Vec::new()),
            published: Mutex::new(Vec::new()),
            pause_audit: false,
            gate: Notify::new(),
            fail: false,
        }
    }
    fn log(&self, value: &'static str) {
        self.calls.lock().unwrap().push(value);
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl IssueDeletionPort for Port {
    type Error = &'static str;
    async fn delete(&self, id: IssueId) -> Result<Option<Issue>, Self::Error> {
        assert_eq!(id, IssueId(1));
        self.log("delete");
        if self.fail { Err("store") } else { Ok(self.row.lock().unwrap().take()) }
    }
    async fn audit(&self, _: &Issue) {
        self.log("audit");
        if self.pause_audit {
            self.gate.notified().await;
        }
    }
    fn publish(&self, row: Issue) {
        self.log("publish");
        self.published.lock().unwrap().push(row);
    }
}
#[tokio::test]
async fn permissions_precede_archive_and_no_denied_or_unpolled_path_deletes() {
    let context = fixtures::issue();
    let port = Port::new(Some(fixtures::issue()));
    assert!(matches!(IssueDeletion::new(false, true).delete(&port, &context).await, Err(IssueDeletionError::Forbidden)));
    let pull = Issue { kind: IssueKind::Pull, ..fixtures::issue() };
    assert!(matches!(IssueDeletion::new(true, true).delete(&port, &pull).await, Err(IssueDeletionError::Forbidden)));
    assert!(matches!(IssueDeletion::new(true, true).delete(&port, &context).await, Err(IssueDeletionError::Archived)));
    let app = IssueDeletion::new(true, false);
    drop(app.delete(&port, &context));
    assert_eq!(*port.calls.lock().unwrap(), Vec::<&str>::new());
}
#[tokio::test]
async fn publishes_the_returned_snapshot_without_copying_its_body_allocation() {
    let row = Issue { body: IssueText::body(&"stored long body ".repeat(256)).unwrap(), ..fixtures::issue() };
    let original_allocation = row.body.as_str().as_ptr() as usize;
    let port = Port::new(Some(row));
    IssueDeletion::new(true, false).delete(&port, &fixtures::issue()).await.unwrap();
    assert_eq!(*port.calls.lock().unwrap(), ["delete", "audit", "publish"]);
    assert_eq!(port.published.lock().unwrap()[0].body.as_str().as_ptr() as usize, original_allocation);
    assert!(matches!(IssueDeletion::new(true, false).delete(&port, &fixtures::issue()).await, Err(IssueDeletionError::NotFound)));
    assert_eq!(port.published.lock().unwrap().len(), 1);
    let mut port = Port::new(None);
    port.fail = true;
    assert!(matches!(IssueDeletion::new(true, false).delete(&port, &fixtures::issue()).await, Err(IssueDeletionError::Port("store"))));
    assert_eq!(*port.calls.lock().unwrap(), ["delete"]);
}
#[tokio::test]
async fn cancelling_during_audit_preserves_existing_postdelete_event_gap() {
    let mut port = Port::new(Some(fixtures::issue()));
    port.pause_audit = true;
    let app = IssueDeletion::new(true, false);
    let context = fixtures::issue();
    let mut operation = Box::pin(app.delete(&port, &context));
    poll_fn(|cx| {
        assert!(operation.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(operation);
    assert_eq!(port.row.lock().unwrap().as_ref(), None);
    assert_eq!(*port.calls.lock().unwrap(), ["delete", "audit"]);
    assert_eq!(port.published.lock().unwrap().len(), 0);
}
