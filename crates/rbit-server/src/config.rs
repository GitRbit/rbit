//! Layered configuration: built-in defaults → TOML file → `RUSTIT_*` environment variables
//! (the names from before the rename to rbit, still honoured) → `RBIT_*` environment variables.

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

pub fn load(file: Option<&Path>) -> anyhow::Result<Config> {
    let mut figment = Figment::from(Serialized::defaults(Config::default()));
    if let Some(file) = file {
        anyhow::ensure!(file.exists(), "configuration file {} does not exist", file.display());
        figment = figment.merge(Toml::file(file));
    }
    for prefix in [LEGACY_ENV_PREFIX, ENV_PREFIX] {
        // Milestone 3 (O): `AUTH__PROVIDERS__<N>__…` would replace the whole providers
        // array in figment; those are applied below instead.
        let env =
            Env::prefixed(prefix).split(ENV_SEPARATOR).ignore(&["config"]).filter(|key| !AuthConfig::is_provider_env_key(key.as_str()));
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
