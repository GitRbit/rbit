//! Which hooks receive an event, and how deliveries are queued for them.
//!
//! An event goes to the active hooks of its repository and of the organisations
//! involved that subscribe to its event. Like GitHub, a hook receives only events that
//! happened after it was created: hook ids are reserved before a hook is stored and each
//! event is stamped with the newest id reserved when it was published, so hooks with a
//! larger id are skipped even when the subscriber reaches the event later. Each queued
//! delivery gets its own random id and records the body's `action` where its event has
//! one; queueing wakes the worker.
use super::payloads::action_of;
use rbit_contracts::{AccountId, RepoId};
use serde_json::Value;
use std::future::Future;

/// The owners whose hooks may receive an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookScope {
    /// The repository's own hooks; `None` once the repository is gone.
    pub repo: Option<RepoId>,
    /// Organisations whose hooks fire for the repository.
    pub orgs: Vec<AccountId>,
}

impl HookScope {
    /// A repository's hooks and its owning organisation's (`None` for personal owners).
    pub fn repository(repo: RepoId, organization: Option<AccountId>) -> Self {
        Self { repo: Some(repo), orgs: organization.into_iter().collect() }
    }

    /// A transferred repository: its own hooks and those of the new and previous
    /// organisation owners, in that order.
    pub fn transferred(repo: RepoId, organization: Option<AccountId>, previous: Option<AccountId>) -> Self {
        Self { repo: Some(repo), orgs: organization.into_iter().chain(previous).collect() }
    }

    /// A deleted repository: its own hooks went with it; only its organisation's remain.
    pub fn deleted(organization: AccountId) -> Self {
        Self { repo: None, orgs: vec![organization] }
    }
}

/// One delivery to queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedDelivery<H, E> {
    pub hook: H,
    pub guid: String,
    pub event: E,
    pub action: Option<String>,
    pub payload: String,
}

/// The hook store and the worker's wake-up, bound by composition.
pub trait HookQueue: Sync {
    type Error: Send;
    type Hook: Copy + Ord + Send + Sync;
    type Event: Copy + Send + Sync;
    type Delivery: Send;
    /// The event's wire name (`push`, `issues`, ...).
    fn event_name(event: Self::Event) -> &'static str;
    /// Active hooks in `scope` subscribed to `event`.
    fn subscribed(&self, scope: &HookScope, event: Self::Event) -> impl Future<Output = Result<Vec<Self::Hook>, Self::Error>> + Send;
    fn enqueue(
        &self,
        deliveries: Vec<QueuedDelivery<Self::Hook, Self::Event>>,
    ) -> impl Future<Output = Result<Vec<Self::Delivery>, Self::Error>> + Send;
    /// A fresh random delivery id.
    fn new_guid(&self) -> String;
    /// Wakes the worker so queued deliveries go out without waiting for a poll.
    fn wake(&self);
}

/// The hooks in `scope` subscribed to `event` that existed when an event stamped
/// `newest` was published.
pub async fn select<Q: HookQueue>(queue: &Q, newest: Q::Hook, scope: &HookScope, event: Q::Event) -> Result<Vec<Q::Hook>, Q::Error> {
    let mut hooks = queue.subscribed(scope, event).await?;
    hooks.retain(|hook| *hook <= newest);
    Ok(hooks)
}

/// Queues `payload` for each of `hooks` and wakes the worker. Nothing happens for no hooks.
pub async fn queue<Q: HookQueue>(queue: &Q, hooks: Vec<Q::Hook>, event: Q::Event, payload: &Value) -> Result<Vec<Q::Delivery>, Q::Error> {
    if hooks.is_empty() {
        return Ok(Vec::new());
    }
    let body = payload.to_string();
    let action = action_of(Q::event_name(event), payload);
    let deliveries = hooks
        .into_iter()
        .map(|hook| QueuedDelivery { hook, guid: queue.new_guid(), event, action: action.clone(), payload: body.clone() })
        .collect();
    let ids = queue.enqueue(deliveries).await?;
    queue.wake();
    Ok(ids)
}

/// Queues a past delivery again as a new one with the same event, action and body.
pub async fn redeliver<Q: HookQueue>(
    queue: &Q,
    hook: Q::Hook,
    event: Q::Event,
    action: Option<String>,
    payload: String,
) -> Result<Vec<Q::Delivery>, Q::Error> {
    let ids = queue.enqueue(vec![QueuedDelivery { hook, guid: queue.new_guid(), event, action, payload }]).await?;
    queue.wake();
    Ok(ids)
}
