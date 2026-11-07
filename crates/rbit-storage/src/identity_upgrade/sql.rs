//! Fixed SQL checkpoints. The only writable connection belongs to the stopped owner.
#[path = "queries.rs"]
mod queries;
use super::model::{Checkpoint, IdentityUpgradeError as Error};
use sqlx::{Executor, Sqlite};
#[derive(sqlx::FromRow)]
struct StateRow {
    mode: i64,
    generation_shape: i64,
    pair_shape: i64,
    generation: Option<Vec<u8>>,
    pair: Option<Vec<u8>>,
    floor: i64,
    retired: i64,
    accounts: i64,
    final_floor: i64,
}
pub(super) enum State {
    Legacy,
    Preparing(Checkpoint),
    Prepared(Checkpoint),
}
pub(super) async fn state<'e, E: Executor<'e, Database = Sqlite>>(executor: E) -> Result<State, Error> {
    let rows: Vec<StateRow> = sqlx::query_as(queries::STATE).fetch_all(executor).await?;
    let [row] = rows.as_slice() else { return Err(Error::Invalid) };
    if row.mode == 1
        && row.generation_shape == 0
        && row.pair_shape == 0
        && row.generation.is_none()
        && row.pair.is_none()
        && row.floor == 0
        && row.retired == 0
        && row.accounts == 0
        && row.final_floor == 0
    {
        return Ok(State::Legacy);
    }
    if row.generation_shape != 1 || row.pair_shape != 1 {
        return Err(Error::Invalid);
    }
    let c = Checkpoint {
        generation: row.generation.as_deref().ok_or(Error::Invalid)?.try_into().map_err(|_| Error::Invalid)?,
        pair: row.pair.as_deref().ok_or(Error::Invalid)?.try_into().map_err(|_| Error::Invalid)?,
        floor: row.floor,
        retired: row.retired,
        accounts: row.accounts,
    };
    let c = Checkpoint::decode(&c.encode())?;
    match row.mode {
        2 if row.final_floor == 0 => Ok(State::Preparing(c)),
        3 if row.final_floor == c.final_floor()? => Ok(State::Prepared(c)),
        _ => Err(Error::Invalid),
    }
}
#[derive(sqlx::FromRow)]
pub(super) struct History {
    pub floor: i64,
    pub retired: i64,
    pub accounts: i64,
    pub valid: bool,
}
pub(super) async fn history<'e, E: Executor<'e, Database = Sqlite>>(executor: E) -> Result<History, Error> {
    let h: History = sqlx::query_as(queries::HISTORY).fetch_one(executor).await?;
    if !h.valid {
        return Err(Error::Invalid);
    }
    if h.floor == i64::MAX {
        return Err(Error::Exhausted);
    }
    Ok(h)
}
pub(super) async fn pair<'e, E: Executor<'e, Database = Sqlite>>(executor: E) -> Result<Option<[u8; 16]>, Error> {
    let rows: Vec<Option<Vec<u8>>> = sqlx::query_scalar(queries::PAIR).fetch_all(executor).await?;
    match rows.as_slice() {
        [] => Ok(None),
        [Some(bytes)] => Ok(Some(bytes.as_slice().try_into().map_err(|_| Error::Invalid)?)),
        _ => Err(Error::Invalid),
    }
}
pub(super) async fn write_preparing(tx: &mut sqlx::Transaction<'_, Sqlite>, c: Checkpoint) -> Result<(), Error> {
    match pair(&mut **tx).await? {
        None => {
            sqlx::query("INSERT INTO credential_session_pair(singleton,identity) VALUES(1,?)")
                .bind(c.pair.as_slice())
                .execute(&mut **tx)
                .await?;
        }
        Some(existing) if existing == c.pair => {}
        _ => return Err(Error::Mismatch),
    }
    let n = sqlx::query(queries::WRITE_PREPARING)
        .bind(c.generation.as_slice())
        .bind(c.pair.as_slice())
        .bind(c.floor)
        .bind(c.retired)
        .bind(c.accounts)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    if n != 1 {
        return Err(Error::Mismatch);
    }
    Ok(())
}
pub(super) async fn rebase(tx: &mut sqlx::Transaction<'_, Sqlite>, c: Checkpoint) -> Result<(), Error> {
    use crate::sqlite::credential_epochs::{allocate_in, raise_floor_in};
    use rbit_identity_models::credential::CredentialEpoch;
    let h = history(&mut **tx).await?;
    if h.floor > c.floor || h.retired != c.retired || h.accounts != c.accounts || pair(&mut **tx).await? != Some(c.pair) {
        return Err(Error::Mismatch);
    }
    raise_floor_in(tx, CredentialEpoch::new(c.floor).ok_or(Error::Invalid)?).await.map_err(allocation)?;
    let mut after = 0;
    let mut count = 0_i64;
    loop {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT account_id FROM user_logins WHERE account_id>? ORDER BY account_id LIMIT 256")
            .bind(after)
            .fetch_all(&mut **tx)
            .await?;
        if ids.is_empty() {
            break;
        }
        for id in ids {
            let epoch = allocate_in(tx).await.map_err(allocation)?;
            let changed = sqlx::query("UPDATE user_logins SET authentication_epoch=? WHERE account_id=?")
                .bind(epoch.get())
                .bind(id)
                .execute(&mut **tx)
                .await?
                .rows_affected();
            if changed != 1 {
                return Err(Error::Mismatch);
            }
            after = id;
            count += 1;
        }
    }
    if count != c.accounts {
        return Err(Error::Mismatch);
    }
    verify_accounts(tx, c).await?;
    sqlx::query(queries::SET_PREPARED).bind(c.final_floor()?).bind(c.generation.as_slice()).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn verify_accounts(tx: &mut sqlx::Transaction<'_, Sqlite>, c: Checkpoint) -> Result<(), Error> {
    let h = history(&mut **tx).await?;
    if h.retired != c.retired || h.accounts != c.accounts || h.floor > c.final_floor()? {
        return Err(Error::Mismatch);
    }
    let mut after = 0;
    let mut expected = c.floor;
    loop {
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT account_id,authentication_epoch FROM user_logins WHERE account_id>? ORDER BY account_id LIMIT 256")
                .bind(after)
                .fetch_all(&mut **tx)
                .await?;
        if rows.is_empty() {
            break;
        }
        for (id, epoch) in rows {
            expected = expected.checked_add(1).ok_or(Error::Exhausted)?;
            if epoch != expected {
                return Err(Error::Mismatch);
            }
            after = id;
        }
    }
    let high: i64 = sqlx::query_scalar("SELECT high_water FROM credential_epoch_allocator WHERE singleton=1").fetch_one(&mut **tx).await?;
    if expected != c.final_floor()? || high != expected || pair(&mut **tx).await? != Some(c.pair) {
        return Err(Error::Mismatch);
    }
    Ok(())
}
fn allocation(error: crate::sqlite::credential_epochs::EpochAllocationError) -> Error {
    use crate::sqlite::credential_epochs::EpochAllocationError;
    match error {
        EpochAllocationError::Storage(error) => error.into(),
        EpochAllocationError::InvalidState => Error::Invalid,
        EpochAllocationError::Exhausted => Error::Exhausted,
    }
}

pub(super) async fn within_limits(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    limits: super::model::IdentityUpgradeLimits,
) -> Result<(), Error> {
    let limit = i64::try_from(limits.max_entries.get()).unwrap_or(i64::MAX - 1).min(i64::MAX - 1);
    for query in
        ["SELECT count(*) FROM (SELECT 1 FROM user_logins LIMIT ?)", "SELECT count(*) FROM (SELECT 1 FROM credential_revocations LIMIT ?)"]
    {
        let count: i64 = sqlx::query_scalar(query).bind(limit + 1).fetch_one(&mut **tx).await?;
        if count > limit {
            return Err(Error::IncompleteScan);
        }
    }
    Ok(())
}
