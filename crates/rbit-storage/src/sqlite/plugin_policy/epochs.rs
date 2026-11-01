//! Bounded passive policy epochs. No execution, pruning or external I/O.

use super::{
    OperationId, PolicyGate, PolicyGateState, PolicySeal, PolicySealReason, Pool, authorize, command, hex, invalid, load_gate,
    validate_current_snapshot,
};
use crate::StorageError;
use rbit_core::{AccountId, Timestamp};
use rbit_plugin_api::{HostPolicyFingerprint, MAX_POLICY_RENEWAL_REQUEST_BYTES, PackagePolicyRenewalRequest, Sha256Digest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, sqlite::SqliteRow};

pub(super) const MAX_EPOCHS: i64 = 3;
pub(super) const MAX_RENEWALS: i64 = 2;
const LEGACY_DESCRIPTOR: &[u8] = b"catalog=empty-v1;execution=unavailable;grants=none;schema=1;limits=unavailable";
const CURRENT_DESCRIPTOR: &[u8] = b"catalog=empty-v1;execution=unavailable;grants=none;schema=1;history=4096;receipts=4096;review_heads=1024;selection_heads=1024;epochs=3;renewals=2";

/// Historical evidence only. Destination need not remain current or Open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRenewalReceipt {
    pub id: OperationId,
    pub actor: Option<AccountId>,
    pub request: PackagePolicyRenewalRequest,
    pub source_epoch: Sha256Digest,
    pub source_fingerprint: HostPolicyFingerprint,
    pub destination_epoch: Sha256Digest,
    pub destination_fingerprint: HostPolicyFingerprint,
    pub created_at: Timestamp,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyRenewalOutcome {
    Applied { receipt: PolicyRenewalReceipt, gate: PolicyGate },
    HistoricalReceipt { receipt: PolicyRenewalReceipt, gate: PolicyGate },
    RetentionFull { gate: PolicyGate },
}

/// Complete bounded epoch row, with raw retired shape checked before construction.
/// Cross-row chain/receipt checks belong to `renewal_operation` and full snapshot validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EpochRecord {
    pub(super) epoch: Sha256Digest,
    pub(super) ordinal: i64,
    pub(super) fingerprint: HostPolicyFingerprint,
    pub(super) descriptor_version: i64,
    pub(super) predecessor: Option<Sha256Digest>,
    pub(super) created_by_renewal: Option<OperationId>,
    pub(super) prepared_by: Option<AccountId>,
    pub(super) prepared_at: Timestamp,
    pub(super) retired: bool,
    pub(super) retirement_operation: Option<OperationId>,
    pub(super) retired_by: Option<AccountId>,
    pub(super) retired_at: Option<Timestamp>,
    pub(super) seal: Option<PolicySeal>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RenewalCommand {
    Renew,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanonicalRenewal {
    encoding_version: u32,
    command: RenewalCommand,
    request: PackagePolicyRenewalRequest,
}
fn canonical_renewal(request: &PackagePolicyRenewalRequest) -> Result<String, StorageError> {
    request.validate().map_err(|_| StorageError::Conflict("invalid package policy renewal request"))?;
    let wire = CanonicalRenewal { encoding_version: 1, command: RenewalCommand::Renew, request: request.clone() };
    let result = serde_json::to_string(&wire).map_err(|_| invalid())?;
    if result.len() > MAX_POLICY_RENEWAL_REQUEST_BYTES {
        return Err(StorageError::Conflict("invalid package policy renewal request"));
    }
    Ok(result)
}
fn stored_request(raw: &str) -> Result<PackagePolicyRenewalRequest, StorageError> {
    if raw.is_empty() || raw.len() > MAX_POLICY_RENEWAL_REQUEST_BYTES {
        return Err(invalid());
    }
    let wire: CanonicalRenewal = serde_json::from_str(raw).map_err(|_| invalid())?;
    if wire.encoding_version != 1 || canonical_renewal(&wire.request).map_err(|_| invalid())? != raw {
        return Err(invalid());
    }
    Ok(wire.request)
}

/// Descriptor1 is frozen0064 identity. Descriptor2 binds actual bounded retention.
/// Never call `load_gate` here: its v2 implementation depends on this pure function.
pub(super) fn fingerprint(epoch: &Sha256Digest, descriptor_version: i64) -> Result<HostPolicyFingerprint, StorageError> {
    let (domain, descriptor): (&[u8], &[u8]) = match descriptor_version {
        1 => (b"rbit-package-policy-v1\0", LEGACY_DESCRIPTOR),
        2 => (b"rbit-package-policy-v2\0", CURRENT_DESCRIPTOR),
        _ => return Err(invalid()),
    };
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(epoch.as_str().as_bytes());
    hash.update(b"\0");
    hash.update(descriptor);
    Ok(HostPolicyFingerprint::new(hex(&hash.finalize()).parse().map_err(|_| invalid())?))
}
fn fresh_epoch() -> Result<Sha256Digest, StorageError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| StorageError::Sqlite(sqlx::Error::Io(std::io::Error::other("package policy epoch entropy unavailable"))))?;
    hex(&bytes).parse().map_err(|_| invalid())
}

pub(super) async fn bootstrap(pool: &Pool, actor: AccountId, now: Timestamp) -> Result<PolicyGate, StorageError> {
    let epoch = fresh_epoch()?;
    let fingerprint = fingerprint(&epoch, 2)?;
    let mut tx = pool.begin_immediate().await?;
    authorize(&mut *tx, actor).await?;
    let gate = load_gate(&mut *tx).await?;
    if gate.state != PolicyGateState::Uninitialized {
        return Err(StorageError::Conflict("policy already prepared or sealed"));
    }
    let empty: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM plugin_policy_epochs) AND NOT EXISTS(SELECT 1 FROM plugin_policy_renewals) AND NOT EXISTS(SELECT 1 FROM plugin_review_history) AND NOT EXISTS(SELECT 1 FROM plugin_review_heads) AND NOT EXISTS(SELECT 1 FROM plugin_selection_heads) AND NOT EXISTS(SELECT 1 FROM plugin_policy_receipts)")
        .fetch_one(&mut *tx).await?;
    if !empty {
        return Err(invalid());
    }
    sqlx::query("INSERT INTO plugin_policy_epochs(epoch,ordinal,fingerprint,descriptor_version,prepared_by,prepared_at,retired) VALUES(?,0,?,2,?,?,0)")
        .bind(epoch.as_str()).bind(fingerprint.as_str()).bind(actor.0).bind(now.0).execute(&mut *tx).await?;
    let updated =
        sqlx::query("UPDATE plugin_policy_gate SET state=1,epoch=?,fingerprint=?,prepared_by=?,prepared_at=? WHERE id=1 AND state=0")
            .bind(epoch.as_str())
            .bind(fingerprint.as_str())
            .bind(actor.0)
            .bind(now.0)
            .execute(&mut *tx)
            .await?;
    if updated.rows_affected() != 1 {
        return Err(invalid());
    }
    let gate = load_gate(&mut *tx).await?;
    tx.commit().await?;
    Ok(gate)
}

/// A mutation owner must retain exclusive maintenance admission through this future.
pub(super) async fn renew(
    pool: &Pool,
    actor: AccountId,
    id: &OperationId,
    request: &PackagePolicyRenewalRequest,
    now: Timestamp,
) -> Result<PolicyRenewalOutcome, StorageError> {
    let canonical = canonical_renewal(request)?;
    if let Some(outcome) = historical_preflight(pool, actor, id, &canonical).await? {
        return Ok(outcome);
    }
    let epoch = fresh_epoch()?; // Outside FULL transaction, never chosen by caller.
    let proposed_fingerprint = fingerprint(&epoch, 2)?;
    let mut tx = pool.begin_immediate().await?;
    authorize(&mut *tx, actor).await?;
    // Exact replay precedes current gate/seal/CAS: never rotate twice after loss.
    if let Some(receipt) = renewal_operation(&mut *tx, id).await? {
        validate_replay(&receipt, actor, &canonical)?;
        let gate = load_gate(&mut *tx).await?;
        tx.commit().await?;
        return Ok(PolicyRenewalOutcome::HistoricalReceipt { receipt, gate });
    }
    let used_ordinary: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_policy_receipts WHERE id=?)").bind(id.as_str()).fetch_one(&mut *tx).await?;
    if used_ordinary || reserved_operation(&mut *tx, id).await? {
        return Err(StorageError::Conflict("policy operation identity"));
    }
    let gate = load_gate(&mut *tx).await?;
    if gate.state != PolicyGateState::Sealed
        || gate.fingerprint.as_ref() != Some(&request.expected_host_policy)
        || gate.seal.as_ref().map(|s| s.operation.as_str()) != Some(request.expected_seal_operation.as_str())
    {
        return Err(StorageError::Conflict("policy renewal gate expectation"));
    }
    validate_current_snapshot(&mut *tx).await?;
    let source = checked_epoch(&mut *tx, gate.epoch.as_ref().ok_or_else(invalid)?).await?;
    let epochs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_epochs").fetch_one(&mut *tx).await?;
    let renewals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_renewals").fetch_one(&mut *tx).await?;
    if epochs >= MAX_EPOCHS || renewals >= MAX_RENEWALS || source.ordinal.checked_add(1).is_none_or(|n| n >= MAX_EPOCHS) {
        tx.commit().await?;
        return Ok(PolicyRenewalOutcome::RetentionFull { gate });
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_policy_epochs WHERE epoch=?)")
        .bind(epoch.as_str())
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        return Err(StorageError::Conflict("policy epoch collision"));
    }
    let transition = RenewalTransition {
        actor,
        id,
        canonical: &canonical,
        gate: &gate,
        source: &source,
        epoch: &epoch,
        proposed_fingerprint: &proposed_fingerprint,
        now,
    };
    retire_and_publish(&mut *tx, transition).await?;
    let receipt = renewal_operation(&mut *tx, id).await?.ok_or_else(invalid)?;
    let gate = load_gate(&mut *tx).await?;
    tx.commit().await?;
    Ok(PolicyRenewalOutcome::Applied { receipt, gate })
}

// Do not require fresh randomness to acknowledge an already durable receipt.
// The subsequent write transaction repeats auth/replay after this unchanged one.
async fn historical_preflight(
    pool: &Pool,
    actor: AccountId,
    id: &OperationId,
    canonical: &str,
) -> Result<Option<PolicyRenewalOutcome>, StorageError> {
    let mut tx = pool.begin_immediate().await?;
    authorize(&mut *tx, actor).await?;
    let result = if let Some(receipt) = renewal_operation(&mut *tx, id).await? {
        validate_replay(&receipt, actor, canonical)?;
        let gate = load_gate(&mut *tx).await?;
        Some(PolicyRenewalOutcome::HistoricalReceipt { receipt, gate })
    } else {
        None
    };
    tx.commit().await?;
    Ok(result)
}
fn validate_replay(receipt: &PolicyRenewalReceipt, actor: AccountId, canonical: &str) -> Result<(), StorageError> {
    if receipt.actor != Some(actor) || canonical_renewal(&receipt.request)? != canonical {
        return Err(StorageError::Conflict("policy renewal operation identity"));
    }
    Ok(())
}

struct RenewalTransition<'a> {
    actor: AccountId,
    id: &'a OperationId,
    canonical: &'a str,
    gate: &'a PolicyGate,
    source: &'a EpochRecord,
    epoch: &'a Sha256Digest,
    proposed_fingerprint: &'a HostPolicyFingerprint,
    now: Timestamp,
}
async fn retire_and_publish<C>(connection: &mut C, transition: RenewalTransition<'_>) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let RenewalTransition { actor, id, canonical, gate, source, epoch, proposed_fingerprint, now } = transition;
    let seal = gate.seal.as_ref().ok_or_else(invalid)?;
    let retired = sqlx::query("UPDATE plugin_policy_epochs SET retired=1,retirement_operation=?,retired_by=?,retired_at=?,seal_reason=?,seal_actor=?,seal_operation=?,seal_request=?,sealed_at=? WHERE epoch=? AND retired=0")
        .bind(id.as_str()).bind(actor.0).bind(now.0).bind(seal.reason.code()).bind(seal.actor.map(|a| a.0))
        .bind(seal.operation.as_str()).bind(&seal.request).bind(seal.at.0).bind(source.epoch.as_str()).execute(&mut *connection).await?;
    if retired.rows_affected() != 1 {
        return Err(invalid());
    }
    let ordinal = source.ordinal.checked_add(1).filter(|n| *n < MAX_EPOCHS).ok_or_else(invalid)?;
    sqlx::query("INSERT INTO plugin_policy_epochs(epoch,ordinal,fingerprint,descriptor_version,predecessor,created_by_renewal,prepared_by,prepared_at,retired) VALUES(?,?,?,2,?,?,?,?,0)")
        .bind(epoch.as_str()).bind(ordinal).bind(proposed_fingerprint.as_str()).bind(source.epoch.as_str())
        .bind(id.as_str()).bind(actor.0).bind(now.0).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO plugin_policy_renewals(id,request,actor,source_epoch,source_fingerprint,destination_epoch,destination_fingerprint,expected_seal_operation,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(id.as_str()).bind(canonical).bind(actor.0).bind(source.epoch.as_str()).bind(source.fingerprint.as_str())
        .bind(epoch.as_str()).bind(proposed_fingerprint.as_str()).bind(seal.operation.as_str()).bind(now.0).execute(&mut *connection).await?;
    let published = sqlx::query("UPDATE plugin_policy_gate SET state=1,epoch=?,fingerprint=?,prepared_by=?,prepared_at=?,seal_reason=NULL,seal_actor=NULL,seal_operation=NULL,seal_request=NULL,sealed_at=NULL WHERE id=1 AND state=2 AND epoch=? AND fingerprint=? AND seal_operation=?")
        .bind(epoch.as_str()).bind(proposed_fingerprint.as_str()).bind(actor.0).bind(now.0).bind(source.epoch.as_str())
        .bind(source.fingerprint.as_str()).bind(seal.operation.as_str()).execute(&mut *connection).await?;
    if published.rows_affected() != 1 {
        return Err(invalid());
    }
    Ok(())
}

/// Reservations without ordinary receipts still own their durable recovery key.
pub(super) async fn reserved_operation<C>(connection: &mut C, id: &OperationId) -> Result<bool, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_policy_renewals WHERE id=? UNION ALL SELECT 1 FROM plugin_policy_gate WHERE seal_operation=? UNION ALL SELECT 1 FROM plugin_policy_epochs WHERE seal_operation=?)")
        .bind(id.as_str()).bind(id.as_str()).bind(id.as_str()).fetch_one(connection).await?)
}

/// Local decode only: no `load_gate` or recursive receipt/chain traversal.
pub(super) async fn checked_epoch<C>(connection: &mut C, requested: &Sha256Digest) -> Result<EpochRecord, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    // Size-only CASE runs before any stored TEXT is materialized into Rust String.
    let bounded: Option<bool> = sqlx::query_scalar("SELECT CASE WHEN length(CAST(epoch AS BLOB))=64 AND length(CAST(fingerprint AS BLOB))=64 AND (predecessor IS NULL OR length(CAST(predecessor AS BLOB))=64) AND (created_by_renewal IS NULL OR length(CAST(created_by_renewal AS BLOB)) BETWEEN 1 AND 64) AND (retirement_operation IS NULL OR length(CAST(retirement_operation AS BLOB)) BETWEEN 1 AND 64) AND (seal_operation IS NULL OR length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64) AND (seal_request IS NULL OR length(CAST(seal_request AS BLOB)) BETWEEN 1 AND 65536) THEN 1 ELSE 0 END FROM plugin_policy_epochs WHERE epoch=?")
        .bind(requested.as_str()).fetch_optional(&mut *connection).await?;
    if bounded != Some(true) {
        return Err(invalid());
    }
    let row = sqlx::query("SELECT * FROM plugin_policy_epochs WHERE epoch=?").bind(requested.as_str()).fetch_one(connection).await?;
    let epoch = decode_epoch(&row)?;
    if &epoch.epoch != requested {
        return Err(invalid());
    }
    Ok(epoch)
}
fn optional_digest(row: &SqliteRow, column: &str) -> Result<Option<Sha256Digest>, StorageError> {
    row.try_get::<Option<String>, _>(column)?.map(|s| s.parse().map_err(|_| invalid())).transpose()
}
fn optional_operation(row: &SqliteRow, column: &str) -> Result<Option<OperationId>, StorageError> {
    row.try_get::<Option<String>, _>(column)?.map(|s| OperationId::new(&s).map_err(|_| invalid())).transpose()
}
fn decode_epoch(row: &SqliteRow) -> Result<EpochRecord, StorageError> {
    let epoch: Sha256Digest = row.try_get::<String, _>("epoch")?.parse().map_err(|_| invalid())?;
    let descriptor_version: i64 = row.try_get("descriptor_version")?;
    let declared_fingerprint = HostPolicyFingerprint::new(row.try_get::<String, _>("fingerprint")?.parse().map_err(|_| invalid())?);
    if declared_fingerprint != fingerprint(&epoch, descriptor_version)? {
        return Err(invalid());
    }
    let ordinal: i64 = row.try_get("ordinal")?;
    let predecessor = optional_digest(row, "predecessor")?;
    let created_by_renewal = optional_operation(row, "created_by_renewal")?;
    if !(0..MAX_EPOCHS).contains(&ordinal)
        || descriptor_version == 1 && ordinal != 0
        || ordinal == 0 && (predecessor.is_some() || created_by_renewal.is_some())
        || ordinal > 0 && (predecessor.is_none() || created_by_renewal.is_none())
        || predecessor.as_ref() == Some(&epoch)
    {
        return Err(invalid());
    }
    let retired = match row.try_get::<i64, _>("retired")? {
        0 => false,
        1 => true,
        _ => return Err(invalid()),
    };
    let retirement_operation = optional_operation(row, "retirement_operation")?;
    let retired_by = row.try_get::<Option<i64>, _>("retired_by")?.map(AccountId);
    let retired_at = row.try_get::<Option<i64>, _>("retired_at")?.map(Timestamp);
    let seal = epoch_seal(row, retired, &declared_fingerprint)?;
    if retired && (retirement_operation.is_none() || retired_at.is_none())
        || !retired && (retirement_operation.is_some() || retired_at.is_some() || retired_by.is_some())
        || retirement_operation.is_some() && retirement_operation == created_by_renewal
        || seal.as_ref().is_some_and(|s| retirement_operation.as_ref() == Some(&s.operation))
    {
        return Err(invalid());
    }
    Ok(EpochRecord {
        epoch,
        ordinal,
        fingerprint: declared_fingerprint,
        descriptor_version,
        predecessor,
        created_by_renewal,
        prepared_by: row.try_get::<Option<i64>, _>("prepared_by")?.map(AccountId),
        prepared_at: Timestamp(row.try_get("prepared_at")?),
        retired,
        retirement_operation,
        retired_by,
        retired_at,
        seal,
    })
}
fn epoch_seal(row: &SqliteRow, retired: bool, fingerprint: &HostPolicyFingerprint) -> Result<Option<PolicySeal>, StorageError> {
    let reason: Option<i64> = row.try_get("seal_reason")?;
    let actor: Option<i64> = row.try_get("seal_actor")?;
    let operation = optional_operation(row, "seal_operation")?;
    let request: Option<String> = row.try_get("seal_request")?;
    let at: Option<i64> = row.try_get("sealed_at")?;
    if !retired {
        if reason.is_some() || actor.is_some() || operation.is_some() || request.is_some() || at.is_some() {
            return Err(invalid());
        }
        return Ok(None);
    }
    let request = request.ok_or_else(invalid)?;
    if command(&request)?.fingerprint() != fingerprint {
        return Err(invalid());
    }
    Ok(Some(PolicySeal {
        reason: PolicySealReason::decode(reason.ok_or_else(invalid)?)?,
        actor: actor.map(AccountId),
        operation: operation.ok_or_else(invalid)?,
        request,
        at: Timestamp(at.ok_or_else(invalid)?),
    }))
}

/// Receipt lookup never claims its destination is current or active. No recursion
/// through `load_gate` or a full snapshot; both epoch rows are decoded locally first.
pub(super) async fn renewal_operation<C>(connection: &mut C, id: &OperationId) -> Result<Option<PolicyRenewalReceipt>, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let bounded: Option<bool> = sqlx::query_scalar("SELECT CASE WHEN length(CAST(id AS BLOB)) BETWEEN 1 AND 64 AND length(CAST(request AS BLOB)) BETWEEN 1 AND 1024 AND length(CAST(source_epoch AS BLOB))=64 AND length(CAST(source_fingerprint AS BLOB))=64 AND length(CAST(destination_epoch AS BLOB))=64 AND length(CAST(destination_fingerprint AS BLOB))=64 AND length(CAST(expected_seal_operation AS BLOB)) BETWEEN 1 AND 64 THEN 1 ELSE 0 END FROM plugin_policy_renewals WHERE id=?")
        .bind(id.as_str()).fetch_optional(&mut *connection).await?;
    match bounded {
        None => return Ok(None),
        Some(false) => return Err(invalid()),
        Some(true) => {}
    }
    let row = sqlx::query("SELECT * FROM plugin_policy_renewals WHERE id=?").bind(id.as_str()).fetch_one(&mut *connection).await?;
    let receipt = decode_renewal(&row)?;
    if &receipt.id != id {
        return Err(invalid());
    }
    let source = checked_epoch(&mut *connection, &receipt.source_epoch).await?;
    let destination = checked_epoch(connection, &receipt.destination_epoch).await?;
    validate_links(&receipt, &source, &destination)?;
    Ok(Some(receipt))
}
fn decode_renewal(row: &SqliteRow) -> Result<PolicyRenewalReceipt, StorageError> {
    let request = stored_request(&row.try_get::<String, _>("request")?)?;
    let source_fingerprint = HostPolicyFingerprint::new(row.try_get::<String, _>("source_fingerprint")?.parse().map_err(|_| invalid())?);
    if source_fingerprint != request.expected_host_policy
        || request.expected_seal_operation != row.try_get::<String, _>("expected_seal_operation")?
    {
        return Err(invalid());
    }
    let id = OperationId::new(&row.try_get::<String, _>("id")?).map_err(|_| invalid())?;
    if id.as_str() == request.expected_seal_operation {
        return Err(invalid());
    }
    Ok(PolicyRenewalReceipt {
        id,
        actor: row.try_get::<Option<i64>, _>("actor")?.map(AccountId),
        request,
        source_epoch: row.try_get::<String, _>("source_epoch")?.parse().map_err(|_| invalid())?,
        source_fingerprint,
        destination_epoch: row.try_get::<String, _>("destination_epoch")?.parse().map_err(|_| invalid())?,
        destination_fingerprint: HostPolicyFingerprint::new(
            row.try_get::<String, _>("destination_fingerprint")?.parse().map_err(|_| invalid())?,
        ),
        created_at: Timestamp(row.try_get("created_at")?),
    })
}
fn validate_links(receipt: &PolicyRenewalReceipt, source: &EpochRecord, destination: &EpochRecord) -> Result<(), StorageError> {
    if !source.retired
        || source.fingerprint != receipt.source_fingerprint
        || destination.fingerprint != receipt.destination_fingerprint
        || source.ordinal.checked_add(1) != Some(destination.ordinal)
        || destination.predecessor.as_ref() != Some(&source.epoch)
        || destination.created_by_renewal.as_ref() != Some(&receipt.id)
        || source.retirement_operation.as_ref() != Some(&receipt.id)
        || source.retired_by != receipt.actor
        || source.retired_at != Some(receipt.created_at)
        || destination.prepared_by != receipt.actor
        || destination.prepared_at != receipt.created_at
        || source.seal.as_ref().map(|s| s.operation.as_str()) != Some(receipt.request.expected_seal_operation.as_str())
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_domains_preserve_legacy_and_bind_finite_retention() {
        let epoch: Sha256Digest = "e".repeat(64).parse().unwrap();
        assert_eq!(fingerprint(&epoch, 1).unwrap().as_str(), "dac397089dbd3401ee998fde15507510ac5dd3fddf60c2e1ee89e992646d08d2");
        assert_eq!(fingerprint(&epoch, 2).unwrap().as_str(), "e0141cf2c25612206f25e3683771e02e6761e20d9b5976f20aea194acc093947");
        assert_ne!(fingerprint(&epoch, 1).unwrap(), fingerprint(&epoch, 2).unwrap());
        assert_ne!(fingerprint(&epoch, 2).unwrap(), fingerprint(&"f".repeat(64).parse().unwrap(), 2).unwrap());
        assert!(fingerprint(&epoch, 3).is_err());
    }
    #[test]
    fn renewal_storage_wire_is_exact_versioned_and_has_no_implicit_command() {
        let request =
            PackagePolicyRenewalRequest::new(HostPolicyFingerprint::new("b".repeat(64).parse().unwrap()), "observed-seal").unwrap();
        let wire = canonical_renewal(&request).unwrap();
        assert_eq!(
            wire,
            format!(
                r#"{{"encoding_version":1,"command":"renew","request":{{"schema_version":1,"expected_host_policy":"{}","expected_seal_operation":"observed-seal"}}}}"#,
                "b".repeat(64)
            )
        );
        assert_eq!(stored_request(&wire).unwrap(), request);
        for changed in [
            wire.replace("\"renew\"", "\"approve\""),
            wire.replace("\"encoding_version\":1", "\"encoding_version\":2"),
            format!(" {wire}"),
            wire.replace("\"command\":\"renew\"", "\"command\":\"renew\",\"command\":\"renew\""),
        ] {
            assert!(stored_request(&changed).is_err());
        }
        assert!(stored_request(&" ".repeat(MAX_POLICY_RENEWAL_REQUEST_BYTES + 1)).is_err());
        let mut invalid = request;
        invalid.schema_version = 2;
        assert!(canonical_renewal(&invalid).is_err());
    }
}
