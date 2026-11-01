//! Frozen migration-0064 policy validator for legacy format-5 backup snapshots.
//!
//! Source: Git commit 751aa01. Retained for pre-0065 format-5 backups.
//! Keep five-table queries, byte bounds, v1 fingerprint domain and canonical
//! request encoding independent of the epoch-qualified runtime introduced in0065.
//! The caller owns schema detection and immutable-connection lifetime. No writes.

use std::{collections::BTreeMap, fmt::Write as _};

use rbit_core::{AccountId, Timestamp};
use rbit_plugin_api::{
    ExpectedPolicyRevision, HostPolicyFingerprint, InstalledPackageRef, PackageKind, PackageReviewIntent, PluginId, PolicyRevision,
    Sha256Digest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use super::{
    OperationId, PolicyCommand, PolicyGate, PolicyGateState, PolicyReceipt, PolicyReview, PolicySeal, PolicySealReason, PolicySelection,
    installed_manifest,
};
use crate::StorageError;

// Published0064 bounds; do not inherit future per-epoch or aggregate limits.
const MAX_POLICY_REQUEST_BYTES: usize = 65536;
const MAX_POLICY_HISTORY: i64 = 4096;
const MAX_POLICY_RECEIPTS: i64 = 4096;
const MAX_POLICY_HEADS: i64 = 1024;
const CATALOG: &str = "empty-v1";
const DESCRIPTOR_V1: &[u8] = b"catalog=empty-v1;execution=unavailable;grants=none;schema=1;limits=unavailable";
type ManifestKinds = BTreeMap<(String, String, String), PackageKind>;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanonicalCommand {
    encoding_version: u32,
    command: PolicyCommand,
}

// Preserve the legacy numeric code map even if the runtime adds seal reasons.
fn decode_seal_reason(value: i64) -> Result<PolicySealReason, StorageError> {
    match value {
        0 => Ok(PolicySealReason::HistoryCapacity),
        1 => Ok(PolicySealReason::ReceiptCapacity),
        2 => Ok(PolicySealReason::ReviewHeadCapacity),
        3 => Ok(PolicySealReason::SelectionHeadCapacity),
        4 => Ok(PolicySealReason::RevisionExhausted),
        _ => Err(invalid()),
    }
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
    hash.update(DESCRIPTOR_V1);
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
            reason: decode_seal_reason(row.try_get("seal_reason")?)?,
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
