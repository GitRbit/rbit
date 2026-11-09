//! The webhook store, worker wake-up and task spawning behind the feature's queueing and
//! worker ports. No selection, retry or envelope policy lives here.
use super::{Inner, WebhookService, delivery};
use crate::ServiceError;
use rbit_core::{
    Timestamp,
    webhook::{DeliveryId, WebhookEvent, WebhookId},
};
use rbit_notifications::webhooks::{
    policy::NextStep,
    queue::{HookQueue, HookScope, QueuedDelivery},
    worker::DeliveryQueue,
};
use rbit_storage::sqlite::webhooks::{AttemptOutcome, AttemptRecord, ClaimedDelivery, NewDelivery};

/// Borrows the service state; holds no policy.
pub(super) struct StoreQueue<'a>(pub(super) &'a Inner);

impl HookQueue for StoreQueue<'_> {
    type Error = ServiceError;
    type Hook = WebhookId;
    type Event = WebhookEvent;
    type Delivery = DeliveryId;

    fn event_name(event: WebhookEvent) -> &'static str {
        event.as_str()
    }

    async fn subscribed(&self, scope: &HookScope, event: WebhookEvent) -> Result<Vec<WebhookId>, ServiceError> {
        Ok(self.0.store.subscribed(scope.repo, &scope.orgs, event).await?)
    }

    async fn enqueue(&self, deliveries: Vec<QueuedDelivery<WebhookId, WebhookEvent>>) -> Result<Vec<DeliveryId>, ServiceError> {
        let deliveries = deliveries
            .into_iter()
            .map(|queued| NewDelivery {
                hook_id: queued.hook,
                guid: queued.guid,
                event: queued.event,
                action: queued.action,
                payload: queued.payload,
            })
            .collect();
        Ok(self.0.store.enqueue(deliveries).await?)
    }

    fn new_guid(&self) -> String {
        delivery::new_guid()
    }

    fn wake(&self) {
        self.0.wake.notify_one();
    }
}

/// The worker's view of the service: claims, concurrent attempts and their records.
pub(super) struct Worker<'a>(pub(super) &'a WebhookService);

impl DeliveryQueue for Worker<'_> {
    type Error = ServiceError;
    type Claimed = ClaimedDelivery;
    type Record = AttemptRecord;

    async fn claim_due(&self, now: Timestamp, limit: u32) -> Result<Vec<ClaimedDelivery>, ServiceError> {
        Ok(self.0.inner.store.claim_due(now, limit).await?)
    }

    async fn run_all(&self, claimed: Vec<ClaimedDelivery>) {
        let mut running = tokio::task::JoinSet::new();
        for delivery in claimed {
            let service = self.0.clone();
            running.spawn(async move { service.run(delivery, None).await });
        }
        while running.join_next().await.is_some() {}
    }

    async fn record(&self, delivery: &ClaimedDelivery, record: AttemptRecord, step: NextStep) {
        let outcome = match step {
            NextStep::Succeeded => AttemptOutcome::Succeeded,
            NextStep::Retry(at) => AttemptOutcome::Retry(at),
            NextStep::Failed => AttemptOutcome::Failed,
        };
        if let Err(error) = self.0.inner.store.record_attempt(delivery.id, record, outcome).await {
            tracing::warn!(%error, delivery = delivery.id.0, "cannot record a webhook delivery attempt");
        }
    }

    fn observe(&self, delivery: &ClaimedDelivery, record: &AttemptRecord, step: NextStep) {
        let label = step.label();
        metrics::counter!("rbit_webhook_attempts_total", "outcome" => label).increment(1);
        tracing::debug!(
            hook = delivery.hook_id.0,
            delivery = delivery.id.0,
            event = delivery.event.as_str(),
            status = record.response_status,
            outcome = label,
            "webhook delivery attempted"
        );
    }
}
