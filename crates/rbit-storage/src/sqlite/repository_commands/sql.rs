//! Fixed queries only, with acknowledged COMMIT/ROLLBACK provenance.
use super::{
    super::{
        pool::{Pool, outer_deferred::OuterBeginError},
        repository_writes,
        rows::RepositoryRow,
    },
    CreatorGrantOutcome, Step,
};
use crate::StorageError;
use rbit_core::{Permission, RepoId, RepoName, Timestamp};

pub(super) enum Raw {
    Created(RepositoryRow),
    CreatorGrant(CreatorGrantOutcome),
    Renamed(RepoName),
    RenameUnchanged(RepoName),
    Unit,
    Changed(bool),
}
pub(super) enum Settled {
    Committed(Raw),
    NoWrite(Raw),
    RolledBack(StorageError),
    NoTransaction(StorageError),
    Unconfirmed { original: StorageError, rollback_error: Option<StorageError> },
}
pub(super) async fn execute(pool: &Pool, step: &Step, id: Option<RepoId>) -> Settled {
    let created_at = matches!(step, Step::Create(_)).then(|| Timestamp::now().seconds());
    let mut tx = match pool.begin_outer_deferred().await {
        Ok(tx) => tx,
        Err(OuterBeginError::Acquire(error)) => return Settled::NoTransaction(error.into()),
        Err(OuterBeginError::ExistingTransaction) => {
            return Settled::Unconfirmed { original: StorageError::Invariant("repository outer transaction depth"), rollback_error: None };
        }
        Err(OuterBeginError::BeginUnconfirmed(error)) => return Settled::Unconfirmed { original: error.into(), rollback_error: None },
    };
    // IDs are bound by the command before this private fixed phase, not caller SQL.
    let result = match step {
        Step::Create(new) => repository_writes::create(&mut *tx, new, created_at.expect("create phase timestamp")).await.map(Raw::Created),
        Step::GrantCreator(actor) => {
            super::super::collaborators::upsert(&mut *tx, id.expect("bound creator repository"), *actor, Permission::Admin, Some(*actor))
                .await
                .map(|()| Raw::CreatorGrant(CreatorGrantOutcome::Granted))
        }
        Step::PreparedRename(name) => {
            repository_writes::rename(&mut *tx, id.expect("bound repository phase"), name).await.map(|()| Raw::Renamed(name.clone()))
        }
        Step::Creator { .. } | Step::DeferredRename { .. } | Step::ArchiveNow(_) => unreachable!("fixed preparation precedes SQL"),
        Step::Delete => repository_writes::delete(&mut *tx, id.expect("bound repository phase")).await.map(|()| Raw::Unit),
        Step::Transfer(owner) => {
            repository_writes::transfer(&mut tx, id.expect("bound repository phase"), *owner).await.map(|()| Raw::Unit)
        }
        Step::General(settings) => {
            repository_writes::update_general(&mut *tx, id.expect("bound repository phase"), settings).await.map(|()| Raw::Unit)
        }
        Step::Rename(name) => repository_writes::rename(&mut *tx, id.expect("bound repository phase"), name).await.map(|()| Raw::Unit),
        Step::Archive(by, at) => {
            repository_writes::archive(&mut *tx, id.expect("bound repository phase"), *by, *at).await.map(Raw::Changed)
        }
        Step::Unarchive => repository_writes::unarchive(&mut *tx, id.expect("bound repository phase")).await.map(Raw::Changed),
    };
    match result {
        Ok(value) => match tx.commit().await {
            Ok(()) => Settled::Committed(value),
            Err(error) => Settled::Unconfirmed { original: error.into(), rollback_error: None },
        },
        Err(original) => match tx.rollback().await {
            Ok(()) => Settled::RolledBack(original),
            Err(error) => Settled::Unconfirmed { original, rollback_error: Some(error.into()) },
        },
    }
}
