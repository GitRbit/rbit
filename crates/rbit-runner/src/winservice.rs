//! The Windows Service that `rbit-runner install --mode service` registers: the service
//! manager starts `rbit-runner --config <file> run --as-service <name>`, which hands the
//! process to the dispatcher here.
//!
//! The service reports itself running, turns the manager's stop, shutdown and pre-shutdown
//! notices into the same stop request as Ctrl-C ([`crate::shutdown::request`]) while it
//! reports itself stopping for the grace period, and reports a failure as exit code 1, so
//! the recovery actions `install` set restart it.

use std::{ffi::OsString, path::PathBuf, sync::OnceLock, time::Duration};

use anyhow::Context as _;
use windows_service::{
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

/// The configuration and service name, for the dispatcher's thread.
static SERVICE: OnceLock<(PathBuf, String)> = OnceLock::new();
static HANDLE: OnceLock<ServiceStatusHandle> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

/// Runs the runner as the service `name`; returns when it has stopped.
pub fn run(config: PathBuf, name: &str) -> anyhow::Result<()> {
    let _ = SERVICE.set((config, name.to_owned()));
    service_dispatcher::start(name, ffi_service_main)
        .context("--as-service runs only under the Windows service manager: `rbit-runner start` starts the service")
}

fn status(state: ServiceState, wait: Duration, failed: bool) -> ServiceStatus {
    let accepted = if state == ServiceState::Running {
        ServiceControlAccept::STOP | ServiceControlAccept::PRESHUTDOWN
    } else {
        ServiceControlAccept::empty()
    };
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accepted,
        exit_code: ServiceExitCode::Win32(u32::from(failed)),
        checkpoint: 0,
        wait_hint: wait,
        process_id: None,
    }
}

fn service_main(_arguments: Vec<OsString>) {
    let Some((config, name)) = SERVICE.get().cloned() else { return };
    let grace = crate::config::load(&config)
        .map_or(Duration::from_secs(600), |loaded| Duration::from_secs(loaded.config.shutdown_grace_seconds + 60));
    let handler = move |control| match control {
        ServiceControl::Stop | ServiceControl::Preshutdown | ServiceControl::Shutdown => {
            tracing::info!("the service manager asks the runner to stop");
            if let Some(handle) = HANDLE.get() {
                let _ = handle.set_service_status(status(ServiceState::StopPending, grace, false));
            }
            crate::shutdown::request();
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let handle = match service_control_handler::register(&name, handler) {
        Ok(handle) => handle,
        Err(error) => {
            tracing::error!(%error, "cannot register with the service manager");
            return;
        }
    };
    let _ = HANDLE.set(handle);
    let _ = handle.set_service_status(status(ServiceState::Running, Duration::ZERO, false));
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(anyhow::Error::from)
        .and_then(|runtime| runtime.block_on(crate::run(&config, false, true)));
    if let Err(error) = &result {
        tracing::error!("{error:#}");
    }
    let _ = handle.set_service_status(status(ServiceState::Stopped, Duration::ZERO, result.is_err()));
}
