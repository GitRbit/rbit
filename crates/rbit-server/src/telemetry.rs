//! Structured logging and metrics. Override log verbosity with `RUST_LOG`, e.g.
//! `RUST_LOG=debug`. Metrics are always recorded (it is cheap) and exposed at `/metrics`
//! only when `[operations] metrics = true`.

use std::io::IsTerminal;

use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_FILTER: &str = "info,tower_http=info,sqlx=warn";

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    // Colour only on a terminal (journald and `docker logs` would store raw escape
    // codes), and never when `NO_COLOR` is set.
    let ansi = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
    fmt().with_env_filter(filter).with_target(false).with_ansi(ansi).compact().init();
    // Before anything records, so start-up and background-job metrics are kept.
    rbit_web::metrics::install();
}
