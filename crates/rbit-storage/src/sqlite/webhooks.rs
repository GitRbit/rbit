//! Webhooks and their delivery queue.
//!
//! Hooks belong to exactly one repository or organisation ([`WebhookOwner`]); every
//! lookup that starts from a request is scoped to that owner, so an id from one
//! repository can never reach another's hook. Secrets are write-only: rows come back with
//! `has_secret` only, and the raw value is read solely when claiming a delivery to sign.
//!
//! The queue is the `webhook_deliveries` table. Producers insert `pending` rows; the
//! worker claims due rows atomically (one `UPDATE … RETURNING`, so two workers never
//! deliver the same row), and records each attempt's outcome on the row.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file. Keep row types here rather than in `rows.rs`.

use super::pool::Pool;
use rbit_core::{
    AccountId, RepoId, Timestamp,
    webhook::{
        Delivery, DeliveryId, DeliveryStatus, LastDelivery, Webhook, WebhookEvent, WebhookId, WebhookOwner, WebhookUrl,
        format_header_block, parse_header_block,
    },
};

use crate::StorageError;

/// Builds a static `SELECT` over hooks (with their latest delivery) and a trailing clause.
macro_rules! select_hook {
    ($clause:literal) => {
        concat!(
            "SELECT w.id, w.repo_id, w.org_id, w.url, w.secret IS NOT NULL AS has_secret, w.events, w.active, w.created_by, \
             w.created_at, w.updated_at, d.status AS last_status, d.response_status AS last_response_status, \
             COALESCE(d.last_attempt_at, d.created_at) AS last_at \
             FROM webhooks w LEFT JOIN webhook_deliveries d \
             ON d.id = (SELECT id FROM webhook_deliveries WHERE hook_id = w.id ORDER BY id DESC LIMIT 1) ",
            $clause
        )
    };
}

/// Builds a static `SELECT` over deliveries with a trailing clause.
macro_rules! select_delivery {
    ($clause:literal) => {
        concat!(
            "SELECT id, hook_id, guid, event, action, payload, status, attempts, next_attempt_at, last_attempt_at, request_headers, \
             response_status, response_headers, response_body, duration_ms, error, created_at FROM webhook_deliveries ",
            $clause
        )
    };
}

/// A hook to store. The URL is validated; whether its host may be contacted is the
/// service's decision.
#[derive(Debug, Clone)]
pub struct NewWebhook {
    /// The id to store the hook under; `None` lets `SQLite` choose. The service passes one
    /// reserved from the event bus, so events can tell which hooks existed when they
    /// happened.
    pub id: Option<WebhookId>,
    pub owner: WebhookOwner,
    pub url: WebhookUrl,
    pub secret: Option<String>,
    pub events: Vec<WebhookEvent>,
    pub active: bool,
    pub created_by: Option<AccountId>,
}

/// What to do with a hook's secret on update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretUpdate {
    Keep,
    Clear,
    Set(String),
}

/// A hook's new settings.
#[derive(Debug, Clone)]
pub struct WebhookUpdate {
    pub url: WebhookUrl,
    pub secret: SecretUpdate,
    pub events: Vec<WebhookEvent>,
    pub active: bool,
}

/// A delivery to queue.
#[derive(Debug, Clone)]
pub struct NewDelivery {
    pub hook_id: WebhookId,
    pub guid: String,
    pub event: WebhookEvent,
    pub action: Option<String>,
    pub payload: String,
}

/// A delivery the worker has claimed, with what it needs to send it.
#[derive(Debug, Clone)]
pub struct ClaimedDelivery {
    pub id: DeliveryId,
    pub hook_id: WebhookId,
    pub guid: String,
    pub event: WebhookEvent,
    pub payload: String,
    /// Attempts made before this one.
    pub attempts: u32,
    pub url: String,
    /// The raw signing secret. Never log or store it elsewhere.
    pub secret: Option<String>,
}

/// What one attempt produced.
#[derive(Debug, Clone)]
pub struct AttemptRecord {
    pub at: Timestamp,
    pub request_headers: Vec<(String, String)>,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Where a delivery goes after an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    Succeeded,
    /// Try again at the given time.
    Retry(Timestamp),
    Failed,
}

#[derive(Debug, sqlx::FromRow)]
struct WebhookRow {
    id: i64,
    repo_id: Option<i64>,
    org_id: Option<i64>,
    url: String,
    has_secret: bool,
    events: String,
    active: bool,
    created_by: Option<i64>,
    created_at: i64,
    updated_at: i64,
    last_status: Option<String>,
    last_response_status: Option<i64>,
    last_at: Option<i64>,
}

impl TryFrom<WebhookRow> for Webhook {
    type Error = StorageError;

    fn try_from(row: WebhookRow) -> Result<Self, Self::Error> {
        let owner = match (row.repo_id, row.org_id) {
            (Some(repo), None) => WebhookOwner::Repository(RepoId(repo)),
            (None, Some(org)) => WebhookOwner::Organization(AccountId(org)),
            _ => return Err(corrupt("webhook owner", row.id)),
        };
        let url = WebhookUrl::parse(&row.url).map_err(|error| corrupt("webhook url", error))?;
        let last_delivery = match (row.last_status, row.last_at) {
            (Some(status), Some(at)) => Some(LastDelivery {
                status: DeliveryStatus::parse(&status).ok_or_else(|| corrupt("delivery status", &status))?,
                response_status: row.last_response_status.and_then(|code| u16::try_from(code).ok()),
                at: Timestamp(at),
            }),
            _ => None,
        };
        Ok(Self {
            id: WebhookId(row.id),
            owner,
            url,
            events: decode_events(&row.events)?,
            active: row.active,
            has_secret: row.has_secret,
            created_by: row.created_by.map(AccountId),
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            last_delivery,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct DeliveryRow {
    id: i64,
    hook_id: i64,
    guid: String,
    event: String,
    action: Option<String>,
    payload: String,
    status: String,
    attempts: i64,
    next_attempt_at: Option<i64>,
    last_attempt_at: Option<i64>,
    request_headers: Option<String>,
    response_status: Option<i64>,
    response_headers: Option<String>,
    response_body: Option<String>,
    duration_ms: Option<i64>,
    error: Option<String>,
    created_at: i64,
}

impl TryFrom<DeliveryRow> for Delivery {
    type Error = StorageError;

    fn try_from(row: DeliveryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: DeliveryId(row.id),
            hook_id: WebhookId(row.hook_id),
            event: WebhookEvent::parse(&row.event).ok_or_else(|| corrupt("delivery event", &row.event))?,
            guid: row.guid,
            action: row.action,
            payload: row.payload,
            status: DeliveryStatus::parse(&row.status).ok_or_else(|| corrupt("delivery status", &row.status))?,
            attempts: u32::try_from(row.attempts).unwrap_or_default(),
            next_attempt_at: row.next_attempt_at.map(Timestamp),
            last_attempt_at: row.last_attempt_at.map(Timestamp),
            request_headers: row.request_headers.as_deref().map(parse_header_block).unwrap_or_default(),
            response_status: row.response_status.and_then(|code| u16::try_from(code).ok()),
            response_headers: row.response_headers.as_deref().map(parse_header_block).unwrap_or_default(),
            response_body: row.response_body,
            duration_ms: row.duration_ms.and_then(|ms| u64::try_from(ms).ok()),
            error: row.error,
            created_at: Timestamp(row.created_at),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimedRow {
    id: i64,
    hook_id: i64,
    guid: String,
    event: String,
    payload: String,
    attempts: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TargetRow {
    id: i64,
    url: String,
    secret: Option<String>,
}

fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

/// `["push","release"]`. Event names are fixed identifiers, so no escaping is needed.
fn encode_events(events: &[WebhookEvent]) -> String {
    let names: Vec<String> = events.iter().map(|event| format!("\"{}\"", event.as_str())).collect();
    format!("[{}]", names.join(","))
}

fn decode_events(text: &str) -> Result<Vec<WebhookEvent>, StorageError> {
    let inner = text.trim().strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).ok_or_else(|| corrupt("webhook events", text))?;
    inner
        .split(',')
        .map(|item| item.trim().trim_matches('"'))
        .filter(|name| !name.is_empty())
        .map(|name| WebhookEvent::parse(name).ok_or_else(|| corrupt("webhook event", name)))
        .collect()
}

/// `(repo_id, org_id)` for binding with `IS ?`.
fn owner_columns(owner: WebhookOwner) -> (Option<i64>, Option<i64>) {
    match owner {
        WebhookOwner::Repository(repo) => (Some(repo.0), None),
        WebhookOwner::Organization(org) => (None, Some(org.0)),
    }
}

fn to_i64(value: impl TryInto<i64>) -> i64 {
    value.try_into().unwrap_or(i64::MAX)
}

#[derive(Debug, Clone)]
pub struct WebhookStore {
    pool: Pool,
}

impl WebhookStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }

    // ------------------------------------------------------------------ hooks

    pub async fn create(&self, new: NewWebhook) -> Result<Webhook, StorageError> {
        let now = Timestamp::now().seconds();
        let (repo_id, org_id) = owner_columns(new.owner);
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO webhooks (id, repo_id, org_id, url, secret, events, active, created_by, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(new.id.map(|id| id.0))
        .bind(repo_id)
        .bind(org_id)
        .bind(new.url.as_str())
        .bind(new.secret.as_deref())
        .bind(encode_events(&new.events))
        .bind(new.active)
        .bind(new.created_by.map(|id| id.0))
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        self.find(new.owner, WebhookId(id)).await?.ok_or_else(|| corrupt("webhook", "missing after insert"))
    }

    /// The largest hook id in use, or 0 without hooks.
    pub async fn max_id(&self) -> Result<WebhookId, StorageError> {
        let id: Option<i64> = sqlx::query_scalar("SELECT MAX(id) FROM webhooks").fetch_one(&self.pool).await?;
        Ok(WebhookId(id.unwrap_or_default()))
    }

    pub async fn count_for(&self, owner: WebhookOwner) -> Result<i64, StorageError> {
        let (repo_id, org_id) = owner_columns(owner);
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM webhooks WHERE repo_id IS ? AND org_id IS ?")
            .bind(repo_id)
            .bind(org_id)
            .fetch_one(&self.pool)
            .await?)
    }

    /// The owner's hooks, oldest first.
    pub async fn list_for(&self, owner: WebhookOwner) -> Result<Vec<Webhook>, StorageError> {
        let (repo_id, org_id) = owner_columns(owner);
        let rows: Vec<WebhookRow> = sqlx::query_as(select_hook!("WHERE w.repo_id IS ? AND w.org_id IS ? ORDER BY w.id"))
            .bind(repo_id)
            .bind(org_id)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Webhook::try_from).collect()
    }

    /// Hook `id`, if it belongs to `owner`.
    pub async fn find(&self, owner: WebhookOwner, id: WebhookId) -> Result<Option<Webhook>, StorageError> {
        let (repo_id, org_id) = owner_columns(owner);
        let row: Option<WebhookRow> = sqlx::query_as(select_hook!("WHERE w.id = ? AND w.repo_id IS ? AND w.org_id IS ?"))
            .bind(id.0)
            .bind(repo_id)
            .bind(org_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Webhook::try_from).transpose()
    }

    /// Replaces the settings of hook `id` if it belongs to `owner`.
    pub async fn update(&self, owner: WebhookOwner, id: WebhookId, update: WebhookUpdate) -> Result<Option<Webhook>, StorageError> {
        let (repo_id, org_id) = owner_columns(owner);
        let (replace_secret, secret) = match update.secret {
            SecretUpdate::Keep => (false, None),
            SecretUpdate::Clear => (true, None),
            SecretUpdate::Set(secret) => (true, Some(secret)),
        };
        let result = sqlx::query(
            "UPDATE webhooks SET url = ?, events = ?, active = ?, updated_at = ?, \
             secret = CASE WHEN ? THEN ? ELSE secret END \
             WHERE id = ? AND repo_id IS ? AND org_id IS ?",
        )
        .bind(update.url.as_str())
        .bind(encode_events(&update.events))
        .bind(update.active)
        .bind(Timestamp::now().seconds())
        .bind(replace_secret)
        .bind(secret)
        .bind(id.0)
        .bind(repo_id)
        .bind(org_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.find(owner, id).await
    }

    /// Deletes hook `id` (and its deliveries) if it belongs to `owner`.
    pub async fn delete(&self, owner: WebhookOwner, id: WebhookId) -> Result<bool, StorageError> {
        let (repo_id, org_id) = owner_columns(owner);
        let result = sqlx::query("DELETE FROM webhooks WHERE id = ? AND repo_id IS ? AND org_id IS ?")
            .bind(id.0)
            .bind(repo_id)
            .bind(org_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Active hooks subscribed to `event` on repository `repo` or on any of `orgs`.
    pub async fn subscribed(&self, repo: Option<RepoId>, orgs: &[AccountId], event: WebhookEvent) -> Result<Vec<WebhookId>, StorageError> {
        let orgs = format!("[{}]", orgs.iter().map(|org| org.0.to_string()).collect::<Vec<_>>().join(","));
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM webhooks \
             WHERE active = 1 AND (repo_id IS NOT NULL AND repo_id = ? OR org_id IN (SELECT value FROM json_each(?))) \
             AND EXISTS (SELECT 1 FROM json_each(webhooks.events) WHERE value = ?) ORDER BY id",
        )
        .bind(repo.map(|repo| repo.0))
        .bind(orgs)
        .bind(event.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(ids.into_iter().map(WebhookId).collect())
    }

    // ------------------------------------------------------------------ queue

    /// Queues deliveries, due now, in one transaction.
    pub async fn enqueue(&self, deliveries: Vec<NewDelivery>) -> Result<Vec<DeliveryId>, StorageError> {
        let now = Timestamp::now().seconds();
        let mut tx = self.pool.begin().await?;
        let mut ids = Vec::with_capacity(deliveries.len());
        for delivery in deliveries {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO webhook_deliveries (hook_id, guid, event, action, payload, status, attempts, next_attempt_at, created_at) \
                 VALUES (?, ?, ?, ?, ?, 'pending', 0, ?, ?) RETURNING id",
            )
            .bind(delivery.hook_id.0)
            .bind(&delivery.guid)
            .bind(delivery.event.as_str())
            .bind(&delivery.action)
            .bind(&delivery.payload)
            .bind(now)
            .bind(now)
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| StorageError::from_insert(error, "delivery id"))?;
            ids.push(DeliveryId(id));
        }
        tx.commit().await?;
        Ok(ids)
    }

    /// Atomically marks up to `limit` due `pending` deliveries as `delivering` and returns
    /// them with their hook's URL and secret. A delivery is claimed by exactly one caller.
    pub async fn claim_due(&self, now: Timestamp, limit: u32) -> Result<Vec<ClaimedDelivery>, StorageError> {
        let claimed: Vec<ClaimedRow> = sqlx::query_as(
            "UPDATE webhook_deliveries SET status = 'delivering' WHERE id IN \
             (SELECT id FROM webhook_deliveries WHERE status = 'pending' AND next_attempt_at <= ? ORDER BY next_attempt_at, id LIMIT ?) \
             RETURNING id, hook_id, guid, event, payload, attempts",
        )
        .bind(now.seconds())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        if claimed.is_empty() {
            return Ok(Vec::new());
        }
        let hook_ids = format!("[{}]", claimed.iter().map(|row| row.hook_id.to_string()).collect::<Vec<_>>().join(","));
        let targets: Vec<TargetRow> = sqlx::query_as("SELECT id, url, secret FROM webhooks WHERE id IN (SELECT value FROM json_each(?))")
            .bind(hook_ids)
            .fetch_all(&self.pool)
            .await?;

        let mut deliveries = Vec::with_capacity(claimed.len());
        for row in claimed {
            let Some(target) = targets.iter().find(|target| target.id == row.hook_id) else {
                // The hook was deleted in between; its deliveries are going with it.
                continue;
            };
            deliveries.push(ClaimedDelivery {
                id: DeliveryId(row.id),
                hook_id: WebhookId(row.hook_id),
                event: WebhookEvent::parse(&row.event).ok_or_else(|| corrupt("delivery event", &row.event))?,
                guid: row.guid,
                payload: row.payload,
                attempts: u32::try_from(row.attempts).unwrap_or_default(),
                url: target.url.clone(),
                secret: target.secret.clone(),
            });
        }
        Ok(deliveries)
    }

    /// Records the outcome of an attempt on a claimed delivery.
    pub async fn record_attempt(&self, id: DeliveryId, attempt: AttemptRecord, outcome: AttemptOutcome) -> Result<(), StorageError> {
        let (status, next_attempt_at) = match outcome {
            AttemptOutcome::Succeeded => (DeliveryStatus::Succeeded, None),
            AttemptOutcome::Failed => (DeliveryStatus::Failed, None),
            AttemptOutcome::Retry(at) => (DeliveryStatus::Pending, Some(at.seconds())),
        };
        sqlx::query(
            "UPDATE webhook_deliveries SET status = ?, attempts = attempts + 1, next_attempt_at = ?, last_attempt_at = ?, \
             request_headers = ?, response_status = ?, response_headers = ?, response_body = ?, duration_ms = ?, error = ? \
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(next_attempt_at)
        .bind(attempt.at.seconds())
        .bind(format_header_block(&attempt.request_headers))
        .bind(attempt.response_status.map(i64::from))
        .bind((!attempt.response_headers.is_empty()).then(|| format_header_block(&attempt.response_headers)))
        .bind(attempt.response_body)
        .bind(to_i64(attempt.duration_ms))
        .bind(attempt.error)
        .bind(id.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns a claimed delivery to the queue without counting an attempt (shutdown
    /// interrupted it).
    pub async fn release(&self, id: DeliveryId) -> Result<(), StorageError> {
        sqlx::query("UPDATE webhook_deliveries SET status = 'pending' WHERE id = ? AND status = 'delivering'")
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns every `delivering` row to the queue: called once at start-up, when nothing
    /// can be in flight (a crash or kill left them claimed). Returns how many.
    pub async fn release_interrupted(&self) -> Result<u64, StorageError> {
        let result =
            sqlx::query("UPDATE webhook_deliveries SET status = 'pending' WHERE status = 'delivering'").execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    /// Deliveries of hook `hook`, newest first.
    pub async fn list_deliveries(&self, hook: WebhookId, limit: u32, offset: u32) -> Result<Vec<Delivery>, StorageError> {
        let rows: Vec<DeliveryRow> = sqlx::query_as(select_delivery!("WHERE hook_id = ? ORDER BY id DESC LIMIT ? OFFSET ?"))
            .bind(hook.0)
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(Delivery::try_from).collect()
    }

    pub async fn count_deliveries(&self, hook: WebhookId) -> Result<i64, StorageError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM webhook_deliveries WHERE hook_id = ?").bind(hook.0).fetch_one(&self.pool).await?)
    }

    /// Delivery `id` of hook `hook`.
    pub async fn find_delivery(&self, hook: WebhookId, id: DeliveryId) -> Result<Option<Delivery>, StorageError> {
        let row: Option<DeliveryRow> =
            sqlx::query_as(select_delivery!("WHERE hook_id = ? AND id = ?")).bind(hook.0).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(Delivery::try_from).transpose()
    }

    /// Deletes finished deliveries created before `cutoff`. Returns how many.
    pub async fn prune(&self, cutoff: Timestamp) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM webhook_deliveries WHERE created_at < ? AND status IN ('succeeded', 'failed')")
            .bind(cutoff.seconds())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_through_their_column() {
        let events = vec![WebhookEvent::Push, WebhookEvent::Release];
        let encoded = encode_events(&events);
        assert_eq!(encoded, r#"["push","release"]"#);
        assert_eq!(decode_events(&encoded).unwrap(), events);
        assert_eq!(decode_events("[]").unwrap(), Vec::new());
        assert_eq!(decode_events(r#"[ "fork" , "push" ]"#).unwrap(), [WebhookEvent::Fork, WebhookEvent::Push]);
        assert!(decode_events(r#"["discussion"]"#).is_err());
        assert!(decode_events("push").is_err());
    }
}
