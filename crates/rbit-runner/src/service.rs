//! Running the runner in the background: `rbit-runner install`, `uninstall`, `start`,
//! `stop` and `status`, with the operating system's own service manager, as GitHub's
//! runner does with `svc.sh` and `config.cmd --runasservice`.
//!
//! Two modes, the same on every system:
//!
//! | Mode | Linux | macOS | Windows |
//! |---|---|---|---|
//! | `service` (default) | systemd system unit | `LaunchDaemon` | Windows Service (session 0) |
//! | `desktop` | systemd `--user` unit | `LaunchAgent` | Task Scheduler logon task |
//!
//! A `service` starts at boot, before anyone logs in, and runs as `--user` (Linux, macOS)
//! or `NT AUTHORITY\NETWORK SERVICE` unless `--user` names another account (Windows). A
//! `desktop` runner starts when its user logs in and runs in their session, so jobs can open
//! windows, take screenshots and reach the desktop; a session-0 Windows Service can show no
//! window at all.
//!
//! Every mode restarts the runner when it fails and stops it gracefully: the first stop
//! request stops polling and gives running tasks `shutdown_grace_seconds`, as Ctrl-C does
//! ([`crate::shutdown`]). systemd and launchd send SIGTERM and wait the grace period plus a
//! minute; the Windows Service turns the service manager's stop and pre-shutdown into the
//! same request and reports itself stopping meanwhile; a logon task is stopped through a
//! stop request in the work directory ([`request_stop`]), and logging off or shutting down
//! stops it as they stop any console program.
//!
//! The service is named `rbit-runner.<runner name>` and runs the absolute path of this
//! executable with the absolute path of the configuration, so the files must stay where
//! they are; `uninstall` before moving them.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};

/// How the runner is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    /// A system service: starts at boot, runs without a desktop.
    Service,
    /// In the user's desktop session: starts when they log in.
    Desktop,
}

/// What `install` writes: the service's name, the program and configuration it runs, and
/// the account it runs as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub name: String,
    pub mode: Mode,
    pub exe: PathBuf,
    pub config: PathBuf,
    /// The account of a `service`; `None` for the default (or the current user in `desktop`).
    pub account: Option<String>,
    /// Seconds the service manager waits after a stop request: the grace period plus a minute.
    pub stop_timeout: u64,
}

/// The service name of a runner called `runner`: `rbit-runner.<name>` with only
/// `[A-Za-z0-9._-]`.
pub fn service_name(runner: &str) -> String {
    let name: String = runner.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).collect();
    format!("rbit-runner.{}", name.trim_matches('-'))
}

/// The plan for the runner configured at `config`.
pub fn plan(config: &Path, mode: Mode, account: Option<String>, name: Option<String>) -> anyhow::Result<Plan> {
    let loaded = crate::config::load(config)?;
    if mode == Mode::Desktop && account.is_some() {
        bail!("a desktop runner runs as the user who logs in; --user is for --mode service");
    }
    let config = std::fs::canonicalize(config).with_context(|| format!("cannot find {}", config.display()))?;
    let exe = std::env::current_exe().context("cannot tell where rbit-runner is")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    Ok(Plan {
        name: name.unwrap_or_else(|| service_name(&loaded.config.name)),
        mode,
        exe: strip_verbatim(exe),
        config: strip_verbatim(config),
        account,
        stop_timeout: loaded.config.shutdown_grace_seconds + 60,
    })
}

/// `\\?\C:\x` as `C:\x`: what Windows' own tools and the service manager read.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

// ------------------------------------------------------------------ the stop request

/// The file a stop request is: `stop` in the work directory, holding `now` to cancel
/// running tasks at once.
pub fn stop_file(work_dir: &Path) -> PathBuf {
    work_dir.join("stop")
}

/// Asks the runner using `work_dir` to stop, as a first (or, with `now`, a second) Ctrl-C.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn request_stop(work_dir: &Path, now: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(work_dir)?;
    std::fs::write(stop_file(work_dir), if now { "now" } else { "stop" }).context("cannot write the stop request")
}

// ------------------------------------------------------------------ the files written

/// A systemd unit: restarted on failure, stopped with SIGTERM and `stop_timeout`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn systemd_unit(plan: &Plan) -> String {
    let user = match (&plan.account, plan.mode) {
        (Some(account), Mode::Service) => format!("User={account}\n"),
        _ => String::new(),
    };
    let (after, wanted_by) = match plan.mode {
        Mode::Service => ("network-online.target", "multi-user.target"),
        Mode::Desktop => ("graphical-session.target", "graphical-session.target"),
    };
    let dir = plan.config.parent().unwrap_or_else(|| Path::new("/"));
    format!(
        "# Written by `rbit-runner install`; `rbit-runner uninstall` removes it.\n\
         [Unit]\n\
         Description=rbit Actions runner ({name})\n\
         After={after}\n\
         Wants={after}\n\
         \n\
         [Service]\n\
         Type=simple\n\
         {user}WorkingDirectory={dir}\n\
         ExecStart={exe} --config {config} run\n\
         Restart=on-failure\n\
         RestartSec=10\n\
         KillSignal=SIGTERM\n\
         KillMode=mixed\n\
         TimeoutStopSec={timeout}\n\
         \n\
         [Install]\n\
         WantedBy={wanted_by}\n",
        name = plan.name,
        dir = systemd_quote(&dir.display().to_string()),
        exe = systemd_quote(&plan.exe.display().to_string()),
        config = systemd_quote(&plan.config.display().to_string()),
        timeout = plan.stop_timeout,
    )
}

/// A path as systemd reads it in a command line or setting.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn systemd_quote(text: &str) -> String {
    if text.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | '%' | '$')) {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%").replace('$', "$$"))
    } else {
        text.to_owned()
    }
}

/// A launchd property list: kept alive when it fails, started at load.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn launchd_plist(plan: &Plan) -> String {
    let user = match (&plan.account, plan.mode) {
        (Some(account), Mode::Service) => format!("  <key>UserName</key>\n  <string>{}</string>\n", xml(account)),
        _ => String::new(),
    };
    let dir = plan.config.parent().unwrap_or_else(|| Path::new("/"));
    let log = dir.join("runner.log");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <!-- Written by `rbit-runner install`; `rbit-runner uninstall` removes it. -->\n\
         <plist version=\"1.0\">\n<dict>\n\
         \x20 <key>Label</key>\n  <string>{name}</string>\n\
         \x20 <key>ProgramArguments</key>\n  <array>\n    <string>{exe}</string>\n    <string>--config</string>\n    <string>{config}</string>\n    <string>run</string>\n  </array>\n\
         {user}\
         \x20 <key>WorkingDirectory</key>\n  <string>{dir}</string>\n\
         \x20 <key>RunAtLoad</key>\n  <true/>\n\
         \x20 <key>KeepAlive</key>\n  <dict>\n    <key>SuccessfulExit</key>\n    <false/>\n  </dict>\n\
         \x20 <key>ThrottleInterval</key>\n  <integer>10</integer>\n\
         \x20 <key>ExitTimeOut</key>\n  <integer>{timeout}</integer>\n\
         \x20 <key>StandardOutPath</key>\n  <string>{log}</string>\n\
         \x20 <key>StandardErrorPath</key>\n  <string>{log}</string>\n\
         </dict>\n</plist>\n",
        name = xml(&plan.name),
        exe = xml(&plan.exe.display().to_string()),
        config = xml(&plan.config.display().to_string()),
        dir = xml(&dir.display().to_string()),
        log = xml(&log.display().to_string()),
        timeout = plan.stop_timeout,
    )
}

#[cfg_attr(not(windows), allow(dead_code))]
/// A Task Scheduler task: at the user's logon, in their session, restarted every minute
/// when it fails, never ended for running long, one instance at a time.
pub fn logon_task_xml(plan: &Plan, user: &str) -> String {
    let dir = plan.config.parent().unwrap_or_else(|| Path::new("."));
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n\
         <Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\r\n\
         \x20 <RegistrationInfo><Description>rbit Actions runner {name}, in the desktop session. Written by rbit-runner install.</Description></RegistrationInfo>\r\n\
         \x20 <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>\r\n\
         \x20 <Principals><Principal id=\"Author\"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>\r\n\
         \x20 <Settings>\r\n\
         \x20   <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\r\n\
         \x20   <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\r\n\
         \x20   <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\r\n\
         \x20   <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\r\n\
         \x20   <RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>\r\n\
         \x20   <StartWhenAvailable>true</StartWhenAvailable>\r\n\
         \x20   <Enabled>true</Enabled>\r\n\
         \x20 </Settings>\r\n\
         \x20 <Actions Context=\"Author\"><Exec><Command>{exe}</Command><Arguments>--config \"{config}\" run</Arguments><WorkingDirectory>{dir}</WorkingDirectory></Exec></Actions>\r\n\
         </Task>\r\n",
        name = xml(&plan.name),
        user = xml(user),
        exe = xml(&plan.exe.display().to_string()),
        config = xml(&plan.config.display().to_string()),
        dir = xml(&dir.display().to_string()),
    )
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ------------------------------------------------------------------ running the tools

/// Runs `program` with `args`; its output, or an error with what it said.
fn tool(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = std::process::Command::new(program).args(args).output().with_context(|| format!("cannot run {program}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        bail!("`{program} {}` failed: {}", args.join(" "), text.trim());
    }
    Ok(text)
}

/// `rbit-runner install`.
pub fn install(plan: &Plan, start: bool) -> anyhow::Result<()> {
    platform::install(plan)?;
    if start {
        platform::start(plan)?;
    }
    println!("Installed {} ({}); `rbit-runner status` shows how it runs.", plan.name, mode_name(plan.mode));
    Ok(())
}

/// `rbit-runner uninstall`: stops the runner gracefully, then removes the service.
pub fn uninstall(plan: &Plan) -> anyhow::Result<()> {
    let _ = platform::stop(plan);
    platform::uninstall(plan)?;
    println!("Removed {}", plan.name);
    Ok(())
}

pub fn start(plan: &Plan) -> anyhow::Result<()> {
    platform::start(plan)
}

pub fn stop(plan: &Plan) -> anyhow::Result<()> {
    platform::stop(plan)
}

/// `rbit-runner status`: whether it is installed, and running.
pub fn status(plan: &Plan) -> anyhow::Result<bool> {
    let (installed, running) = platform::status(plan)?;
    println!(
        "{}: {} ({})",
        plan.name,
        match (installed, running) {
            (false, _) => "not installed",
            (true, true) => "installed, running",
            (true, false) => "installed, not running",
        },
        mode_name(plan.mode)
    );
    Ok(installed && running)
}

const fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Service => "service",
        Mode::Desktop => "desktop",
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Mode, Plan, systemd_unit, tool};

    fn unit_path(plan: &Plan) -> anyhow::Result<std::path::PathBuf> {
        Ok(match plan.mode {
            Mode::Service => std::path::PathBuf::from("/etc/systemd/system").join(format!("{}.service", plan.name)),
            Mode::Desktop => {
                let config = std::env::var_os("XDG_CONFIG_HOME")
                    .map(std::path::PathBuf::from)
                    .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config")))
                    .ok_or_else(|| anyhow::anyhow!("no home directory"))?;
                config.join("systemd/user").join(format!("{}.service", plan.name))
            }
        })
    }

    fn systemctl(plan: &Plan, args: &[&str]) -> anyhow::Result<String> {
        let mut all: Vec<&str> = if plan.mode == Mode::Desktop { vec!["--user"] } else { Vec::new() };
        all.extend_from_slice(args);
        tool("systemctl", &all)
    }

    pub fn install(plan: &Plan) -> anyhow::Result<()> {
        let path = unit_path(plan)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, systemd_unit(plan))
            .map_err(|error| anyhow::anyhow!("cannot write {} (as root for --mode service?): {error}", path.display()))?;
        systemctl(plan, &["daemon-reload"])?;
        systemctl(plan, &["enable", &format!("{}.service", plan.name)])?;
        Ok(())
    }

    pub fn uninstall(plan: &Plan) -> anyhow::Result<()> {
        let _ = systemctl(plan, &["disable", &format!("{}.service", plan.name)]);
        let path = unit_path(plan)?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        systemctl(plan, &["daemon-reload"]).map(drop)
    }

    pub fn start(plan: &Plan) -> anyhow::Result<()> {
        systemctl(plan, &["start", &format!("{}.service", plan.name)]).map(drop)
    }

    pub fn stop(plan: &Plan) -> anyhow::Result<()> {
        systemctl(plan, &["stop", &format!("{}.service", plan.name)]).map(drop)
    }

    pub fn status(plan: &Plan) -> anyhow::Result<(bool, bool)> {
        let installed = unit_path(plan)?.exists();
        let running = systemctl(plan, &["is-active", &format!("{}.service", plan.name)]).is_ok_and(|text| text.trim() == "active");
        Ok((installed, running))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Mode, Plan, launchd_plist, tool};

    fn plist_path(plan: &Plan) -> anyhow::Result<std::path::PathBuf> {
        Ok(match plan.mode {
            Mode::Service => std::path::PathBuf::from("/Library/LaunchDaemons").join(format!("{}.plist", plan.name)),
            Mode::Desktop => {
                let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("no home directory"))?;
                std::path::PathBuf::from(home).join("Library/LaunchAgents").join(format!("{}.plist", plan.name))
            }
        })
    }

    /// `system` for a daemon, `gui/<uid>` for an agent.
    fn domain(plan: &Plan) -> String {
        match plan.mode {
            Mode::Service => "system".to_owned(),
            Mode::Desktop => format!("gui/{}", tool("id", &["-u"]).unwrap_or_default().trim()),
        }
    }

    pub fn install(plan: &Plan) -> anyhow::Result<()> {
        let path = plist_path(plan)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, launchd_plist(plan))
            .map_err(|error| anyhow::anyhow!("cannot write {} (as root for --mode service?): {error}", path.display()))?;
        Ok(())
    }

    pub fn uninstall(plan: &Plan) -> anyhow::Result<()> {
        let path = plist_path(plan)?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn start(plan: &Plan) -> anyhow::Result<()> {
        let path = plist_path(plan)?;
        tool("launchctl", &["bootstrap", &domain(plan), &path.display().to_string()]).map(drop)
    }

    /// `bootout` sends SIGTERM and waits `ExitTimeOut`.
    pub fn stop(plan: &Plan) -> anyhow::Result<()> {
        tool("launchctl", &["bootout", &format!("{}/{}", domain(plan), plan.name)]).map(drop)
    }

    pub fn status(plan: &Plan) -> anyhow::Result<(bool, bool)> {
        let installed = plist_path(plan)?.exists();
        let running =
            tool("launchctl", &["print", &format!("{}/{}", domain(plan), plan.name)]).is_ok_and(|text| text.contains("state = running"));
        Ok((installed, running))
    }
}

#[cfg(windows)]
mod platform {
    use super::{Mode, Plan, logon_task_xml, tool};

    /// The account a service runs as by default, as GitHub's runner service does.
    const DEFAULT_ACCOUNT: &str = r"NT AUTHORITY\NETWORK SERVICE";

    fn current_user() -> String {
        let domain = std::env::var("USERDOMAIN").unwrap_or_default();
        let user = std::env::var("USERNAME").unwrap_or_default();
        if domain.is_empty() { user } else { format!("{domain}\\{user}") }
    }

    pub fn install(plan: &Plan) -> anyhow::Result<()> {
        match plan.mode {
            Mode::Service => {
                let command = format!("\"{}\" --config \"{}\" run --as-service {}", plan.exe.display(), plan.config.display(), plan.name);
                let account = plan.account.clone().unwrap_or_else(|| DEFAULT_ACCOUNT.to_owned());
                let password = std::env::var("RBIT_RUNNER_SERVICE_PASSWORD").unwrap_or_default();
                let mut args = vec!["create", &plan.name, "binPath=", &command, "start=", "delayed-auto", "obj=", &account];
                if !password.is_empty() {
                    args.extend(["password=", &password]);
                }
                let display = format!("rbit Actions runner ({})", plan.name);
                args.extend(["DisplayName=", &display]);
                tool("sc.exe", &args).map_err(|error| anyhow::anyhow!("{error} (run from an elevated prompt)"))?;
                // As GitHub's `config.cmd --runasservice`: the account may change the runner's
                // directory (its configuration, credential and work directory), nothing else.
                if let Some(dir) = plan.config.parent() {
                    let grant = format!("{account}:(OI)(CI)M");
                    tool("icacls.exe", &[&dir.display().to_string(), "/grant", &grant, "/T", "/Q"])?;
                }
                let _ = tool("sc.exe", &["description", &plan.name, "Runs rbit Actions jobs. Written by rbit-runner install."]);
                // Restarted after 10 s, 10 s and a minute; the count resets after a day.
                tool("sc.exe", &["failure", &plan.name, "reset=", "86400", "actions=", "restart/10000/restart/10000/restart/60000"])?;
                tool("sc.exe", &["failureflag", &plan.name, "1"])?;
                // The pre-shutdown notice gives running tasks their grace period when Windows shuts down.
                let timeout = (plan.stop_timeout * 1000).to_string();
                let _ = tool(
                    "reg.exe",
                    &[
                        "add",
                        &format!(r"HKLM\SYSTEM\CurrentControlSet\Services\{}", plan.name),
                        "/v",
                        "PreshutdownTimeout",
                        "/t",
                        "REG_DWORD",
                        "/d",
                        &timeout,
                        "/f",
                    ],
                );
                Ok(())
            }
            Mode::Desktop => {
                let xml = logon_task_xml(plan, &current_user());
                // schtasks reads the XML as UTF-16, as its declaration says.
                let mut bytes = vec![0xFF, 0xFE];
                bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
                let file = std::env::temp_dir().join(format!("{}.xml", plan.name));
                std::fs::write(&file, bytes)?;
                let created = tool("schtasks.exe", &["/create", "/f", "/tn", &plan.name, "/xml", &file.display().to_string()]);
                let _ = std::fs::remove_file(&file);
                created.map(drop)
            }
        }
    }

    pub fn uninstall(plan: &Plan) -> anyhow::Result<()> {
        match plan.mode {
            Mode::Service => tool("sc.exe", &["delete", &plan.name]).map(drop),
            Mode::Desktop => tool("schtasks.exe", &["/delete", "/f", "/tn", &plan.name]).map(drop),
        }
    }

    pub fn start(plan: &Plan) -> anyhow::Result<()> {
        match plan.mode {
            Mode::Service => tool("sc.exe", &["start", &plan.name]).map(drop),
            Mode::Desktop => tool("schtasks.exe", &["/run", "/tn", &plan.name]).map(drop),
        }
    }

    /// The service manager's stop is graceful ([`super::super::winservice`]); a logon task
    /// gets a stop request, and the call waits until the runner is gone.
    pub fn stop(plan: &Plan) -> anyhow::Result<()> {
        match plan.mode {
            Mode::Service => {
                tool("sc.exe", &["stop", &plan.name])?;
                wait(plan, |plan| status(plan).is_ok_and(|(_, running)| !running))
            }
            Mode::Desktop => {
                let loaded = crate::config::load(&plan.config)?;
                super::request_stop(&loaded.work_dir, false)?;
                wait(plan, |plan| status(plan).is_ok_and(|(_, running)| !running))
            }
        }
    }

    fn wait(plan: &Plan, done: impl Fn(&Plan) -> bool) -> anyhow::Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(plan.stop_timeout);
        while std::time::Instant::now() < deadline {
            if done(plan) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        anyhow::bail!("{} did not stop within {} seconds", plan.name, plan.stop_timeout)
    }

    #[allow(clippy::unnecessary_wraps)] // the same signature on every system
    pub fn status(plan: &Plan) -> anyhow::Result<(bool, bool)> {
        match plan.mode {
            Mode::Service => match tool("sc.exe", &["query", &plan.name]) {
                Ok(text) => Ok((true, text.contains("RUNNING") || text.contains("STOP_PENDING"))),
                Err(_) => Ok((false, false)),
            },
            Mode::Desktop => match tool("schtasks.exe", &["/query", "/tn", &plan.name, "/fo", "csv", "/nh"]) {
                Ok(text) => Ok((true, text.contains("\"Running\""))),
                Err(_) => Ok((false, false)),
            },
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::Plan;

    fn unsupported() -> anyhow::Error {
        anyhow::anyhow!("installing a service is supported on Linux, macOS and Windows")
    }

    pub fn install(_: &Plan) -> anyhow::Result<()> {
        Err(unsupported())
    }
    pub fn uninstall(_: &Plan) -> anyhow::Result<()> {
        Err(unsupported())
    }
    pub fn start(_: &Plan) -> anyhow::Result<()> {
        Err(unsupported())
    }
    pub fn stop(_: &Plan) -> anyhow::Result<()> {
        Err(unsupported())
    }
    pub fn status(_: &Plan) -> anyhow::Result<(bool, bool)> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(mode: Mode, account: Option<&str>) -> Plan {
        Plan {
            name: service_name("build 1/linux"),
            mode,
            exe: PathBuf::from("/opt/rbit runner/rbit-runner"),
            config: PathBuf::from("/var/lib/rbit-runner/runner.toml"),
            account: account.map(str::to_owned),
            stop_timeout: 660,
        }
    }

    #[test]
    fn names_are_safe_for_every_service_manager() {
        assert_eq!(service_name("build 1/linux"), "rbit-runner.build-1-linux");
        assert_eq!(service_name("pc2"), "rbit-runner.pc2");
    }

    #[test]
    fn systemd_units_restart_and_stop_gracefully() {
        let unit = systemd_unit(&sample(Mode::Service, Some("rbit-runner")));
        for line in [
            "User=rbit-runner",
            "ExecStart=\"/opt/rbit runner/rbit-runner\" --config /var/lib/rbit-runner/runner.toml run",
            "Restart=on-failure",
            "KillSignal=SIGTERM",
            "TimeoutStopSec=660",
            "WantedBy=multi-user.target",
        ] {
            assert!(unit.lines().any(|candidate| candidate == line), "{line}\n{unit}");
        }
        let user = systemd_unit(&sample(Mode::Desktop, None));
        assert!(!user.contains("User=") && user.contains("WantedBy=graphical-session.target"), "{user}");
    }

    #[test]
    fn launchd_and_task_scheduler_keep_it_running() {
        let plist = launchd_plist(&sample(Mode::Service, Some("_rbit")));
        for needle in [
            "<string>rbit-runner.build-1-linux</string>",
            "<key>UserName</key>\n  <string>_rbit</string>",
            "<key>SuccessfulExit</key>\n    <false/>",
            "<key>ExitTimeOut</key>\n  <integer>660</integer>",
            "<string>/opt/rbit runner/rbit-runner</string>",
        ] {
            assert!(plist.contains(needle), "{needle}\n{plist}");
        }
        assert!(!launchd_plist(&sample(Mode::Desktop, None)).contains("UserName"));
        let task = logon_task_xml(&sample(Mode::Desktop, None), r"PC\alice & bob");
        for needle in [
            "<LogonTrigger><Enabled>true</Enabled><UserId>PC\\alice &amp; bob</UserId>",
            "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
            "<RestartOnFailure><Interval>PT1M</Interval>",
            "<LogonType>InteractiveToken</LogonType>",
            "<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
        ] {
            assert!(task.contains(needle), "{needle}\n{task}");
        }
    }

    #[test]
    fn paths_lose_the_verbatim_prefix() {
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\C:\rbit\runner.toml")), PathBuf::from(r"C:\rbit\runner.toml"));
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\UNC\server\share")), PathBuf::from(r"\\?\UNC\server\share"));
    }

    #[test]
    fn stop_requests_are_files_in_the_work_directory() {
        let dir = tempfile::tempdir().unwrap();
        request_stop(dir.path(), false).unwrap();
        assert_eq!(std::fs::read_to_string(stop_file(dir.path())).unwrap(), "stop");
        request_stop(dir.path(), true).unwrap();
        assert_eq!(std::fs::read_to_string(stop_file(dir.path())).unwrap(), "now");
    }
}
