//! Exact replay comparison: no content hash substitutes for the accepted bytes.
use super::WikiMutationError;
use rbit_core::Timestamp;
use rbit_wikis::{
    WikiGeneration, WikiPageId, WikiStoreId,
    commands::{WikiActorIncarnation, WikiIntent, WikiWriteCommand},
    policy::WikiExpected,
};

#[derive(sqlx::FromRow)]
pub(super) struct IntentRow {
    wiki_id: i64,
    repo_id: i64,
    actor_id: i64,
    actor_incarnation: i64,
    command_id: Vec<u8>,
    operation: String,
    expected_generation: Option<i64>,
    expected_revision: Option<String>,
    page_id: i64,
    generation: i64,
    path: String,
    title: String,
    content: Option<String>,
    accepted_at: i64,
    target_revision: Option<String>,
    state: String,
}

impl IntentRow {
    pub fn replay(
        self,
        command: WikiWriteCommand,
        store: WikiStoreId,
        actor_incarnation: WikiActorIncarnation,
    ) -> Result<WikiIntent, WikiMutationError> {
        let (repository, expected, page, operation) = match command.expected {
            WikiExpected::Create { repository, head, .. } => (repository, head, None, "create"),
            WikiExpected::Change { repository, head, page, .. } => (repository, Some(head), Some(page), "edit"),
        };
        if self.actor_incarnation != actor_incarnation.get()
            || self.wiki_id != command.wiki().get()
            || self.repo_id != repository.0
            || self.actor_id != command.actor.0
            || self.command_id.as_slice() != command.id.as_bytes()
            || self.operation != operation
            || self.expected_generation != expected.map(|head| head.generation.get())
            || self.expected_revision != expected.map(|head| head.revision.to_string())
            || page.is_some_and(|page| page.get() != self.page_id)
            || self.path != command.path.as_str()
            || self.title != command.title.as_str()
        {
            return Err(WikiMutationError::ReplayMismatch);
        }
        match self.state.as_str() {
            // A later coordinator must verify exact native historical body bytes.
            // This store cannot turn absent replay content into a successful ACK.
            "visible" => return Err(WikiMutationError::VisibleReplay),
            "quarantined" => return Err(WikiMutationError::Quarantined),
            "aborted" => return Err(WikiMutationError::Aborted),
            "pending" => {}
            _ => return Err(WikiMutationError::Corrupt),
        }
        if self.content.as_deref() != Some(command.content.as_str()) {
            return Err(WikiMutationError::ReplayMismatch);
        }
        let page = WikiPageId::new(self.page_id).ok_or(WikiMutationError::Corrupt)?;
        let generation = WikiGeneration::new(self.generation).ok_or(WikiMutationError::Corrupt)?;
        let target = self.target_revision.map(|revision| revision.parse().map_err(|_| WikiMutationError::Corrupt)).transpose()?;
        Ok(WikiIntent { store, actor_incarnation, command, page, generation, accepted_at: Timestamp(self.accepted_at), target })
    }
}
