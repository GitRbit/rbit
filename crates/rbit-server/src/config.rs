//! Layered configuration: built-in defaults → TOML file → saved administrator settings →
//! `RUSTIT_*` environment variables (legacy) → `RBIT_*` environment variables.

use std::path::Path;

use anyhow::Context;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use rbit_core::{Config, config::AuthConfig};

/// Nested keys use a double underscore: `RBIT_SERVER__PUBLIC_URL`.
const ENV_PREFIX: &str = "RBIT_";
/// The prefix from before the rename to rbit; `RBIT_*` wins where both are set.
const LEGACY_ENV_PREFIX: &str = "RUSTIT_";
const ENV_SEPARATOR: &str = "__";

pub async fn load(file: Option<&Path>) -> anyhow::Result<Config> {
    let mut figment = Figment::from(Serialized::defaults(Config::default()));
    if let Some(file) = file {
        anyhow::ensure!(file.exists(), "configuration file {} does not exist", file.display());
        figment = figment.merge(Toml::file(file));
    }
    // Locate the database using deployment sources only. Storage relocation is an offline operation.
    let locator: Config = figment
        .clone()
        .merge(
            Env::prefixed(LEGACY_ENV_PREFIX).split(ENV_SEPARATOR).filter(|key| key.as_str().to_ascii_lowercase().starts_with("storage.")),
        )
        .merge(Env::prefixed(ENV_PREFIX).split(ENV_SEPARATOR).filter(|key| key.as_str().to_ascii_lowercase().starts_with("storage.")))
        .extract()
        .context("invalid storage configuration")?;
    if locator.storage.sqlite_path().exists() && std::env::var_os("RBIT_IGNORE_ADMIN_CONFIG").is_none() {
        let database = rbit_storage::Database::open_with_key_file(&locator.storage.sqlite_path(), &locator.storage.secret_key_path())
            .await
            .context("cannot read admin configuration")?;
        let entries = database.instance_settings().load().await?;
        let key = database.sealing_key().clone();
        database.close().await;
        if let Some((_, document)) = entries.into_iter().find(|(key, _)| key == rbit_services::admin::configuration::CONFIGURATION_KEY) {
            let config = rbit_services::admin::configuration::open_stored(&document, &key)
                .map_err(anyhow::Error::msg)
                .context("invalid administrator configuration; set RBIT_IGNORE_ADMIN_CONFIG=1 to recover")?;
            let mut value = serde_json::to_value(config)?;
            value.as_object_mut().expect("config is an object").remove("storage");
            figment = figment.merge(Serialized::defaults(value));
        }
    }
    for prefix in [LEGACY_ENV_PREFIX, ENV_PREFIX] {
        // Milestone 3 (O): `AUTH__PROVIDERS__<N>__…` would replace the whole providers
        // array in figment; those are applied below instead.
        let env = Env::prefixed(prefix)
            .split(ENV_SEPARATOR)
            .ignore(&["config", "ignore_admin_config"])
            .filter(|key| !AuthConfig::is_provider_env_key(key.as_str()));
        figment = figment.merge(env);
    }
    let mut config: Config = figment.extract().context("invalid configuration")?;
    // Milestone 3: external sign-in (O): provider secrets from the environment, then
    // every provider checked, so a typo fails at start-up rather than at the first sign-in.
    config
        .auth
        .apply_provider_env(provider_env(std::env::vars()))
        .map_err(|problem| anyhow::anyhow!("invalid configuration: {problem}"))?;
    config.auth.validate().map_err(|problems| anyhow::anyhow!("invalid [auth] configuration: {problems}"))?;
    rbit_services::admin::configuration::validate_configuration(&config).map_err(anyhow::Error::msg)?;
    Ok(config)
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
        database.sealing_key().seal_secret_fields(&mut document).unwrap();
        assert!(!document.to_string().contains("smtp-value"));
        database.instance_settings().save(&[("deployment_config", &document.to_string())]).await.unwrap();
        let effective = load(Some(&file)).await.unwrap();
        assert_eq!(effective.mail.smtp_password, "smtp-value");
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
