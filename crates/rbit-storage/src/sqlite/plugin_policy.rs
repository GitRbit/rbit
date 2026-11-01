//! Passive managed-package decisions. Open permits administration, never execution.
//! No runtime consumer exists; every package remains unavailable. A future adapter
//! must enforce the persistent seal before admission, including stale generations.

use std::{collections::BTreeMap, fmt::Write as _};

use rbit_core::{AccountId, Timestamp};
use rbit_plugin_api::{
    CapabilityId, ExpectedPolicyRevision, HostPolicyFingerprint, InstalledPackageRef, MAX_POLICY_REQUEST_BYTES, PackageBudgets,
    PackageKind, PackageManifest, PackageReviewIntent, PackageReviewRequest, PackageSelectionClearRequest, PackageSelectionRequest,
    PluginId, PolicyRevision, Sha256Digest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use super::{plugin_installations::OperationId, pool::Pool};
use crate::StorageError;

pub const MAX_POLICY_HISTORY: i64 = 4096;
pub const MAX_POLICY_RECEIPTS: i64 = 4096;
pub const MAX_POLICY_HEADS: i64 = 1024;
const CATALOG: &str = "empty-v1";
const DESCRIPTOR: &[u8] = b"catalog=empty-v1;execution=unavailable;grants=none;schema=1;limits=unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyGateState {
    Uninitialized,
    Open,
    Sealed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicySealReason {
    HistoryCapacity,
    ReceiptCapacity,
    ReviewHeadCapacity,
    SelectionHeadCapacity,
    RevisionExhausted,
}
impl PolicySealReason {
    const fn code(self) -> i64 {
        match self {
            Self::HistoryCapacity => 0,
            Self::ReceiptCapacity => 1,
            Self::ReviewHeadCapacity => 2,
            Self::SelectionHeadCapacity => 3,
            Self::RevisionExhausted => 4,
        }
    }
    fn decode(value: i64) -> Result<Self, StorageError> {
        match value {
            0 => Ok(Self::HistoryCapacity),
            1 => Ok(Self::ReceiptCapacity),
            2 => Ok(Self::ReviewHeadCapacity),
            3 => Ok(Self::SelectionHeadCapacity),
            4 => Ok(Self::RevisionExhausted),
            _ => Err(invalid()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySeal {
    pub reason: PolicySealReason,
    pub actor: Option<AccountId>,
    pub operation: OperationId,
    pub request: String,
    pub at: Timestamp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyGate {
    pub state: PolicyGateState,
    pub epoch: Option<Sha256Digest>,
    pub fingerprint: Option<HostPolicyFingerprint>,
    pub seal: Option<PolicySeal>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "request", rename_all = "snake_case", deny_unknown_fields)]
pub enum PolicyCommand {
    Review(PackageReviewRequest),
    Select(PackageSelectionRequest),
    Clear(PackageSelectionClearRequest),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanonicalCommand {
    encoding_version: u32,
    command: PolicyCommand,
}
impl PolicyCommand {
    fn validate(&self) -> Result<(), StorageError> {
        match self {
            Self::Review(r) => r.validate(),
            Self::Select(r) => r.validate(),
            Self::Clear(r) => r.validate(),
        }
        .map_err(|_| invalid())
    }
    fn kind(&self) -> i64 {
        match self {
            Self::Review(_) => 0,
            Self::Select(_) => 1,
            Self::Clear(_) => 2,
        }
    }
    fn plugin(&self) -> &PluginId {
        match self {
            Self::Review(r) => r.package.plugin_id(),
            Self::Select(r) => r.package.plugin_id(),
            Self::Clear(r) => &r.plugin_id,
        }
    }
    fn package(&self) -> Option<&InstalledPackageRef> {
        match self {
            Self::Review(r) => Some(&r.package),
            Self::Select(r) => Some(&r.package),
            Self::Clear(_) => None,
        }
    }
    fn expected(&self) -> ExpectedPolicyRevision {
        match self {
            Self::Review(r) => r.expected_review,
            Self::Select(r) => r.expected_selection,
            Self::Clear(r) => r.expected_selection,
        }
    }
    fn fingerprint(&self) -> &HostPolicyFingerprint {
        match self {
            Self::Review(r) => &r.expected_host_policy,
            Self::Select(r) => &r.expected_host_policy,
            Self::Clear(r) => &r.expected_host_policy,
        }
    }
    /// Versioned canonical receipt representation; contains no authorization.
    pub fn canonical_request(&self) -> Result<String, StorageError> {
        self.validate()?;
        let result = serde_json::to_string(&CanonicalCommand { encoding_version: 1, command: self.clone() }).map_err(|_| invalid())?;
        if result.len() > MAX_POLICY_REQUEST_BYTES {
            return Err(invalid());
        }
        Ok(result)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyReceipt {
    pub id: OperationId,
    pub actor: Option<AccountId>,
    pub command: PolicyCommand,
    pub result_revision: PolicyRevision,
    pub epoch: Sha256Digest,
    pub fingerprint: HostPolicyFingerprint,
    pub created_at: Timestamp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyReview {
    pub request: PackageReviewRequest,
    pub revision: PolicyRevision,
    pub actor: Option<AccountId>,
    pub created_at: Timestamp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySelection {
    pub plugin_id: PluginId,
    pub revision: PolicyRevision,
    pub package: Option<InstalledPackageRef>,
    pub review_revision: Option<PolicyRevision>,
    pub actor: Option<AccountId>,
    pub updated_at: Timestamp,
}
/// Inspection is passive. Neither Open nor a historical allow is runnable authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySnapshot {
    pub gate: PolicyGate,
    pub selection: Option<PolicySelection>,
    pub review: Option<PolicyReview>,
}
/// Bounded host-read declarations for informed review, never approved authority.
/// The archive file inventory is intentionally omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyPackageDeclarations {
    pub kind: PackageKind,
    pub capabilities: Vec<CapabilityId>,
    pub host_api: semver::VersionReq,
    pub budgets: PackageBudgets,
}
/// One authoritative passive proposal snapshot, not an execution permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyPackageSnapshot {
    pub gate: PolicyGate,
    pub package: InstalledPackageRef,
    pub declarations: PolicyPackageDeclarations,
    pub review: Option<PolicyReview>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied { receipt: PolicyReceipt, gate: PolicyGate },
    HistoricalReceipt { receipt: PolicyReceipt, gate: PolicyGate },
    GateSealed { gate: PolicyGate },
    CapacitySealed { gate: PolicyGate },
}
#[derive(Debug, Clone)]
pub struct PluginPolicyStore {
    pool: Pool,
}
impl PluginPolicyStore {
    pub(crate) fn new(pool: Pool) -> Self {
        Self { pool }
    }
    /// Explicit authenticated initialization only. Reads/construction never open a
    /// gate. Randomness is generated before acquiring the FULL write transaction.
    pub async fn prepare(&self, actor: AccountId, now: Timestamp) -> Result<PolicyGate, StorageError> {
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|_| invalid())?;
        let epoch: Sha256Digest = hex(&random).parse().map_err(|_| invalid())?;
        let fingerprint = policy_fingerprint(&epoch)?;
        let mut tx = self.pool.begin_immediate().await?;
        authorize(&mut *tx, actor).await?;
        let gate = load_gate(&mut *tx).await?;
        if gate.state != PolicyGateState::Uninitialized {
            return Err(StorageError::Conflict("policy already prepared or sealed"));
        }
        for table in ["plugin_review_history", "plugin_review_heads", "plugin_selection_heads", "plugin_policy_receipts"] {
            if count(&mut *tx, table).await? != 0 {
                return Err(invalid());
            }
        }
        sqlx::query("UPDATE plugin_policy_gate SET state=1,epoch=?,fingerprint=?,prepared_by=?,prepared_at=? WHERE id=1 AND state=0")
            .bind(epoch.as_str())
            .bind(fingerprint.as_str())
            .bind(actor.0)
            .bind(now.0)
            .execute(&mut *tx)
            .await?;
        let result = load_gate(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn gate(&self, actor: AccountId) -> Result<PolicyGate, StorageError> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut *tx, actor).await?;
        let gate = load_gate(&mut *tx).await?;
        tx.commit().await?;
        Ok(gate)
    }
    pub async fn inspect(&self, actor: AccountId, id: &PluginId) -> Result<PolicySnapshot, StorageError> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut *tx, actor).await?;
        let gate = load_gate(&mut *tx).await?;
        let selection = load_selection(&mut *tx, id).await?;
        let review = if let Some(package) = selection.as_ref().and_then(|s| s.package.as_ref()) {
            load_review(&mut *tx, package.archive_digest()).await?
        } else {
            None
        };
        tx.commit().await?;
        Ok(PolicySnapshot { gate, selection, review })
    }
    /// Inspect an installed candidate even before it is selected.
    pub async fn inspect_package(&self, actor: AccountId, package: &InstalledPackageRef) -> Result<PolicyPackageSnapshot, StorageError> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut *tx, actor).await?;
        let gate = load_gate(&mut *tx).await?;
        let manifest = installed_manifest(&mut *tx, package).await?;
        let review = load_review(&mut *tx, package.archive_digest()).await?;
        if review.as_ref().is_some_and(|review| review.request.package != *package) {
            return Err(invalid());
        }
        tx.commit().await?;
        Ok(PolicyPackageSnapshot {
            gate,
            package: package.clone(),
            declarations: PolicyPackageDeclarations {
                kind: manifest.kind,
                capabilities: manifest.plugin.capabilities,
                host_api: manifest.plugin.host_api,
                budgets: manifest.budgets,
            },
            review,
        })
    }
    pub async fn operation(&self, actor: AccountId, id: &OperationId) -> Result<Option<PolicyReceipt>, StorageError> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut *tx, actor).await?;
        let result = sqlx::query("SELECT * FROM plugin_policy_receipts WHERE id=? AND length(CAST(request AS BLOB))<=65536")
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await?
            .as_ref()
            .map(receipt)
            .transpose()?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn apply_review(
        &self,
        actor: AccountId,
        id: &OperationId,
        request: &PackageReviewRequest,
        now: Timestamp,
    ) -> Result<ApplyOutcome, StorageError> {
        self.apply(actor, id, PolicyCommand::Review(request.clone()), now).await
    }
    pub async fn apply_selection(
        &self,
        actor: AccountId,
        id: &OperationId,
        request: &PackageSelectionRequest,
        now: Timestamp,
    ) -> Result<ApplyOutcome, StorageError> {
        self.apply(actor, id, PolicyCommand::Select(request.clone()), now).await
    }
    pub async fn apply_clear(
        &self,
        actor: AccountId,
        id: &OperationId,
        request: &PackageSelectionClearRequest,
        now: Timestamp,
    ) -> Result<ApplyOutcome, StorageError> {
        self.apply(actor, id, PolicyCommand::Clear(request.clone()), now).await
    }
    async fn apply(
        &self,
        actor: AccountId,
        id: &OperationId,
        command: PolicyCommand,
        now: Timestamp,
    ) -> Result<ApplyOutcome, StorageError> {
        let canonical = command.canonical_request()?;
        let mut tx = self.pool.begin_immediate().await?;
        authorize(&mut *tx, actor).await?;
        let mut gate = load_gate(&mut *tx).await?;
        if gate.state == PolicyGateState::Sealed {
            tx.commit().await?;
            return Ok(ApplyOutcome::GateSealed { gate });
        }
        if gate.state != PolicyGateState::Open {
            return Err(StorageError::Conflict("policy not prepared"));
        }
        if let Some(row) = sqlx::query("SELECT * FROM plugin_policy_receipts WHERE id=? AND length(CAST(request AS BLOB))<=65536")
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        {
            let existing = receipt(&row)?;
            if existing.actor != Some(actor) || row.try_get::<String, _>("request")? != canonical {
                return Err(StorageError::Conflict("policy operation identity"));
            }
            tx.commit().await?;
            return Ok(ApplyOutcome::HistoricalReceipt { receipt: existing, gate });
        }
        if gate.fingerprint.as_ref() != Some(command.fingerprint()) {
            return Err(StorageError::Conflict("host policy fingerprint"));
        }
        if let Some(package) = command.package() {
            let manifest = installed_manifest(&mut *tx, package).await?;
            if let PolicyCommand::Review(request) = &command {
                request
                    .validate_manifest_declarations(&manifest)
                    .map_err(|_| StorageError::Conflict("package review declaration mismatch"))?;
                if !request.grants.as_slice().is_empty() {
                    return Err(StorageError::Conflict("empty host capability catalog"));
                }
            }
        }
        let review = if let PolicyCommand::Review(r) = &command { load_review(&mut *tx, r.package.archive_digest()).await? } else { None };
        let selection = if matches!(command, PolicyCommand::Review(_)) { None } else { load_selection(&mut *tx, command.plugin()).await? };
        let current = review.as_ref().map(|r| r.revision).or_else(|| selection.as_ref().map(|s| s.revision));
        if !expected_matches(command.expected(), current) {
            return Err(StorageError::Conflict("policy revision"));
        }
        if let PolicyCommand::Select(r) = &command {
            let reviewed =
                load_review(&mut *tx, r.package.archive_digest()).await?.ok_or(StorageError::Conflict("package not reviewed"))?;
            if reviewed.request.package != r.package
                || reviewed.revision != r.expected_review_revision
                || matches!(reviewed.request.intent, PackageReviewIntent::Deny {})
                || reviewed.request.expected_host_policy != *command.fingerprint()
            {
                return Err(StorageError::Conflict("current package review"));
            }
        }
        // A clear requires a retained head; clearing a never-created selection is
        // a conflict rather than allocating an ambiguous absent tombstone.
        if matches!(command, PolicyCommand::Clear(_)) && selection.is_none() {
            return Err(StorageError::Conflict("selection absent"));
        }
        let new_head = if matches!(command, PolicyCommand::Review(_)) { review.is_none() } else { selection.is_none() };
        let (failure, next) = capacity_failure(&mut *tx, &command, new_head, current).await?;
        if let Some(reason) = failure {
            sqlx::query("UPDATE plugin_policy_gate SET state=2,seal_reason=?,seal_actor=?,seal_operation=?,seal_request=?,sealed_at=? WHERE id=1 AND state=1")
                .bind(reason.code()).bind(actor.0).bind(id.as_str()).bind(&canonical).bind(now.0).execute(&mut *tx).await?;
            gate = load_gate(&mut *tx).await?;
            tx.commit().await?;
            return Ok(ApplyOutcome::CapacitySealed { gate });
        }
        let revision = PolicyRevision::new(next.ok_or_else(invalid)?).map_err(|_| invalid())?;
        let epoch = gate.epoch.as_ref().ok_or_else(invalid)?;
        let fingerprint = gate.fingerprint.as_ref().ok_or_else(invalid)?;
        match &command {
            PolicyCommand::Review(r) => {
                sqlx::query("INSERT INTO plugin_review_history(digest,revision,plugin_id,version,intent,request,catalog_version,epoch,fingerprint,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                    .bind(r.package.archive_digest().as_str()).bind(integer(revision)?).bind(r.package.plugin_id().as_str()).bind(r.package.version().to_string()).bind(intent(r.intent)).bind(&canonical).bind(CATALOG).bind(epoch.as_str()).bind(fingerprint.as_str()).bind(actor.0).bind(now.0).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO plugin_review_heads(digest,revision) VALUES(?,?) ON CONFLICT(digest) DO UPDATE SET revision=excluded.revision")
                    .bind(r.package.archive_digest().as_str()).bind(integer(revision)?).execute(&mut *tx).await?;
            }
            PolicyCommand::Select(r) => {
                sqlx::query("INSERT INTO plugin_selection_heads(plugin_id,revision,version,digest,review_revision,actor,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(plugin_id) DO UPDATE SET revision=excluded.revision,version=excluded.version,digest=excluded.digest,review_revision=excluded.review_revision,actor=excluded.actor,updated_at=excluded.updated_at")
                .bind(r.package.plugin_id().as_str()).bind(integer(revision)?).bind(r.package.version().to_string()).bind(r.package.archive_digest().as_str()).bind(integer(r.expected_review_revision)?).bind(actor.0).bind(now.0).execute(&mut *tx).await?;
            }
            PolicyCommand::Clear(r) => {
                sqlx::query("UPDATE plugin_selection_heads SET revision=?,version=NULL,digest=NULL,review_revision=NULL,actor=?,updated_at=? WHERE plugin_id=?")
                .bind(integer(revision)?).bind(actor.0).bind(now.0).bind(r.plugin_id.as_str()).execute(&mut *tx).await?;
            }
        }
        sqlx::query("INSERT INTO plugin_policy_receipts(id,kind,request,actor,plugin_id,digest,result_revision,epoch,fingerprint,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(id.as_str()).bind(command.kind()).bind(&canonical).bind(actor.0).bind(command.plugin().as_str()).bind(command.package().map(|p|p.archive_digest().as_str())).bind(integer(revision)?).bind(epoch.as_str()).bind(fingerprint.as_str()).bind(now.0).execute(&mut *tx).await?;
        let receipt = PolicyReceipt {
            id: id.clone(),
            actor: Some(actor),
            command,
            result_revision: revision,
            epoch: epoch.clone(),
            fingerprint: fingerprint.clone(),
            created_at: now,
        };
        tx.commit().await?;
        Ok(ApplyOutcome::Applied { receipt, gate })
    }
}

async fn authorize<C>(connection: &mut C, actor: AccountId) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts a JOIN user_logins l ON l.account_id=a.id WHERE a.id=? AND a.kind='user' AND l.is_admin=1 AND l.suspended_at IS NULL AND l.password_change_required=0)")
        .bind(actor.0).fetch_one(connection).await?;
    if !allowed {
        return Err(StorageError::Conflict("active administrator required"));
    }
    Ok(())
}
async fn count<C>(connection: &mut C, table: &str) -> Result<i64, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let query = match table {
        "plugin_review_history" => "SELECT COUNT(*) FROM plugin_review_history",
        "plugin_review_heads" => "SELECT COUNT(*) FROM plugin_review_heads",
        "plugin_selection_heads" => "SELECT COUNT(*) FROM plugin_selection_heads",
        "plugin_policy_receipts" => "SELECT COUNT(*) FROM plugin_policy_receipts",
        _ => return Err(invalid()),
    };
    Ok(sqlx::query_scalar(query).fetch_one(connection).await?)
}
fn expected_matches(expected: ExpectedPolicyRevision, current: Option<PolicyRevision>) -> bool {
    match (expected, current) {
        (ExpectedPolicyRevision::Absent {}, None) => true,
        (ExpectedPolicyRevision::Exact { revision }, Some(current)) => revision == current,
        _ => false,
    }
}
fn integer(revision: PolicyRevision) -> Result<i64, StorageError> {
    i64::try_from(revision.get()).map_err(|_| invalid())
}
fn revision(value: i64) -> Result<PolicyRevision, StorageError> {
    PolicyRevision::new(u64::try_from(value).map_err(|_| invalid())?).map_err(|_| invalid())
}
fn intent(value: PackageReviewIntent) -> i64 {
    match value {
        PackageReviewIntent::Deny {} => 0,
        PackageReviewIntent::DataOnly {} => 1,
        PackageReviewIntent::NativeFullServiceUserTrust {} => 2,
        PackageReviewIntent::WasmSandboxRequired {} => 3,
    }
}
fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(result, "{byte:02x}").expect("String writes cannot fail");
    }
    result
}
fn policy_fingerprint(epoch: &Sha256Digest) -> Result<HostPolicyFingerprint, StorageError> {
    let mut hash = Sha256::new();
    hash.update(b"rbit-package-policy-v1\0");
    hash.update(epoch.as_str().as_bytes());
    hash.update(b"\0");
    hash.update(DESCRIPTOR);
    Ok(HostPolicyFingerprint::new(hex(&hash.finalize()).parse().map_err(|_| invalid())?))
}
fn invalid() -> StorageError {
    StorageError::Corrupt("invalid package policy ledger".to_owned())
}
fn command(value: &str) -> Result<PolicyCommand, StorageError> {
    if value.len() > MAX_POLICY_REQUEST_BYTES {
        return Err(invalid());
    }
    let wire: CanonicalCommand = serde_json::from_str(value).map_err(|_| invalid())?;
    if wire.encoding_version != 1 || wire.command.canonical_request()? != value {
        return Err(invalid());
    }
    Ok(wire.command)
}
async fn load_gate<C>(connection: &mut C) -> Result<PolicyGate, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let row =
        sqlx::query("SELECT * FROM plugin_policy_gate WHERE id=1 AND (SELECT COUNT(*) FROM plugin_policy_gate)=1 AND (epoch IS NULL OR length(CAST(epoch AS BLOB))=64) AND (fingerprint IS NULL OR length(CAST(fingerprint AS BLOB))=64) AND (seal_operation IS NULL OR length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64) AND (seal_request IS NULL OR length(CAST(seal_request AS BLOB))<=65536)")
            .fetch_optional(connection)
            .await?
            .ok_or_else(invalid)?;
    let state = match row.try_get::<i64, _>("state")? {
        0 => PolicyGateState::Uninitialized,
        1 => PolicyGateState::Open,
        2 => PolicyGateState::Sealed,
        _ => return Err(invalid()),
    };
    let epoch = row.try_get::<Option<String>, _>("epoch")?.map(|s| s.parse::<Sha256Digest>().map_err(|_| invalid())).transpose()?;
    let fingerprint = row
        .try_get::<Option<String>, _>("fingerprint")?
        .map(|s| s.parse::<Sha256Digest>().map(HostPolicyFingerprint::new).map_err(|_| invalid()))
        .transpose()?;
    let prepared_by: Option<i64> = row.try_get("prepared_by")?;
    let prepared_at: Option<i64> = row.try_get("prepared_at")?;
    let seal_reason: Option<i64> = row.try_get("seal_reason")?;
    let seal_actor: Option<i64> = row.try_get("seal_actor")?;
    let seal_operation: Option<String> = row.try_get("seal_operation")?;
    let seal_request: Option<String> = row.try_get("seal_request")?;
    let sealed_at: Option<i64> = row.try_get("sealed_at")?;
    if state == PolicyGateState::Uninitialized
        && (epoch.is_some() || fingerprint.is_some() || prepared_by.is_some() || prepared_at.is_some())
    {
        return Err(invalid());
    }
    if state != PolicyGateState::Uninitialized && prepared_at.is_none() {
        return Err(invalid());
    }
    if state != PolicyGateState::Sealed
        && (seal_reason.is_some() || seal_actor.is_some() || seal_operation.is_some() || seal_request.is_some() || sealed_at.is_some())
    {
        return Err(invalid());
    }
    if state != PolicyGateState::Uninitialized && fingerprint.as_ref() != Some(&policy_fingerprint(epoch.as_ref().ok_or_else(invalid)?)?) {
        return Err(invalid());
    }
    let seal = if state == PolicyGateState::Sealed {
        let request: String = row.try_get("seal_request")?;
        command(&request)?;
        Some(PolicySeal {
            reason: PolicySealReason::decode(row.try_get("seal_reason")?)?,
            actor: row.try_get::<Option<i64>, _>("seal_actor")?.map(AccountId),
            operation: OperationId::new(&row.try_get::<String, _>("seal_operation")?)?,
            request,
            at: Timestamp(row.try_get("sealed_at")?),
        })
    } else {
        None
    };
    Ok(PolicyGate { state, epoch, fingerprint, seal })
}
async fn installed_manifest<C>(connection: &mut C, package: &InstalledPackageRef) -> Result<PackageManifest, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let json:Option<String>=sqlx::query_scalar("SELECT o.manifest FROM plugin_installations i JOIN plugin_objects o ON o.digest=i.digest WHERE i.plugin_id=? AND i.version=? AND i.digest=? AND length(CAST(o.manifest AS BLOB))<=1048576")
        .bind(package.plugin_id().as_str()).bind(package.version().to_string()).bind(package.archive_digest().as_str()).fetch_optional(connection).await?;
    let manifest: PackageManifest =
        serde_json::from_str(&json.ok_or(StorageError::Conflict("package not installed"))?).map_err(|_| invalid())?;
    if &manifest.plugin.id != package.plugin_id() || &manifest.plugin.version != package.version() {
        return Err(invalid());
    }
    Ok(manifest)
}
async fn load_review<C>(connection: &mut C, digest: &Sha256Digest) -> Result<Option<PolicyReview>, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    sqlx::query("SELECT h.* FROM plugin_review_heads c JOIN plugin_review_history h ON h.digest=c.digest AND h.revision=c.revision WHERE c.digest=? AND length(CAST(h.request AS BLOB))<=65536")
        .bind(digest.as_str()).fetch_optional(connection).await?.as_ref().map(review).transpose()
}
fn review(row: &SqliteRow) -> Result<PolicyReview, StorageError> {
    let PolicyCommand::Review(request) = command(&row.try_get::<String, _>("request")?)? else {
        return Err(invalid());
    };
    if request.package.archive_digest().as_str() != row.try_get::<String, _>("digest")?
        || request.package.plugin_id().as_str() != row.try_get::<String, _>("plugin_id")?
        || request.package.version().to_string() != row.try_get::<String, _>("version")?
        || intent(request.intent) != row.try_get::<i64, _>("intent")?
        || !request.grants.as_slice().is_empty()
        || row.try_get::<String, _>("catalog_version")? != CATALOG
        || request.expected_host_policy.as_str() != row.try_get::<String, _>("fingerprint")?
    {
        return Err(invalid());
    }
    Ok(PolicyReview {
        request,
        revision: revision(row.try_get("revision")?)?,
        actor: row.try_get::<Option<i64>, _>("actor")?.map(AccountId),
        created_at: Timestamp(row.try_get("created_at")?),
    })
}
async fn load_selection<C>(connection: &mut C, id: &PluginId) -> Result<Option<PolicySelection>, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let Some(row) =
        sqlx::query("SELECT * FROM plugin_selection_heads WHERE plugin_id=?").bind(id.as_str()).fetch_optional(connection).await?
    else {
        return Ok(None);
    };
    let version: Option<String> = row.try_get("version")?;
    let digest: Option<String> = row.try_get("digest")?;
    let review: Option<i64> = row.try_get("review_revision")?;
    let package = match (version, digest, review) {
        (Some(version), Some(digest), Some(_)) => Some(
            InstalledPackageRef::new(id.clone(), version.parse().map_err(|_| invalid())?, digest.parse().map_err(|_| invalid())?)
                .map_err(|_| invalid())?,
        ),
        (None, None, None) => None,
        _ => return Err(invalid()),
    };
    Ok(Some(PolicySelection {
        plugin_id: id.clone(),
        revision: revision(row.try_get("revision")?)?,
        package,
        review_revision: review.map(revision).transpose()?,
        actor: row.try_get::<Option<i64>, _>("actor")?.map(AccountId),
        updated_at: Timestamp(row.try_get("updated_at")?),
    }))
}
fn receipt(row: &SqliteRow) -> Result<PolicyReceipt, StorageError> {
    let command = command(&row.try_get::<String, _>("request")?)?;
    if command.kind() != row.try_get::<i64, _>("kind")?
        || command.plugin().as_str() != row.try_get::<String, _>("plugin_id")?
        || command.package().map(|p| p.archive_digest().as_str()) != row.try_get::<Option<String>, _>("digest")?.as_deref()
        || command.fingerprint().as_str() != row.try_get::<String, _>("fingerprint")?
    {
        return Err(invalid());
    }
    Ok(PolicyReceipt {
        id: OperationId::new(&row.try_get::<String, _>("id")?)?,
        actor: row.try_get::<Option<i64>, _>("actor")?.map(AccountId),
        command,
        result_revision: revision(row.try_get("result_revision")?)?,
        epoch: row.try_get::<String, _>("epoch")?.parse().map_err(|_| invalid())?,
        fingerprint: HostPolicyFingerprint::new(row.try_get::<String, _>("fingerprint")?.parse().map_err(|_| invalid())?),
        created_at: Timestamp(row.try_get("created_at")?),
    })
}

/// Validates all present policy tables in an immutable backup snapshot. The caller
/// decides legacy table absence/migration policy. This issues no writes and grants
/// no execution authority. Work is bounded by row/input limits; rows are paged.
pub async fn validate_snapshot(connection: &mut SqliteConnection) -> Result<(), StorageError> {
    let gates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_gate").fetch_one(&mut *connection).await?;
    if gates != 1 {
        return Err(invalid());
    }
    let gate = load_gate(connection).await?;
    for (table, limit) in [
        ("plugin_review_history", MAX_POLICY_HISTORY),
        ("plugin_review_heads", MAX_POLICY_HEADS),
        ("plugin_selection_heads", MAX_POLICY_HEADS),
        ("plugin_policy_receipts", MAX_POLICY_RECEIPTS),
    ] {
        let rows = count(connection, table).await?;
        if rows > limit || (gate.state == PolicyGateState::Uninitialized && rows != 0) {
            return Err(invalid());
        }
    }
    let malformed: i64 = sqlx::query_scalar("SELECT         (SELECT COUNT(*) FROM plugin_review_history WHERE length(CAST(digest AS BLOB))<>64 OR digest GLOB '*[^0-9a-f]*' OR length(CAST(plugin_id AS BLOB)) NOT BETWEEN 3 AND 128 OR length(CAST(version AS BLOB)) NOT BETWEEN 1 AND 128 OR length(CAST(epoch AS BLOB))<>64 OR epoch GLOB '*[^0-9a-f]*' OR length(CAST(fingerprint AS BLOB))<>64 OR fingerprint GLOB '*[^0-9a-f]*' OR length(CAST(catalog_version AS BLOB))<>8) +         (SELECT COUNT(*) FROM plugin_policy_receipts WHERE length(CAST(id AS BLOB)) NOT BETWEEN 1 AND 64 OR id GLOB '*[^a-z0-9-]*' OR length(CAST(plugin_id AS BLOB)) NOT BETWEEN 3 AND 128 OR (digest IS NOT NULL AND (length(CAST(digest AS BLOB))<>64 OR digest GLOB '*[^0-9a-f]*')) OR length(CAST(epoch AS BLOB))<>64 OR epoch GLOB '*[^0-9a-f]*' OR length(CAST(fingerprint AS BLOB))<>64 OR fingerprint GLOB '*[^0-9a-f]*') +         (SELECT COUNT(*) FROM plugin_review_heads WHERE length(CAST(digest AS BLOB))<>64 OR digest GLOB '*[^0-9a-f]*') +         (SELECT COUNT(*) FROM plugin_selection_heads WHERE length(CAST(plugin_id AS BLOB)) NOT BETWEEN 3 AND 128 OR (version IS NOT NULL AND length(CAST(version AS BLOB)) NOT BETWEEN 1 AND 128) OR (digest IS NOT NULL AND (length(CAST(digest AS BLOB))<>64 OR digest GLOB '*[^0-9a-f]*')))")
        .fetch_one(&mut *connection).await?;
    if malformed != 0 {
        return Err(invalid());
    }
    let mut manifests = BTreeMap::new();
    // Restrict the pragma itself, rather than filtering its output after it has
    // walked unrelated application tables.
    for table in ["plugin_policy_gate", "plugin_review_history", "plugin_review_heads", "plugin_selection_heads", "plugin_policy_receipts"]
    {
        let broken: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check(?)").bind(table).fetch_one(&mut *connection).await?;
        if broken != 0 {
            return Err(invalid());
        }
    }
    let bad:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM plugin_review_history WHERE length(CAST(request AS BLOB)) NOT BETWEEN 1 AND 65536)+(SELECT COUNT(*) FROM plugin_policy_receipts WHERE length(CAST(request AS BLOB)) NOT BETWEEN 1 AND 65536)").fetch_one(&mut *connection).await?;
    if bad != 0 {
        return Err(invalid());
    }
    let broken_heads:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM plugin_review_heads h WHERE h.revision<>(SELECT MAX(r.revision) FROM plugin_review_history r WHERE r.digest=h.digest))+(SELECT COUNT(*) FROM (SELECT DISTINCT digest FROM plugin_review_history) r LEFT JOIN plugin_review_heads h USING(digest) WHERE h.digest IS NULL)").fetch_one(&mut *connection).await?;
    if broken_heads != 0 {
        return Err(invalid());
    }
    validate_history(connection, &gate, &mut manifests).await?;
    validate_receipts(connection, &gate, &mut manifests).await?;
    validate_selections(connection).await?;

    Ok(())
}
async fn check_selection_receipt(connection: &mut SqliteConnection, receipt: &PolicyReceipt) -> Result<(), StorageError> {
    let head = load_selection(connection, receipt.command.plugin()).await?.ok_or_else(invalid)?;
    if receipt.result_revision > head.revision {
        return Err(invalid());
    }
    if receipt.result_revision.get() > 1 {
        let previous = integer(receipt.result_revision)? - 1;
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_receipts WHERE plugin_id=? AND kind IN (1,2) AND result_revision=?")
                .bind(receipt.command.plugin().as_str())
                .bind(previous)
                .fetch_one(connection)
                .await?;
        if count != 1 {
            return Err(invalid());
        }
    }
    Ok(())
}

async fn cached_kind(
    connection: &mut SqliteConnection,
    package: &InstalledPackageRef,
    cache: &mut BTreeMap<(String, String, String), PackageKind>,
) -> Result<PackageKind, StorageError> {
    let key = (package.plugin_id().to_string(), package.version().to_string(), package.archive_digest().to_string());
    if let Some(kind) = cache.get(&key) {
        return Ok(kind.clone());
    }
    if cache.len() >= 1024 {
        return Err(invalid());
    }
    let manifest = installed_manifest(connection, package).await?;
    let kind = manifest.kind;
    cache.insert(key, kind.clone());
    Ok(kind)
}

async fn capacity_failure<C>(
    connection: &mut C,
    command: &PolicyCommand,
    new_head: bool,
    current: Option<PolicyRevision>,
) -> Result<(Option<PolicySealReason>, Option<u64>), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let mut failure = None;
    if count(connection, "plugin_policy_receipts").await? >= MAX_POLICY_RECEIPTS {
        failure = Some(PolicySealReason::ReceiptCapacity);
    }
    if matches!(command, PolicyCommand::Review(_)) {
        if count(connection, "plugin_review_history").await? >= MAX_POLICY_HISTORY {
            failure = Some(PolicySealReason::HistoryCapacity);
        }
        if new_head && count(connection, "plugin_review_heads").await? >= MAX_POLICY_HEADS {
            failure = Some(PolicySealReason::ReviewHeadCapacity);
        }
    } else if new_head && count(connection, "plugin_selection_heads").await? >= MAX_POLICY_HEADS {
        failure = Some(PolicySealReason::SelectionHeadCapacity);
    }
    let next = current.map_or(Some(1), |r| r.get().checked_add(1).filter(|v| *v <= PolicyRevision::MAX));
    if next.is_none() {
        failure = Some(PolicySealReason::RevisionExhausted);
    }
    Ok((failure, next))
}

type ManifestKinds = BTreeMap<(String, String, String), PackageKind>;
async fn validate_history(connection: &mut SqliteConnection, gate: &PolicyGate, manifests: &mut ManifestKinds) -> Result<(), StorageError> {
    let mut after_digest = String::new();
    let mut after_revision = 0_i64;
    loop {
        let rows = sqlx::query(
            "SELECT * FROM plugin_review_history WHERE digest>? OR (digest=? AND revision>?) ORDER BY digest,revision LIMIT 128",
        )
        .bind(&after_digest)
        .bind(&after_digest)
        .bind(after_revision)
        .fetch_all(&mut *connection)
        .await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let reviewed = review(&row)?;
            let epoch: Sha256Digest = row.try_get::<String, _>("epoch")?.parse().map_err(|_| invalid())?;
            if gate.epoch.as_ref() != Some(&epoch) || gate.fingerprint.as_ref() != Some(&reviewed.request.expected_host_policy) {
                return Err(invalid());
            }
            let kind = cached_kind(connection, &reviewed.request.package, manifests).await?;
            let compatible = matches!(
                (reviewed.request.intent, kind),
                (PackageReviewIntent::Deny {}, _)
                    | (PackageReviewIntent::DataOnly {}, PackageKind::Data {})
                    | (PackageReviewIntent::NativeFullServiceUserTrust {}, PackageKind::Native { .. })
                    | (PackageReviewIntent::WasmSandboxRequired {}, PackageKind::Wasm { .. })
            );
            if !compatible {
                return Err(invalid());
            }
            let previous = reviewed.revision.get() - 1;
            if !expected_matches(
                reviewed.request.expected_review,
                if previous == 0 { None } else { Some(PolicyRevision::new(previous).map_err(|_| invalid())?) },
            ) {
                return Err(invalid());
            }
            if previous != 0 {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_review_history WHERE digest=? AND revision=?)")
                    .bind(reviewed.request.package.archive_digest().as_str())
                    .bind(i64::try_from(previous).map_err(|_| invalid())?)
                    .fetch_one(&mut *connection)
                    .await?;
                if !exists {
                    return Err(invalid());
                }
            }
            let receipts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_receipts WHERE kind=0 AND digest=? AND result_revision=? AND request=? AND actor IS ? AND created_at=?")
                .bind(reviewed.request.package.archive_digest().as_str()).bind(integer(reviewed.revision)?).bind(row.try_get::<String,_>("request")?).bind(reviewed.actor.map(|a|a.0)).bind(reviewed.created_at.0).fetch_one(&mut *connection).await?;
            if receipts != 1 {
                return Err(invalid());
            }
            after_digest = row.try_get("digest")?;
            after_revision = row.try_get("revision")?;
        }
    }
    Ok(())
}
async fn validate_receipts(
    connection: &mut SqliteConnection,
    gate: &PolicyGate,
    manifests: &mut ManifestKinds,
) -> Result<(), StorageError> {
    let mut after = String::new();
    loop {
        let rows = sqlx::query("SELECT * FROM plugin_policy_receipts WHERE id>? ORDER BY id LIMIT 128")
            .bind(&after)
            .fetch_all(&mut *connection)
            .await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let received = receipt(&row)?;
            if gate.epoch.as_ref() != Some(&received.epoch) || gate.fingerprint.as_ref() != Some(&received.fingerprint) {
                return Err(invalid());
            }
            let previous = received.result_revision.get() - 1;
            if !expected_matches(
                received.command.expected(),
                if previous == 0 { None } else { Some(PolicyRevision::new(previous).map_err(|_| invalid())?) },
            ) {
                return Err(invalid());
            }
            match &received.command {
                PolicyCommand::Review(r) => {
                    let history = sqlx::query("SELECT * FROM plugin_review_history WHERE digest=? AND revision=?")
                        .bind(r.package.archive_digest().as_str())
                        .bind(integer(received.result_revision)?)
                        .fetch_optional(&mut *connection)
                        .await?
                        .ok_or_else(invalid)?;
                    if review(&history)?.request != *r {
                        return Err(invalid());
                    }
                }
                PolicyCommand::Select(r) => {
                    cached_kind(connection, &r.package, manifests).await?;
                    let history = sqlx::query("SELECT * FROM plugin_review_history WHERE digest=? AND revision=?")
                        .bind(r.package.archive_digest().as_str())
                        .bind(integer(r.expected_review_revision)?)
                        .fetch_optional(&mut *connection)
                        .await?
                        .ok_or_else(invalid)?;
                    let approval = review(&history)?;
                    if approval.request.package != r.package
                        || matches!(approval.request.intent, PackageReviewIntent::Deny {})
                        || approval.request.expected_host_policy != received.fingerprint
                    {
                        return Err(invalid());
                    }
                    check_selection_receipt(connection, &received).await?;
                }
                PolicyCommand::Clear(_) => {
                    if previous == 0 {
                        return Err(invalid());
                    }
                    check_selection_receipt(connection, &received).await?;
                }
            }
            after = received.id.as_str().to_owned();
        }
    }
    Ok(())
}
async fn validate_selections(connection: &mut SqliteConnection) -> Result<(), StorageError> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT plugin_id FROM plugin_selection_heads ORDER BY plugin_id LIMIT 1024")
        .fetch_all(&mut *connection)
        .await?;
    for id in ids {
        let id: PluginId = id.parse().map_err(|_| invalid())?;
        let head = load_selection(connection, &id).await?.ok_or_else(invalid)?;
        let rows=sqlx::query("SELECT * FROM plugin_policy_receipts WHERE plugin_id=? AND kind IN (1,2) AND result_revision=? AND actor IS ? AND created_at=?")
            .bind(id.as_str()).bind(integer(head.revision)?).bind(head.actor.map(|a|a.0)).bind(head.updated_at.0).fetch_all(&mut *connection).await?;
        if rows.len() != 1 {
            return Err(invalid());
        }
        let received = receipt(&rows[0])?;
        match received.command {
            PolicyCommand::Select(r)
                if head.package.as_ref() == Some(&r.package) && head.review_revision == Some(r.expected_review_revision) => {}
            PolicyCommand::Clear(_) if head.package.is_none() && head.review_revision.is_none() => {}
            _ => return Err(invalid()),
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_receipts WHERE plugin_id=? AND kind IN (1,2)")
            .bind(id.as_str())
            .fetch_one(&mut *connection)
            .await?;
        if count != integer(head.revision)? {
            return Err(invalid());
        }
    }
    Ok(())
}
