//! Fixed identity-layout shape/history projections; no descriptions or user text.
pub(super) const CORE_SHAPE: &str = "SELECT count(*) FROM pragma_table_list
 WHERE schema='main' AND type='table' AND strict=1
 AND name IN ('accounts','user_logins','repositories')";
pub(super) const CORE_KEYS: &str = "SELECT
 EXISTS(SELECT 1 FROM pragma_table_info('accounts') WHERE name='id' AND type='INTEGER' AND pk=1)
 AND EXISTS(SELECT 1 FROM pragma_table_info('user_logins') WHERE name='account_id' AND type='INTEGER' AND pk=1)
 AND EXISTS(SELECT 1 FROM pragma_foreign_key_list('user_logins')
            WHERE [table]='accounts' AND [from]='account_id' AND [to]='id' AND on_delete='CASCADE')";
pub(super) const MIGRATIONS: &str = "SELECT version,
 CASE WHEN success=1 THEN 1 ELSE 0 END AS success,
 CASE WHEN typeof(checksum)='blob' AND length(checksum)=48 THEN checksum END AS checksum
 FROM _sqlx_migrations ORDER BY version LIMIT ?";
pub(super) const PROTOCOL_SHAPE: &str = "SELECT
 CASE WHEN name='credential_protocol' AND type='table' THEN 1 ELSE 0 END
 FROM sqlite_schema WHERE name GLOB 'credential_protocol*' AND type IN ('table','view') LIMIT 2";
