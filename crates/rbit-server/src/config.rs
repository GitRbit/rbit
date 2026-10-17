//! Layered configuration: built-in defaults → TOML file → `RUSTIT_*` environment variables
//! (the names from before the rename to Rbit, still honoured) → `RBIT_*` environment variables.

use std::path::Path;

use anyhow::Context;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use rbit_core::Config;

/// Nested keys use a double underscore: `RBIT_SERVER__PUBLIC_URL`.
const ENV_PREFIX: &str = "RBIT_";
/// The prefix from before the rename to Rbit; `RBIT_*` wins where both are set.
const LEGACY_ENV_PREFIX: &str = "RUSTIT_";
const ENV_SEPARATOR: &str = "__";

pub fn load(file: Option<&Path>) -> anyhow::Result<Config> {
    let mut figment = Figment::from(Serialized::defaults(Config::default()));
    if let Some(file) = file {
        anyhow::ensure!(file.exists(), "configuration file {} does not exist", file.display());
        figment = figment.merge(Toml::file(file));
    }
    for prefix in [LEGACY_ENV_PREFIX, ENV_PREFIX] {
        figment = figment.merge(Env::prefixed(prefix).split(ENV_SEPARATOR).ignore(&["config"]));
    }
    figment.extract().context("invalid configuration")
}
