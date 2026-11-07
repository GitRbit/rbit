//! One synchronous gate/admission path for legacy acceptance and closed recovery.
use super::{FullWikiWriter, Pool, WikiCatalogHandle, WikiWriteError};
use rbit_wikis::ownership::WikiCatalogOwnership;
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
};

#[derive(Clone, Copy)]
pub(super) enum Admission {
    Legacy,
    BeginFence,
    Recovery,
}

impl FullWikiWriter {
    pub(super) fn start_operation<T, F, Fut>(
        &self,
        admission: Admission,
        operation: F,
    ) -> Result<tokio::task::JoinHandle<Result<T, WikiWriteError>>, WikiWriteError>
    where
        T: Send + 'static,
        F: FnOnce(Pool, WikiCatalogHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, WikiWriteError>> + Send + 'static,
    {
        let closed = self.0.gate.lock().map_err(|_| WikiWriteError::Closed)?;
        if self.0.state.is_uncertain() {
            return Err(WikiWriteError::Quarantined);
        }
        let claimed = self.0.protocol_claimed.load(Ordering::Acquire);
        if matches!(admission, Admission::Legacy) && claimed {
            return Err(WikiWriteError::FencedOwnerRequired);
        }
        if *closed {
            return Err(WikiWriteError::Closed);
        }
        match admission {
            Admission::Recovery if !claimed => return Err(WikiWriteError::Binding),
            Admission::BeginFence if claimed => return Err(WikiWriteError::Busy),
            _ => {}
        }
        let permit = Arc::clone(&self.0.admission).try_acquire_owned().map_err(|_| WikiWriteError::Busy)?;
        if matches!(admission, Admission::BeginFence) {
            self.0.protocol_claimed.store(true, Ordering::Release);
        }
        let pool = self.0.commands.clone();
        let catalog = self.0.catalog.clone();
        let state = Arc::clone(&self.0.state);
        Ok(self.0.tasks.spawn(async move {
            let _permit = permit;
            let check = catalog.clone();
            tokio::task::spawn_blocking(move || check.check_bound()).await.map_err(|_| WikiWriteError::Uncertain)??;
            let result = tokio::select! {
                result = operation(pool, catalog) => result,
                () = state.unproved() => return Err(WikiWriteError::Quarantined),
            };
            if state.is_uncertain() {
                return Err(WikiWriteError::Quarantined);
            }
            result
        }))
    }
}
