//! Deployment parsing never opens, migrates, creates, reseals or reads a database/key.
//! Accepted-store startup can hydrate only AFTER its real paired owner is established.
use super::{AuthConfig, Config, Context, ENV_PREFIX, ENV_SEPARATOR, LEGACY_ENV_PREFIX, Path, provider_env};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use rbit_storage::{Database, sealing::SealingKey};

pub(crate) struct ConfigSources {
    figment: Figment,
    locator: Config,
    read_saved: bool,
}
pub(crate) struct SavedConfig {
    entries: Vec<(String, String)>,
    key: SealingKey,
}
impl std::fmt::Debug for ConfigSources {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ConfigSources").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for SavedConfig {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SavedConfig").finish_non_exhaustive()
    }
}
impl ConfigSources {
    pub(crate) fn parse(file: Option<&Path>) -> anyhow::Result<Self> {
        let mut figment = Figment::from(Serialized::defaults(Config::default()));
        if let Some(file) = file {
            anyhow::ensure!(file.exists(), "configuration file {} does not exist", file.display());
            figment = figment.merge(Toml::file(file));
        }
        // Storage relocation is controlled by deployment sources, never saved settings.
        let locator: Config = figment
            .clone()
            .merge(
                Env::prefixed(LEGACY_ENV_PREFIX)
                    .split(ENV_SEPARATOR)
                    .filter(|key| key.as_str().to_ascii_lowercase().starts_with("storage.")),
            )
            .merge(Env::prefixed(ENV_PREFIX).split(ENV_SEPARATOR).filter(|key| key.as_str().to_ascii_lowercase().starts_with("storage.")))
            .extract()
            .context("invalid storage configuration")?;
        Ok(Self { figment, locator, read_saved: std::env::var_os("RBIT_IGNORE_ADMIN_CONFIG").is_none() })
    }
    /// A deployment locator/data fact, never accepted SQL/KV or runtime authority.
    pub(crate) fn deployment(&self) -> &Config {
        &self.locator
    }
    pub(crate) fn reads_saved_configuration(&self) -> bool {
        self.read_saved
    }
    /// No database open or key loading here: callers supply their already-owned store.
    pub(crate) async fn read_saved(&self, database: &Database) -> anyhow::Result<Option<SavedConfig>> {
        if !self.read_saved {
            return Ok(None);
        }
        let entries = database.instance_settings().load().await?;
        let key = database.sealing_key().clone();
        Ok(Some(SavedConfig { entries, key }))
    }
    pub(crate) fn finish(self, saved: Option<SavedConfig>) -> anyhow::Result<Config> {
        let mut figment = self.figment;
        if let Some(SavedConfig { entries, key }) = saved
            && let Some((_, document)) = entries.into_iter().find(|(key, _)| key == rbit_services::admin::configuration::CONFIGURATION_KEY)
        {
            let config = rbit_services::admin::configuration::open_stored(&document, &key)
                .map_err(anyhow::Error::msg)
                .context("invalid administrator configuration; set RBIT_IGNORE_ADMIN_CONFIG=1 to recover")?;
            let mut value = serde_json::to_value(config)?;
            value.as_object_mut().expect("config is an object").remove("storage");
            figment = figment.merge(Serialized::defaults(value));
        }
        for prefix in [LEGACY_ENV_PREFIX, ENV_PREFIX] {
            let env = Env::prefixed(prefix)
                .split(ENV_SEPARATOR)
                .ignore(&["config", "ignore_admin_config"])
                .filter(|key| !AuthConfig::is_provider_env_key(key.as_str()));
            figment = figment.merge(env);
        }
        let mut config: Config = figment.extract().context("invalid configuration")?;
        config
            .auth
            .apply_provider_env(provider_env(std::env::vars()))
            .map_err(|problem| anyhow::anyhow!("invalid configuration: {problem}"))?;
        config.auth.validate().map_err(|problems| anyhow::anyhow!("invalid [auth] configuration: {problems}"))?;
        rbit_services::admin::configuration::validate_configuration(&config).map_err(anyhow::Error::msg)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests;
