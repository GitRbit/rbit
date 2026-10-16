//! Layered configuration: built-in defaults → TOML file → `RUSTIT_*` environment variables.

use std::path::Path;

use anyhow::Context;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use rustit_core::Config;

/// Nested keys use a double underscore: `RUSTIT_SERVER__PUBLIC_URL`.
const ENV_PREFIX: &str = "RUSTIT_";
const ENV_SEPARATOR: &str = "__";

pub fn load(file: Option<&Path>) -> anyhow::Result<Config> {
    let mut figment = Figment::from(Serialized::defaults(Config::default()));
    if let Some(file) = file {
        anyhow::ensure!(file.exists(), "configuration file {} does not exist", file.display());
        figment = figment.merge(Toml::file(file));
    }
    figment = figment.merge(Env::prefixed(ENV_PREFIX).split(ENV_SEPARATOR).ignore(&["config"]));
    figment.extract().context("invalid configuration")
}
