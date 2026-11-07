//! Committed command acceptance only. Nothing here publishes a caller-supplied OID.
mod authorization;
pub mod full;
pub mod ownership;
mod rows;

use std::num::NonZeroU32;

use super::pool::Pool;
use crate::StorageError;
use rbit_core::Timestamp;
use rbit_wikis::{
    WikiPageId,
    commands::{WikiActorIncarnation, WikiIntent, WikiWriteCommand},
    policy::{WikiExpected, WikiMutationState, WikiPolicyError, check_write},
};

/// One of these errors never means a visible write succeeded. A commit I/O error
/// can be uncertain: preserve the original command identity and retry/reconcile.
#[derive(Debug)]
pub enum WikiMutationError {
    Storage(StorageError),
    Policy(WikiPolicyError),
    ReplayMismatch,
    Capacity,
    Corrupt,
    Quarantined,
    Exhausted,
    /// Completed metadata requires native exact-payload verification before ACK.
    VisibleReplay,
    /// An exact command was terminally refused; it is never accepted again.
    Aborted,
}
impl From<StorageError> for WikiMutationError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}
impl From<sqlx::Error> for WikiMutationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error.into())
    }
}

#[derive(Debug, Clone)]
pub struct WikiMutationStore {
    pool: Pool,
}

impl super::Database {
    pub fn wiki_mutations(&self) -> WikiMutationStore {
        WikiMutationStore { pool: self.store("wiki_mutations") }
    }
}

impl WikiMutationStore {
    /// Fresh permission/archive/head checks and exact command binding occur under
    /// one writer TX. Its commit reserves page identity before any Git preparation.
    /// `max_pending` is an explicit host-wide pending-intent limit, not a queue size.
    /// This legacy general pool uses NORMAL synchronization; successful acceptance
    /// here is not a power-loss durability acknowledgement.
    pub async fn accept(&self, command: WikiWriteCommand, at: Timestamp, max_pending: NonZeroU32) -> Result<WikiIntent, WikiMutationError> {
        Ok(accept_on(&self.pool, command, at, max_pending).await?.intent)
    }
}

struct AcceptedCommand {
    intent: WikiIntent,
    inserted: bool,
}

async fn accept_on(
    pool: &Pool,
    command: WikiWriteCommand,
    at: Timestamp,
    max_pending: NonZeroU32,
) -> Result<AcceptedCommand, WikiMutationError> {
    let (repository, wiki, expected_head, expected_page) = match command.expected {
        WikiExpected::Create { repository, wiki, head } => (repository, wiki, head, None),
        WikiExpected::Change { repository, wiki, head, page } => (repository, wiki, Some(head), Some(page)),
    };
    let mut tx = pool.begin_immediate().await?;
    let access = authorization::load(&mut tx, repository, command.actor).await?;
    let authorization::AuthorizedWiki { access: facts, wiki: current_wiki, store, head, actor_incarnation } =
        access.decode(repository, command.actor)?;
    // Permission and archive precede replay metadata, keeping private history hidden.
    let probe = WikiMutationState { repository, wiki: current_wiki, head: expected_head, page: expected_page, pending: false };
    check_write(&facts, command.expected, &probe).map_err(WikiMutationError::Policy)?;
    let existing: Option<rows::IntentRow> =
        sqlx::query_as("SELECT * FROM wiki_mutation_intents WHERE wiki_id=?1 AND actor_incarnation=?2 AND actor_id=?3 AND command_id=?4")
            .bind(wiki.get())
            .bind(actor_incarnation.map(WikiActorIncarnation::get))
            .bind(command.actor.0)
            .bind(command.id.as_bytes().as_slice())
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(existing) = existing {
        let intent = existing.replay(command, store, actor_incarnation.ok_or(WikiMutationError::Corrupt)?)?;
        if head != expected_head {
            return Err(WikiMutationError::Policy(WikiPolicyError::Conflict));
        }
        tx.commit().await?;
        return Ok(AcceptedCommand { intent, inserted: false });
    }
    let pending: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_mutation_intents WHERE wiki_id=? AND state IN ('pending','quarantined'))")
            .bind(wiki.get())
            .fetch_one(&mut *tx)
            .await?;
    let slot: Option<(i64, String)> = match expected_page {
        Some(page) => {
            sqlx::query_as("SELECT id,path FROM wiki_pages WHERE repo_id=?1 AND id=?2")
                .bind(repository.0)
                .bind(page.get())
                .fetch_optional(&mut *tx)
                .await?
        }
        None => {
            sqlx::query_as("SELECT id,path FROM wiki_pages WHERE repo_id=?1 AND path=?2 COLLATE BINARY")
                .bind(repository.0)
                .bind(command.path.as_str())
                .fetch_optional(&mut *tx)
                .await?
        }
    };
    // W2 edit does not rename. Rename/aliases need their own future command.
    if expected_page.is_some() && slot.as_ref().is_some_and(|(_, path)| path != command.path.as_str()) {
        return Err(WikiMutationError::Policy(WikiPolicyError::Conflict));
    }
    let current =
        WikiMutationState { repository, wiki: current_wiki, head, page: slot.as_ref().and_then(|(id, _)| WikiPageId::new(*id)), pending };
    let generation = check_write(&facts, command.expected, &current).map_err(WikiMutationError::Policy)?;
    let full: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_mutation_intents WHERE state IN ('pending','quarantined') LIMIT 1 OFFSET ?)")
            .bind(i64::from(max_pending.get()) - 1)
            .fetch_one(&mut *tx)
            .await?;
    if full {
        return Err(WikiMutationError::Capacity);
    }
    let actor_incarnation = if let Some(incarnation) = actor_incarnation {
        incarnation
    } else {
        let id: i64 = sqlx::query_scalar("INSERT INTO wiki_actor_incarnations(account_id) VALUES(?) RETURNING id")
            .bind(command.actor.0)
            .fetch_one(&mut *tx)
            .await?;
        WikiActorIncarnation::new(id).ok_or(WikiMutationError::Corrupt)?
    };
    let page = if let Some(page) = expected_page {
        page
    } else {
        let floor: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0) FROM sqlite_sequence WHERE name IN ('wiki_pages','wiki_page_allocations')")
                .fetch_one(&mut *tx)
                .await?;
        let id = floor.checked_add(1).and_then(WikiPageId::new).ok_or(WikiMutationError::Exhausted)?;
        sqlx::query("INSERT INTO wiki_page_allocations(id) VALUES(?)").bind(id.get()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wiki_page_allocations WHERE id=?").bind(id.get()).execute(&mut *tx).await?;
        id
    };
    sqlx::query("INSERT INTO wiki_mutation_intents(wiki_id,repo_id,actor_id,actor_incarnation,command_id,operation,expected_generation,expected_revision,page_id,generation,path,title,content,accepted_at,state) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,'pending')")
            .bind(wiki.get()).bind(repository.0).bind(command.actor.0).bind(actor_incarnation.get()).bind(command.id.as_bytes().as_slice())
            .bind(if expected_page.is_some() { "edit" } else { "create" })
            .bind(expected_head.map(|head| head.generation.get())).bind(expected_head.map(|head| head.revision.to_string()))
            .bind(page.get()).bind(generation.get()).bind(command.path.as_str()).bind(command.title.as_str()).bind(command.content.as_str())
            .bind(at.seconds()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(AcceptedCommand {
        intent: WikiIntent { store, actor_incarnation, command, page, generation, accepted_at: at, target: None },
        inserted: true,
    })
}
