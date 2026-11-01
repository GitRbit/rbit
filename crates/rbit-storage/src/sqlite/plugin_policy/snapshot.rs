//! Bounded read-only validation of legacy0064 and retained-epoch0065 policy graphs.
//! Metadata validation issues no execution or host-call admission authority.

use std::collections::BTreeMap;

use rbit_plugin_api::{InstalledPackageRef, PackageKind, PackageReviewIntent, PluginId, PolicyRevision, Sha256Digest};
use sqlx::{Row, SqliteConnection};

use super::{
    MAX_POLICY_HEADS, MAX_POLICY_HISTORY, MAX_POLICY_RECEIPTS, PolicyCommand, PolicyGate, PolicyGateState, PolicyReceipt, PolicySeal,
    count,
    epochs::{self, EpochRecord, MAX_EPOCHS, MAX_RENEWALS},
    expected_matches, installed_manifest, integer, invalid, legacy_snapshot, load_gate, load_selection, receipt, review,
};
use crate::StorageError;

type ManifestKinds = BTreeMap<(String, String, String), PackageKind>;
const MAX_MANIFEST_KINDS: usize = 3072;

#[derive(Default)]
struct Totals {
    epochs: i64,
    renewals: i64,
    ordinary: i64,
}

/// The caller supplies an immutable backup connection. Absence of all policy
/// tables is a caller-owned pre0064 decision, never a valid current graph here.
pub async fn validate_snapshot(connection: &mut SqliteConnection) -> Result<(), StorageError> {
    let legacy: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name IN ('plugin_policy_gate','plugin_review_history','plugin_review_heads','plugin_selection_heads','plugin_policy_receipts')")
        .fetch_one(&mut *connection).await?;
    let epochs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name IN ('plugin_policy_epochs','plugin_policy_renewals')",
    )
    .fetch_one(&mut *connection)
    .await?;
    if legacy != 5 {
        return Err(invalid());
    }
    match epochs {
        0 => {
            let migrations: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='_sqlx_migrations')")
                    .fetch_one(&mut *connection)
                    .await?;
            if migrations {
                let current: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version>=65)")
                    .fetch_one(&mut *connection)
                    .await?;
                if current {
                    return Err(invalid());
                }
            }
            legacy_snapshot::validate_snapshot(connection).await
        }
        2 => validate_current_snapshot(connection).await,
        _ => Err(invalid()),
    }
}

/// Also usable inside the instrumented FULL transaction before a renewal. The
/// caller owns its consistent snapshot and transaction; this function only reads.
pub(super) async fn validate_current_snapshot<C>(connection: &mut C) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let totals = preflight(connection).await?;
    check_foreign_keys(connection).await?;
    let gate = load_gate(connection).await?;
    if gate.state == PolicyGateState::Uninitialized {
        if totals.epochs != 0 || totals.renewals != 0 || totals.ordinary != 0 {
            return Err(invalid());
        }
        return Ok(());
    }
    let records = registry(connection, &gate, totals.epochs, totals.renewals).await?;
    check_global_keys(connection, &gate, &records).await?;
    let mut manifests = BTreeMap::new();
    for epoch in &records {
        validate_heads(connection, &epoch.epoch).await?;
        validate_history(connection, epoch, &mut manifests).await?;
        validate_receipts(connection, epoch, &mut manifests).await?;
        validate_selections(connection, &epoch.epoch).await?;
    }
    Ok(())
}

async fn preflight<C>(connection: &mut C) -> Result<Totals, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    // All counts and SQL byte/type predicates run before any stored String/row
    // can be materialized. CASE treats NULL/unknown predicates as malformed.
    let mut totals = Totals::default();
    for (table, limit, query) in &TABLE_CHECKS {
        let rows: i64 = sqlx::query_scalar(table_count(table)?).fetch_one(&mut *connection).await?;
        if rows > *limit || (*table == "plugin_policy_gate" && rows != 1) {
            return Err(invalid());
        }
        match *table {
            "plugin_policy_epochs" => totals.epochs = rows,
            "plugin_policy_renewals" => totals.renewals = rows,
            "plugin_policy_gate" => {}
            _ => totals.ordinary += rows,
        }
        let malformed: bool = sqlx::query_scalar(*query).fetch_one(&mut *connection).await?;
        if malformed {
            return Err(invalid());
        }
    }
    for (query, limit) in [
        ("SELECT EXISTS(SELECT 1 FROM plugin_review_history GROUP BY epoch HAVING COUNT(*)>?)", MAX_POLICY_HISTORY),
        ("SELECT EXISTS(SELECT 1 FROM plugin_policy_receipts GROUP BY epoch HAVING COUNT(*)>?)", MAX_POLICY_RECEIPTS),
        ("SELECT EXISTS(SELECT 1 FROM plugin_review_heads GROUP BY epoch HAVING COUNT(*)>?)", MAX_POLICY_HEADS),
        ("SELECT EXISTS(SELECT 1 FROM plugin_selection_heads GROUP BY epoch HAVING COUNT(*)>?)", MAX_POLICY_HEADS),
    ] {
        let excess: bool = sqlx::query_scalar(query).bind(limit).fetch_one(&mut *connection).await?;
        if excess {
            return Err(invalid());
        }
    }
    Ok(totals)
}
fn table_count(table: &str) -> Result<&'static str, StorageError> {
    match table {
        "plugin_policy_gate" => Ok("SELECT COUNT(*) FROM plugin_policy_gate"),
        "plugin_policy_epochs" => Ok("SELECT COUNT(*) FROM plugin_policy_epochs"),
        "plugin_policy_renewals" => Ok("SELECT COUNT(*) FROM plugin_policy_renewals"),
        "plugin_review_history" => Ok("SELECT COUNT(*) FROM plugin_review_history"),
        "plugin_policy_receipts" => Ok("SELECT COUNT(*) FROM plugin_policy_receipts"),
        "plugin_review_heads" => Ok("SELECT COUNT(*) FROM plugin_review_heads"),
        "plugin_selection_heads" => Ok("SELECT COUNT(*) FROM plugin_selection_heads"),
        _ => Err(invalid()),
    }
}
async fn check_foreign_keys<C>(connection: &mut C) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    for (table, _, _) in TABLE_CHECKS {
        let broken: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check(?))").bind(table).fetch_one(&mut *connection).await?;
        if broken {
            return Err(invalid());
        }
    }
    Ok(())
}

async fn registry<C>(connection: &mut C, gate: &PolicyGate, epochs: i64, renewals: i64) -> Result<Vec<EpochRecord>, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    if epochs == 0 || renewals != epochs - 1 {
        return Err(invalid());
    }
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT epoch FROM plugin_policy_epochs ORDER BY ordinal LIMIT 3").fetch_all(&mut *connection).await?;
    let mut records: Vec<EpochRecord> = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let id: Sha256Digest = id.parse().map_err(|_| invalid())?;
        let record = epochs::checked_epoch(connection, &id).await?;
        if record.ordinal != i64::try_from(index).map_err(|_| invalid())?
            || record.retired != (index + 1 < ids.len())
            || record.predecessor.as_ref() != records.last().map(|r| &r.epoch)
        {
            return Err(invalid());
        }
        if let Some(previous) = records.last() {
            let key = record.created_by_renewal.as_ref().ok_or_else(invalid)?;
            let renewal = epochs::renewal_operation(connection, key).await?.ok_or_else(invalid)?;
            if renewal.source_epoch != previous.epoch
                || renewal.destination_epoch != record.epoch
                || previous.retirement_operation.as_ref() != Some(key)
            {
                return Err(invalid());
            }
        }
        records.push(record);
    }
    let active = records.last().ok_or_else(invalid)?;
    if gate.epoch.as_ref() != Some(&active.epoch) || gate.fingerprint.as_ref() != Some(&active.fingerprint) {
        return Err(invalid());
    }
    Ok(records)
}

async fn check_global_keys<C>(connection: &mut C, gate: &PolicyGate, records: &[EpochRecord]) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let overlap: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_policy_receipts o JOIN plugin_policy_renewals r ON r.id=o.id)")
            .fetch_one(&mut *connection)
            .await?;
    if overlap {
        return Err(invalid());
    }
    let mut seals: BTreeMap<String, (&Sha256Digest, &PolicySeal)> = BTreeMap::new();
    for (epoch, seal) in
        records.iter().filter_map(|r| r.seal.as_ref().map(|s| (&r.epoch, s))).chain(gate.epoch.as_ref().zip(gate.seal.as_ref()))
    {
        if let Some((previous_epoch, previous_seal)) = seals.insert(seal.operation.as_str().to_owned(), (epoch, seal))
            && (previous_epoch != epoch || previous_seal != seal)
        {
            return Err(invalid());
        }
        let used: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM plugin_policy_receipts WHERE id=? UNION ALL SELECT 1 FROM plugin_policy_renewals WHERE id=?)",
        )
        .bind(seal.operation.as_str())
        .bind(seal.operation.as_str())
        .fetch_one(&mut *connection)
        .await?;
        if used {
            return Err(invalid());
        }
    }
    Ok(())
}

async fn validate_heads<C>(connection: &mut C, epoch: &Sha256Digest) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let broken: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_review_heads h WHERE h.epoch=? AND h.revision<>(SELECT MAX(r.revision) FROM plugin_review_history r WHERE r.epoch=h.epoch AND r.digest=h.digest)) OR EXISTS(SELECT 1 FROM (SELECT DISTINCT digest FROM plugin_review_history WHERE epoch=?) r LEFT JOIN plugin_review_heads h ON h.epoch=? AND h.digest=r.digest WHERE h.digest IS NULL)")
        .bind(epoch.as_str()).bind(epoch.as_str()).bind(epoch.as_str()).fetch_one(&mut *connection).await?;
    if broken {
        return Err(invalid());
    }
    // Explicit per-epoch counts also defend callers against a future accidental
    // widening of the aggregate preflight without corresponding local bounds.
    for (table, limit) in [
        ("plugin_review_history", MAX_POLICY_HISTORY),
        ("plugin_policy_receipts", MAX_POLICY_RECEIPTS),
        ("plugin_review_heads", MAX_POLICY_HEADS),
        ("plugin_selection_heads", MAX_POLICY_HEADS),
    ] {
        if count(connection, table, epoch).await? > limit {
            return Err(invalid());
        }
    }
    Ok(())
}

// Every known stored TEXT column is bounded, including optional metadata and
// new receipt.version/head.epoch columns; integers reject non-integer blobs.
const TABLE_CHECKS: [(&str, i64, &str); 7] = [
    (
        "plugin_policy_gate",
        1,
        "SELECT EXISTS(SELECT 1 FROM plugin_policy_gate WHERE CASE WHEN typeof(id)='integer' AND typeof(state)='integer' AND (epoch IS NULL OR (typeof(epoch)='text' AND length(CAST(epoch AS BLOB))=64)) AND (fingerprint IS NULL OR (typeof(fingerprint)='text' AND length(CAST(fingerprint AS BLOB))=64)) AND (prepared_by IS NULL OR typeof(prepared_by)='integer') AND (prepared_at IS NULL OR typeof(prepared_at)='integer') AND (seal_operation IS NULL OR (typeof(seal_operation)='text' AND length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64)) AND (seal_request IS NULL OR (typeof(seal_request)='text' AND length(CAST(seal_request AS BLOB)) BETWEEN 1 AND 65536)) AND (seal_reason IS NULL OR typeof(seal_reason)='integer') AND (seal_actor IS NULL OR typeof(seal_actor)='integer') AND (sealed_at IS NULL OR typeof(sealed_at)='integer') THEN 0 ELSE 1 END)",
    ),
    (
        "plugin_policy_epochs",
        MAX_EPOCHS,
        "SELECT EXISTS(SELECT 1 FROM plugin_policy_epochs WHERE CASE WHEN (typeof(epoch)='text' AND length(CAST(epoch AS BLOB))=64) AND (typeof(fingerprint)='text' AND length(CAST(fingerprint AS BLOB))=64) AND typeof(ordinal)='integer' AND typeof(descriptor_version)='integer' AND (predecessor IS NULL OR (typeof(predecessor)='text' AND length(CAST(predecessor AS BLOB))=64)) AND (created_by_renewal IS NULL OR (typeof(created_by_renewal)='text' AND length(CAST(created_by_renewal AS BLOB)) BETWEEN 1 AND 64)) AND (prepared_by IS NULL OR typeof(prepared_by)='integer') AND typeof(prepared_at)='integer' AND typeof(retired)='integer' AND (retirement_operation IS NULL OR (typeof(retirement_operation)='text' AND length(CAST(retirement_operation AS BLOB)) BETWEEN 1 AND 64)) AND (retired_by IS NULL OR typeof(retired_by)='integer') AND (retired_at IS NULL OR typeof(retired_at)='integer') AND (seal_operation IS NULL OR (typeof(seal_operation)='text' AND length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64)) AND (seal_request IS NULL OR (typeof(seal_request)='text' AND length(CAST(seal_request AS BLOB)) BETWEEN 1 AND 65536)) AND (seal_reason IS NULL OR typeof(seal_reason)='integer') AND (seal_actor IS NULL OR typeof(seal_actor)='integer') AND (sealed_at IS NULL OR typeof(sealed_at)='integer') THEN 0 ELSE 1 END)",
    ),
    (
        "plugin_policy_renewals",
        MAX_RENEWALS,
        "SELECT EXISTS(SELECT 1 FROM plugin_policy_renewals WHERE CASE WHEN (typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND 64) AND (typeof(request)='text' AND length(CAST(request AS BLOB)) BETWEEN 1 AND 1024) AND (actor IS NULL OR typeof(actor)='integer') AND (typeof(source_epoch)='text' AND length(CAST(source_epoch AS BLOB))=64) AND (typeof(source_fingerprint)='text' AND length(CAST(source_fingerprint AS BLOB))=64) AND (typeof(destination_epoch)='text' AND length(CAST(destination_epoch AS BLOB))=64) AND (typeof(destination_fingerprint)='text' AND length(CAST(destination_fingerprint AS BLOB))=64) AND (typeof(expected_seal_operation)='text' AND length(CAST(expected_seal_operation AS BLOB)) BETWEEN 1 AND 64) AND typeof(created_at)='integer' THEN 0 ELSE 1 END)",
    ),
    (
        "plugin_review_history",
        MAX_EPOCHS * MAX_POLICY_HISTORY,
        "SELECT EXISTS(SELECT 1 FROM plugin_review_history WHERE CASE WHEN (typeof(epoch)='text' AND length(CAST(epoch AS BLOB))=64) AND (typeof(digest)='text' AND length(CAST(digest AS BLOB))=64) AND typeof(revision)='integer' AND (typeof(plugin_id)='text' AND length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128) AND (typeof(version)='text' AND length(CAST(version AS BLOB)) BETWEEN 1 AND 128) AND typeof(intent)='integer' AND (typeof(request)='text' AND length(CAST(request AS BLOB)) BETWEEN 1 AND 65536) AND (typeof(catalog_version)='text' AND length(CAST(catalog_version AS BLOB))=8) AND (typeof(fingerprint)='text' AND length(CAST(fingerprint AS BLOB))=64) AND (actor IS NULL OR typeof(actor)='integer') AND typeof(created_at)='integer' THEN 0 ELSE 1 END)",
    ),
    (
        "plugin_policy_receipts",
        MAX_EPOCHS * MAX_POLICY_RECEIPTS,
        "SELECT EXISTS(SELECT 1 FROM plugin_policy_receipts WHERE CASE WHEN (typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND 64) AND typeof(kind)='integer' AND (typeof(request)='text' AND length(CAST(request AS BLOB)) BETWEEN 1 AND 65536) AND (actor IS NULL OR typeof(actor)='integer') AND (typeof(plugin_id)='text' AND length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128) AND (version IS NULL OR (typeof(version)='text' AND length(CAST(version AS BLOB)) BETWEEN 1 AND 128)) AND (digest IS NULL OR (typeof(digest)='text' AND length(CAST(digest AS BLOB))=64)) AND typeof(result_revision)='integer' AND (typeof(epoch)='text' AND length(CAST(epoch AS BLOB))=64) AND (typeof(fingerprint)='text' AND length(CAST(fingerprint AS BLOB))=64) AND typeof(created_at)='integer' THEN 0 ELSE 1 END)",
    ),
    (
        "plugin_review_heads",
        MAX_EPOCHS * MAX_POLICY_HEADS,
        "SELECT EXISTS(SELECT 1 FROM plugin_review_heads WHERE CASE WHEN (typeof(epoch)='text' AND length(CAST(epoch AS BLOB))=64) AND (typeof(digest)='text' AND length(CAST(digest AS BLOB))=64) AND typeof(revision)='integer' THEN 0 ELSE 1 END)",
    ),
    (
        "plugin_selection_heads",
        MAX_EPOCHS * MAX_POLICY_HEADS,
        "SELECT EXISTS(SELECT 1 FROM plugin_selection_heads WHERE CASE WHEN (typeof(epoch)='text' AND length(CAST(epoch AS BLOB))=64) AND (typeof(plugin_id)='text' AND length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128) AND typeof(revision)='integer' AND (version IS NULL OR (typeof(version)='text' AND length(CAST(version AS BLOB)) BETWEEN 1 AND 128)) AND (digest IS NULL OR (typeof(digest)='text' AND length(CAST(digest AS BLOB))=64)) AND (review_revision IS NULL OR typeof(review_revision)='integer') AND (actor IS NULL OR typeof(actor)='integer') AND typeof(updated_at)='integer' THEN 0 ELSE 1 END)",
    ),
];

async fn cached_kind<C>(connection: &mut C, package: &InstalledPackageRef, cache: &mut ManifestKinds) -> Result<PackageKind, StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let key = (package.plugin_id().to_string(), package.version().to_string(), package.archive_digest().to_string());
    if let Some(kind) = cache.get(&key) {
        return Ok(kind.clone());
    }
    if cache.len() >= MAX_MANIFEST_KINDS {
        return Err(invalid());
    }
    // The shared immutable installation resolver caps manifest TEXT at1MiB.
    // Retain only its kind; at most one complete manifest is parsed at a time.
    let kind = installed_manifest(connection, package).await?.kind;
    cache.insert(key, kind.clone());
    Ok(kind)
}

async fn validate_history<C>(connection: &mut C, epoch: &EpochRecord, manifests: &mut ManifestKinds) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let mut after_digest = String::new();
    let mut after_revision = 0_i64;
    loop {
        let rows=sqlx::query("SELECT * FROM plugin_review_history WHERE epoch=? AND (digest>? OR (digest=? AND revision>?)) ORDER BY digest,revision LIMIT 128")
            .bind(epoch.epoch.as_str()).bind(&after_digest).bind(&after_digest).bind(after_revision).fetch_all(&mut *connection).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let reviewed = review(&row)?;
            if row.try_get::<String, _>("epoch")? != epoch.epoch.as_str() || reviewed.request.expected_host_policy != epoch.fingerprint {
                return Err(invalid());
            }
            let kind = cached_kind(connection, &reviewed.request.package, manifests).await?;
            if !matches!(
                (reviewed.request.intent, kind),
                (PackageReviewIntent::Deny {}, _)
                    | (PackageReviewIntent::DataOnly {}, PackageKind::Data {})
                    | (PackageReviewIntent::NativeFullServiceUserTrust {}, PackageKind::Native { .. })
                    | (PackageReviewIntent::WasmSandboxRequired {}, PackageKind::Wasm { .. })
            ) {
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
                let exists: bool =
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_review_history WHERE epoch=? AND digest=? AND revision=?)")
                        .bind(epoch.epoch.as_str())
                        .bind(reviewed.request.package.archive_digest().as_str())
                        .bind(i64::try_from(previous).map_err(|_| invalid())?)
                        .fetch_one(&mut *connection)
                        .await?;
                if !exists {
                    return Err(invalid());
                }
            }
            let receipts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_receipts WHERE epoch=? AND kind=0 AND digest=? AND result_revision=? AND request=? AND actor IS ? AND created_at=?")
                .bind(epoch.epoch.as_str()).bind(reviewed.request.package.archive_digest().as_str()).bind(integer(reviewed.revision)?)
                .bind(row.try_get::<String,_>("request")?).bind(reviewed.actor.map(|a|a.0)).bind(reviewed.created_at.0).fetch_one(&mut *connection).await?;
            if receipts != 1 {
                return Err(invalid());
            }
            after_digest = row.try_get("digest")?;
            after_revision = row.try_get("revision")?;
        }
    }
    Ok(())
}

async fn validate_receipts<C>(connection: &mut C, epoch: &EpochRecord, manifests: &mut ManifestKinds) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let mut after = String::new();
    loop {
        let rows = sqlx::query("SELECT * FROM plugin_policy_receipts WHERE epoch=? AND id>? ORDER BY id LIMIT 128")
            .bind(epoch.epoch.as_str())
            .bind(&after)
            .fetch_all(&mut *connection)
            .await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let received = receipt(&row)?;
            if received.epoch != epoch.epoch || received.fingerprint != epoch.fingerprint {
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
                    let history = sqlx::query("SELECT * FROM plugin_review_history WHERE epoch=? AND digest=? AND revision=?")
                        .bind(epoch.epoch.as_str())
                        .bind(r.package.archive_digest().as_str())
                        .bind(integer(received.result_revision)?)
                        .fetch_optional(&mut *connection)
                        .await?
                        .ok_or_else(invalid)?;
                    let historical = review(&history)?;
                    if historical.request != *r || historical.actor != received.actor || historical.created_at != received.created_at {
                        return Err(invalid());
                    }
                }
                PolicyCommand::Select(r) => {
                    cached_kind(connection, &r.package, manifests).await?;
                    let history = sqlx::query("SELECT * FROM plugin_review_history WHERE epoch=? AND digest=? AND revision=?")
                        .bind(epoch.epoch.as_str())
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
                    check_selection_receipt(connection, &epoch.epoch, &received).await?;
                }
                PolicyCommand::Clear(_) => {
                    if previous == 0 {
                        return Err(invalid());
                    }
                    check_selection_receipt(connection, &epoch.epoch, &received).await?;
                }
            }
            after = received.id.as_str().to_owned();
        }
    }
    Ok(())
}

async fn check_selection_receipt<C>(connection: &mut C, epoch: &Sha256Digest, receipt: &PolicyReceipt) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let head = load_selection(connection, epoch, receipt.command.plugin()).await?.ok_or_else(invalid)?;
    if receipt.result_revision > head.revision {
        return Err(invalid());
    }
    if receipt.result_revision.get() > 1 {
        let previous = integer(receipt.result_revision)? - 1;
        let rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_policy_receipts WHERE epoch=? AND plugin_id=? AND kind IN (1,2) AND result_revision=?",
        )
        .bind(epoch.as_str())
        .bind(receipt.command.plugin().as_str())
        .bind(previous)
        .fetch_one(&mut *connection)
        .await?;
        if rows != 1 {
            return Err(invalid());
        }
    }
    Ok(())
}

async fn validate_selections<C>(connection: &mut C, epoch: &Sha256Digest) -> Result<(), StorageError>
where
    for<'c> &'c mut C: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    let mut after = String::new();
    loop {
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT plugin_id FROM plugin_selection_heads WHERE epoch=? AND plugin_id>? ORDER BY plugin_id LIMIT 128")
                .bind(epoch.as_str())
                .bind(&after)
                .fetch_all(&mut *connection)
                .await?;
        if ids.is_empty() {
            break;
        }
        for id in ids {
            let plugin: PluginId = id.parse().map_err(|_| invalid())?;
            let head = load_selection(connection, epoch, &plugin).await?.ok_or_else(invalid)?;
            let rows=sqlx::query("SELECT * FROM plugin_policy_receipts WHERE epoch=? AND plugin_id=? AND kind IN (1,2) AND result_revision=? AND actor IS ? AND created_at=? LIMIT 2")
                .bind(epoch.as_str()).bind(plugin.as_str()).bind(integer(head.revision)?)
                .bind(head.actor.map(|a|a.0)).bind(head.updated_at.0).fetch_all(&mut *connection).await?;
            if rows.len() != 1 {
                return Err(invalid());
            }
            match receipt(&rows[0])?.command {
                PolicyCommand::Select(r)
                    if head.package.as_ref() == Some(&r.package) && head.review_revision == Some(r.expected_review_revision) => {}
                PolicyCommand::Clear(_) if head.package.is_none() && head.review_revision.is_none() => {}
                _ => return Err(invalid()),
            }
            let rows: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM plugin_policy_receipts WHERE epoch=? AND plugin_id=? AND kind IN (1,2)")
                    .bind(epoch.as_str())
                    .bind(plugin.as_str())
                    .fetch_one(&mut *connection)
                    .await?;
            if rows != integer(head.revision)? {
                return Err(invalid());
            }
            after = id;
        }
    }
    Ok(())
}
