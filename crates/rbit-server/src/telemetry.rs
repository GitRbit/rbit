//! Structured logging and metrics. Override log verbosity with `RUST_LOG`, e.g.
//! `RUST_LOG=debug`. Metrics are always recorded (it is cheap) and exposed at `/metrics`
//! only when `[operations] metrics = true`.
//!
//! A panic anywhere stops the whole server (release builds abort on panic), and the
//! service manager starts it again. Before it stops, [`crash_log`] records what happened.

use std::io::IsTerminal;

use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_FILTER: &str = "info,tower_http=info,sqlx=warn";

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    // Colour only on a terminal (journald and `docker logs` would store raw escape
    // codes), and never when `NO_COLOR` is set.
    let ansi = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
    fmt().with_env_filter(filter).with_target(false).with_ansi(ansi).compact().init();
    std::panic::set_hook(Box::new(crash_log));
    // Before anything records, so start-up and background-job metrics are kept.
    rbit_web::metrics::install();
}

/// The panic hook: writes the panic's message, location, thread and a backtrace to stderr
/// and to the log, so every crash is on record before the process aborts. Stderr comes
/// first, as the log may be what panicked.
fn crash_log(info: &std::panic::PanicHookInfo<'_>) {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".to_owned());
    let location = info.location().map_or_else(|| "(unknown)".to_owned(), ToString::to_string);
    let thread = std::thread::current();
    let report = crash_report(&message, &location, thread.name().unwrap_or("(unnamed)"), &std::backtrace::Backtrace::force_capture());
    eprintln!("{report}");
    tracing::error!(panic = %message, %location, thread = thread.name().unwrap_or("(unnamed)"), "rbit crashed; the server stops");
}

/// One crash as text: what panicked, where, on which thread, and how it got there.
fn crash_report(message: &str, location: &str, thread: &str, backtrace: &std::backtrace::Backtrace) -> String {
    format!("rbit crashed: panic in thread '{thread}' at {location}: {message}\nbacktrace:\n{backtrace}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_crash_report_names_the_panic_its_place_thread_and_backtrace() {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let report = super::crash_report("index out of bounds", "crates/x/src/lib.rs:7:9", "tokio-runtime-worker", &backtrace);
        let head = "rbit crashed: panic in thread 'tokio-runtime-worker' at crates/x/src/lib.rs:7:9: index out of bounds\n";
        assert!(report.starts_with(head), "{report}");
        assert!(report.contains("backtrace:\n") && report.len() > 120, "{report}");
    }
}
