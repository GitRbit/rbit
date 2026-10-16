//! Structured logging and metrics. Override log verbosity with `RUST_LOG`, e.g.
//! `RUST_LOG=debug`. Metrics are always recorded (it is cheap) and exposed at `/metrics`
//! only when `[operations] metrics = true`.

use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_FILTER: &str = "info,tower_http=info,sqlx=warn";

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    fmt().with_env_filter(filter).with_target(false).compact().init();
    // Before anything records, so start-up and background-job metrics are kept.
    rustit_web::metrics::install();
}
