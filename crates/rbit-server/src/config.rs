//! Layered configuration: built-in defaults → TOML file → saved administrator settings →
//! `RUSTIT_*` environment variables (legacy) → `RBIT_*` environment variables.

use std::path::Path;

use anyhow::Context;
use rbit_core::{Config, config::AuthConfig};

/// Nested keys use a double underscore: `RBIT_SERVER__PUBLIC_URL`.
const ENV_PREFIX: &str = "RBIT_";
/// The prefix from before the rename to rbit; `RBIT_*` wins where both are set.
const LEGACY_ENV_PREFIX: &str = "RUSTIT_";
const ENV_SEPARATOR: &str = "__";

mod sources;
pub(crate) use sources::ConfigSources;

pub async fn load(file: Option<&Path>) -> anyhow::Result<Config> {
    let sources = ConfigSources::parse(file)?;
    let locator = sources.deployment();
    // What an interrupted restore left is never opened; the command then refuses it.
    let readable = locator.storage.sqlite_path().exists() && !crate::backup::restore::interrupted(&locator.storage.data_dir);
    let saved = if readable && sources.reads_saved_configuration() {
        let database = rbit_storage::Database::open_with_key_file(&locator.storage.sqlite_path(), &locator.storage.secret_key_path())
            .await
            .context("cannot read admin configuration")?;
        let saved = sources.read_saved(&database).await?;
        database.close().await;
        saved
    } else {
        None
    };
    sources.finish(saved)
}

/// [`load`] for `rbit backup`, which must copy the data as it is: a database older than this
/// binary is not migrated (it may be the pre-upgrade backup an operator rolls back to), so
/// its saved administrator settings, which may predate this version, are not applied.
pub async fn load_unmigrated(file: Option<&Path>) -> anyhow::Result<Config> {
    let sources = ConfigSources::parse(file)?;
    let storage = &sources.deployment().storage;
    let database = storage.sqlite_path();
    if database.exists() && !crate::backup::restore::interrupted(&storage.data_dir) {
        let pending =
            rbit_storage::sqlite::snapshot::pending_migrations(&database).await.context("cannot read the database's migrations")?;
        if !pending.is_empty() {
            tracing::warn!(
                pending = pending.len(),
                "the database is older than this rbit; it is backed up as it is, without migrating or applying saved administrator settings"
            );
            return sources.finish(None);
        }
    }
    load(file).await
}

/// The configuration without the administrator settings saved in the database, for
/// `rbit restore`, which must not open the database of the directory it restores into.
pub fn load_deployment(file: Option<&Path>) -> anyhow::Result<Config> {
    ConfigSources::parse(file)?.finish(None)
}

/// `RBIT_AUTH__PROVIDERS__…` (and the legacy `RUSTIT_` spelling) without the prefix,
/// legacy first so `RBIT_*` wins.
fn provider_env(vars: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let vars: Vec<(String, String)> = vars.collect();
    let mut found = Vec::new();
    for prefix in [LEGACY_ENV_PREFIX, ENV_PREFIX] {
        for (key, value) in &vars {
            let rest = key.get(..prefix.len()).filter(|start| start.eq_ignore_ascii_case(prefix)).and_then(|_| key.get(prefix.len()..));
            if let Some(rest) = rest.filter(|rest| AuthConfig::is_provider_env_key(rest)) {
                found.push((rest.to_owned(), value.clone()));
            }
        }
    }
    found
}

#[cfg(test)]
mod password_policy_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn startup_applies_saved_configuration_and_reset_restores_file_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let mut base = Config::default();
        base.storage.data_dir = directory.path().join("data");
        std::fs::create_dir_all(&base.storage.data_dir).unwrap();
        let file = directory.path().join("rbit.toml");
        let path = base.storage.data_dir.to_string_lossy().replace('\\', "/");
        std::fs::write(&file, format!("[storage]\ndata_dir = {path:?}\n[repository]\ndefault_branch = \"trunk\"\n")).unwrap();
        let database = rbit_storage::Database::open(&base.storage.sqlite_path()).await.unwrap();
        base.repository.default_branch = "develop".into();
        base.server.site_name = "Acme Forge".into();
        // A stale storage location in saved data must never redirect startup to a new instance.
        base.storage.data_dir = directory.path().join("must-not-open");
        base.mail.smtp_password = "smtp-value".into();
        // Saved secrets are sealed with the key file in the data directory.
        let mut document = serde_json::to_value(&base).unwrap();
        document["security"].as_object_mut().unwrap().remove("max_password_jobs");
        database.sealing_key().seal_secret_fields(&mut document).unwrap();
        assert!(!document.to_string().contains("smtp-value"));
        database.instance_settings().save(&[("deployment_config", &document.to_string())]).await.unwrap();
        let effective = load(Some(&file)).await.unwrap();
        assert_eq!(effective.mail.smtp_password, "smtp-value");
        assert_eq!(effective.security.password_job_capacity().unwrap().get(), 2, "old saved documents retain the default");
        assert_eq!(effective.repository.default_branch, "develop");
        assert_eq!(effective.server.site_name, "Acme Forge");
        assert_eq!(effective.storage.data_dir, directory.path().join("data"));
        database.instance_settings().delete("deployment_config").await.unwrap();
        let reset = load(Some(&file)).await.unwrap();
        assert_eq!(reset.repository.default_branch, "trunk");
        database.close().await;
    }

    #[test]
    fn provider_variables_lose_their_prefix_and_rbit_wins() {
        let vars = [
            ("RBIT_AUTH__PROVIDERS__0__CLIENT_SECRET", "new"),
            ("RUSTIT_AUTH__PROVIDERS__0__CLIENT_SECRET", "old"),
            ("RBIT_SERVER__PUBLIC_URL", "x"),
            ("PATH", "y"),
        ]
        .map(|(key, value)| (key.to_owned(), value.to_owned()));
        let found = provider_env(vars.into_iter());
        assert_eq!(
            found,
            [
                ("AUTH__PROVIDERS__0__CLIENT_SECRET".to_owned(), "old".to_owned()),
                ("AUTH__PROVIDERS__0__CLIENT_SECRET".to_owned(), "new".to_owned())
            ]
        );
    }
}
