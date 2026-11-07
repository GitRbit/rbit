use super::*;
use rbit_identity_models::{
    AccountId,
    credential::CredentialEpoch,
    sessions::{CompletedSessionRevocation, SessionRevocationRequest},
};
use std::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
};
struct RetiredPort {
    writes: AtomicUsize,
}
fn pending() -> PendingSessionRevocation {
    PendingSessionRevocation::from_storage(1, AccountId(1), CredentialEpoch::new(1).unwrap(), None).unwrap()
}
impl SessionRevocationRepository for RetiredPort {
    type Error = std::io::Error;
    type Applied = ();
    fn pending(&self, _after: i64, _limit: NonZeroU8) -> impl Future<Output = Result<Vec<PendingSessionRevocation>, Self::Error>> + Send {
        std::future::ready(Ok(vec![pending()]))
    }
    fn state(&self, _pending: &PendingSessionRevocation) -> impl Future<Output = Result<SessionRevocationState, Self::Error>> + Send {
        std::future::ready(Ok(SessionRevocationState::Retired))
    }
    fn apply(&self, _request: SessionRevocationRequest) -> impl Future<Output = Result<Self::Applied, Self::Error>> + Send {
        self.writes.fetch_add(1, Ordering::Relaxed);
        std::future::ready(Err(std::io::Error::other("unexpected retired apply")))
    }
    fn acknowledge(
        &self,
        _request: &SessionRevocationRequest,
        _applied: &Self::Applied,
    ) -> impl Future<Output = Result<Ack, Self::Error>> + Send {
        self.writes.fetch_add(1, Ordering::Relaxed);
        std::future::ready(Err(std::io::Error::other("unexpected retired acknowledgment")))
    }
    fn supersede(
        &self,
        _pending: &PendingSessionRevocation,
        _witness: &CompletedSessionRevocation,
    ) -> impl Future<Output = Result<Ack, Self::Error>> + Send {
        self.writes.fetch_add(1, Ordering::Relaxed);
        std::future::ready(Err(std::io::Error::other("unexpected retired supersession")))
    }
}
#[tokio::test]
async fn accepted_upgrade_retirement_is_distinct_and_never_applies_or_acknowledges() {
    let port = RetiredPort { writes: AtomicUsize::new(0) };
    let application = SessionRecoveryApplication::new(&port);
    assert_eq!(application.reconcile(&pending()).await.unwrap(), SessionRecoveryDisposition::Retired);
    let batch = application.batch(0, NonZeroU8::new(1).unwrap()).await.unwrap();
    assert_eq!((batch.visited, batch.cursor, batch.retired), (1, 1, 1));
    assert_eq!((batch.completed, batch.already_resolved, batch.superseded, batch.unsupported), (0, 0, 0, 0));
    assert_eq!(port.writes.load(Ordering::Relaxed), 0);
}
