//! Structured logging. Override verbosity with `RUST_LOG`, e.g. `RUST_LOG=debug`.

use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_FILTER: &str = "info,tower_http=info,sqlx=warn";

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    fmt().with_env_filter(filter).with_target(false).compact().init();
}
