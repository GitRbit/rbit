//! The container backend (§12.5.2): jobs run in Docker or Podman containers, driven
//! through their command-line interface rather than a daemon API.
//!
//! For each task:
//!
//! 1. `network create rbit-<task>` with the configured driver;
//! 2. each service: `run -d --network rbit-<task> --network-alias <id>`;
//! 3. the job container: `create` with the network, the workspace, `temp` and `files`
//!    bind-mounted under `/__w`, `--security-opt no-new-privileges`, `--pids-limit`,
//!    `--memory`, `--cpus` and the entrypoint `tail -f /dev/null`, then `start`;
//! 4. one `exec` per step.
//!
//! Built-in steps work on the mounted directories from the host (§12.5.5, §12.5.6); while
//! they do, the job container is paused (`pause`, then `unpause`), so no process a step
//! left running can change those directories under them. A container that cannot be
//! paused fails the built-in step.
//!
//! Values never travel as arguments: a container's, a service's and every step's
//! environment goes through an `--env-file` (mode `0600`, in a directory no container
//! mounts, deleted right after the command), and a multi-line value — which env files
//! cannot hold — as `-e NAME` with the value in the CLI's own environment. That CLI runs
//! on the host, so names are checked first ([`EnvFile::write`]): only
//! `[A-Za-z_][A-Za-z0-9_.-]*` (nothing that could split or empty an env file's line, which
//! would copy one of the runner's own variables into the container), and no multi-line
//! value for a name the CLI, its loader or its C library reads ([`reaches_the_engine`]),
//! such as `LD_PRELOAD` or `DOCKER_HOST`. Registry credentials go through `login
//! --password-stdin` into a per-task configuration (`DOCKER_CONFIG`, or
//! `REGISTRY_AUTH_FILE` for Podman), set after the job's values so none can replace it.
//!
//! Every container drops `MKNOD`: a job cannot make device nodes in the directories the
//! runner works on from the host.
//!
//! The backend never uses `--privileged`, never mounts the Docker socket and never uses
//! the host's network. Everything it creates is labelled `rbit.task` and `rbit.runner`;
//! [`janitor`] removes what a crashed runner left behind. Each command line is built by a
//! function of this module, so the argument vectors are tested without Docker.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

use rbit_workflow::protocol::{MAX_LOG_LINE_BYTES, TaskId};
use tokio::process::Command;

use super::{Dirs, StepCommand, View};
use crate::{
    config::{ContainerConfig, RunnerConfig},
    process::{self, Exit, Stopper},
    task::Reporter,
};

/// Where the workspace, `temp` and `files` appear inside the job container.
pub const MOUNT_ROOT: &str = "/__w";
/// The `PATH` assumed when the image's own cannot be read.
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// How long a short CLI command (a probe, `rm`) may take.
const SHORT: Duration = Duration::from_secs(120);

/// A container to start: the job's or a service's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub image: String,
    pub env: BTreeMap<String, String>,
    /// `(username, password)`, evaluated.
    pub credentials: Option<(String, String)>,
}

/// The container backend of one task.
#[derive(Debug)]
pub struct Container {
    program: PathBuf,
    names: Names,
    config: ContainerConfig,
    dirs: Dirs,
    /// `<task dir>/private`: env files and registry logins, mounted nowhere.
    private: PathBuf,
    view: View,
    /// What the job container has, probed after it started.
    shells: BTreeSet<String>,
    base_path: String,
    started: Vec<String>,
    network_created: bool,
}

/// The names of a task's containers and network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    pub task: TaskId,
    pub runner: String,
    pub network: String,
    pub job: String,
}

impl Names {
    pub fn new(task: TaskId, runner: &str) -> Self {
        Self { task, runner: runner.to_owned(), network: format!("rbit-{task}"), job: format!("rbit-{task}-job") }
    }

    pub fn service(&self, id: &str) -> String {
        format!("rbit-{}-{id}", self.task)
    }

    /// `--label` arguments marking a resource as this task's.
    fn labels(&self) -> Vec<String> {
        vec!["--label".into(), format!("rbit.task={}", self.task), "--label".into(), format!("rbit.runner={}", self.runner)]
    }
}

/// The task's directories inside the job container: `/__w/<repository name>`,
/// `/__w/_temp` and `/__w/_files`.
pub fn view(repository: &str) -> View {
    let name = repository.rsplit('/').next().filter(|name| !name.is_empty() && !name.starts_with('_')).unwrap_or("workspace");
    View { workspace: format!("{MOUNT_ROOT}/{name}"), temp: format!("{MOUNT_ROOT}/_temp"), files: format!("{MOUNT_ROOT}/_files") }
}

/// Whether `image` can be passed as an image reference: not empty, no whitespace or
/// control characters, and not starting with `-`, so it can never be read as an option.
pub fn valid_image(image: &str) -> bool {
    !image.is_empty() && !image.starts_with('-') && !image.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The registry an image is pulled from, as `login` names it.
pub fn registry(image: &str) -> &str {
    match image.split_once('/') {
        Some((first, _)) if first.contains(['.', ':']) || first == "localhost" => first,
        _ => "docker.io",
    }
}

/// `network create`.
pub fn network_create(names: &Names, driver: &str) -> Vec<String> {
    let mut args = vec!["network".into(), "create".into(), "--driver".into(), driver.to_owned()];
    args.extend(names.labels());
    args.push(names.network.clone());
    args
}

/// The limits every container of a task gets.
fn limits(config: &ContainerConfig) -> Vec<String> {
    let mut args = vec![
        "--security-opt".into(),
        "no-new-privileges".into(),
        // No device nodes in the mounted directories, which the runner opens from the host.
        "--cap-drop".into(),
        "MKNOD".into(),
        "--pids-limit".into(),
        config.pids_limit.to_string(),
    ];
    if let Some(memory) = &config.memory {
        args.extend(["--memory".into(), memory.clone()]);
    }
    if let Some(cpus) = config.cpus {
        args.extend(["--cpus".into(), cpus.to_string()]);
    }
    args
}

/// `run -d` of service `id`; its environment is in `env_file`.
pub fn service_run(names: &Names, config: &ContainerConfig, id: &str, image: &str, env_file: &Path, inherited: &[String]) -> Vec<String> {
    let mut args = vec!["run".into(), "--detach".into(), "--name".into(), names.service(id)];
    args.extend(["--network".into(), names.network.clone(), "--network-alias".into(), id.to_owned()]);
    args.extend(names.labels());
    args.extend(limits(config));
    args.extend(env_args(env_file, inherited));
    args.push(image.to_owned());
    args
}

/// `create` of the job container.
pub fn job_create(
    names: &Names,
    config: &ContainerConfig,
    dirs: &Dirs,
    view: &View,
    image: &str,
    env_file: &Path,
    inherited: &[String],
) -> Vec<String> {
    let mut args = vec!["create".into(), "--name".into(), names.job.clone(), "--network".into(), names.network.clone()];
    for (source, target) in [(&dirs.workspace, &view.workspace), (&dirs.temp, &view.temp), (&dirs.files, &view.files)] {
        args.extend(["--mount".into(), format!("type=bind,source={},target={target}", source.display())]);
    }
    args.extend(["--workdir".into(), view.workspace.clone()]);
    args.extend(names.labels());
    args.extend(limits(config));
    args.extend(env_args(env_file, inherited));
    args.extend(["--entrypoint".into(), "tail".into(), image.to_owned(), "-f".into(), "/dev/null".into()]);
    args
}

/// `exec` of a step in the job container.
pub fn step_exec(names: &Names, step: &StepCommand, env_file: &Path, inherited: &[String]) -> Vec<String> {
    let mut args = vec!["exec".into()];
    args.extend(env_args(env_file, inherited));
    args.extend(["--workdir".into(), step.cwd.clone(), names.job.clone(), step.program.clone()]);
    args.extend(step.args.iter().cloned());
    args
}

/// `--env-file <file>`, and `-e NAME` for each value passed through the CLI's environment.
fn env_args(env_file: &Path, inherited: &[String]) -> Vec<String> {
    let mut args = vec!["--env-file".into(), env_file.display().to_string()];
    for name in inherited {
        args.extend(["-e".into(), name.clone()]);
    }
    args
}

/// `rm -f` of the task's containers.
pub fn remove(containers: &[String]) -> Vec<String> {
    ["rm".to_owned(), "--force".to_owned()].into_iter().chain(containers.iter().cloned()).collect()
}

/// Whether a variable called `name` changes how the container CLI itself runs on the host:
/// what its dynamic loader and C library read (glibc's list of variables it ignores for
/// set-user-ID programs), the Go runtime's, the engines' own, trust stores and proxies,
/// and the paths and identity it runs with. Such a name may not carry a multi-line value,
/// the one kind that travels in the CLI's environment. Case is ignored (Windows).
pub fn reaches_the_engine(name: &str) -> bool {
    const PREFIXES: [&str; 18] = [
        "LD_",
        "DYLD_",
        "MALLOC_",
        "GLIBC_",
        "GO",
        "DOCKER_",
        "PODMAN",
        "CONTAINER_",
        "CONTAINERS_",
        "_CONTAINERS_",
        "BUILDAH_",
        "STORAGE_",
        "XDG_",
        "SSL_",
        "CURL_",
        "REGISTRY_",
        "NIX_",
        "DBUS_",
    ];
    const NAMES: [&str; 26] = [
        "GCONV_PATH",
        "GETCONF_DIR",
        "HOSTALIASES",
        "LOCALDOMAIN",
        "LOCPATH",
        "NIS_PATH",
        "NLSPATH",
        "RESOLV_HOST_CONF",
        "RES_OPTIONS",
        "TMPDIR",
        "TZDIR",
        "PATH",
        "HOME",
        "USER",
        "SHELL",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "USERPROFILE",
        "TEMP",
        "TMP",
        "IFS",
    ];
    let upper = name.to_ascii_uppercase();
    PREFIXES.iter().any(|prefix| upper.starts_with(prefix)) || NAMES.contains(&upper.as_str()) || upper.ends_with("_PROXY")
}

/// An env file that is deleted when dropped.
#[derive(Debug)]
pub struct EnvFile {
    path: PathBuf,
    /// Multi-line values, passed as `-e NAME` from the CLI's environment.
    pub inherited: Vec<(String, String)>,
}

impl EnvFile {
    /// Writes `env` to `path` (mode `0600` on Unix): `NAME=value` lines for single-line
    /// values, the rest left to [`Self::inherited`]. A name that is not a valid variable
    /// name, or a multi-line value of one that [`reaches_the_engine`], is refused before
    /// anything is written.
    pub fn write(path: &Path, env: &BTreeMap<String, String>) -> std::io::Result<Self> {
        let refused = |message: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, message);
        for (name, value) in env {
            if !crate::envfiles::valid_variable_name(name) {
                return Err(refused(format!("`{}` is not a valid variable name", name.escape_debug())));
            }
            if value.contains(['\n', '\r']) && reaches_the_engine(name) {
                return Err(refused(format!(
                    "`{name}` cannot hold a multi-line value in a container job: it would reach the container engine's own process"
                )));
            }
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        let mut inherited = Vec::new();
        let mut text = String::new();
        for (name, value) in env {
            if value.contains(['\n', '\r']) {
                inherited.push((name.clone(), value.clone()));
            } else {
                text.push_str(name);
                text.push('=');
                text.push_str(value);
                text.push('\n');
            }
        }
        file.write_all(text.as_bytes())?;
        Ok(Self { path: path.to_path_buf(), inherited })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn names(&self) -> Vec<String> {
        self.inherited.iter().map(|(name, _)| name.clone()).collect()
    }
}

impl Drop for EnvFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Container {
    pub fn new(runner: &RunnerConfig, task: TaskId, repository: &str, dirs: &Dirs, task_dir: &Path) -> Result<Self, String> {
        let program = process::find_program(&runner.container.program)
            .ok_or_else(|| format!("`{}` is not on PATH; the container backend needs Docker or Podman", runner.container.program))?;
        for dir in [&dirs.workspace, &dirs.temp, &dirs.files] {
            if dir.to_string_lossy().contains(',') {
                return Err(format!("the work directory {} cannot be mounted: it contains a comma", dir.display()));
            }
        }
        let private = task_dir.join("private");
        std::fs::create_dir_all(&private).map_err(|error| format!("cannot create {}: {error}", private.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700));
        }
        Ok(Self {
            program,
            names: Names::new(task, &runner.name),
            config: runner.container.clone(),
            dirs: dirs.clone(),
            private,
            view: view(repository),
            shells: BTreeSet::new(),
            base_path: DEFAULT_PATH.to_owned(),
            started: Vec::new(),
            network_created: false,
        })
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn base_path(&self) -> String {
        self.base_path.clone()
    }

    /// The program `name` as the job container has it.
    pub fn find(&self, name: &str) -> Option<String> {
        self.shells.contains(name).then(|| name.to_owned())
    }

    /// The CLI with `inherited` (multi-line values, checked by [`EnvFile::write`]) and then
    /// this task's registry configuration, which nothing of the job's can replace.
    fn cli_with(&self, inherited: &[(String, String)]) -> Command {
        let mut command = Command::new(&self.program);
        command.envs(inherited.iter().map(|(name, value)| (name, value)));
        command.env("DOCKER_CONFIG", self.private.join("docker")).env("REGISTRY_AUTH_FILE", self.private.join("auth.json"));
        command
    }

    /// The CLI with this task's registry configuration.
    fn cli(&self) -> Command {
        self.cli_with(&[])
    }

    /// Runs the CLI with `args`, logging the command line and its output.
    async fn run(&self, reporter: &Reporter, args: &[String], inherited: &[(String, String)]) -> Result<(), String> {
        reporter.log(&format!("[command]{} {}", self.program_name(), args.join(" ")));
        let mut command = self.cli_with(inherited);
        command.args(args);
        let cancel = reporter.cancellation().clone();
        match process::run_streaming(command, MAX_LOG_LINE_BYTES, async move { cancel.cancelled().await }, &Stopper::Host, |line| {
            reporter.log(&line);
        })
        .await
        {
            Ok(Exit::Code(Some(0))) => Ok(()),
            Ok(Exit::Stopped) => Err("cancelled".to_owned()),
            Ok(Exit::Code(code)) => Err(format!("`{} {}` failed ({code:?})", self.program_name(), args.first().map_or("", String::as_str))),
            Err(error) => Err(format!("cannot run {}: {error}", self.program.display())),
        }
    }

    fn program_name(&self) -> String {
        self.program.file_stem().map_or_else(|| "docker".to_owned(), |stem| stem.to_string_lossy().into_owned())
    }

    /// Logs in to `image`'s registry with `credentials`, the password on standard input.
    async fn login(&self, reporter: &Reporter, image: &str, credentials: &(String, String)) -> Result<(), String> {
        let (username, password) = credentials;
        reporter.add_mask(password);
        let args =
            ["login".to_owned(), "--username".to_owned(), username.clone(), "--password-stdin".to_owned(), registry(image).to_owned()];
        reporter.log(&format!("[command]{} {}", self.program_name(), args.join(" ")));
        let mut command = self.cli();
        command.args(&args);
        let output = process::capture(command, Some(password.as_bytes()), SHORT).await.map_err(|error| format!("login failed: {error}"))?;
        for line in output.stdout.lines().chain(output.stderr.lines()) {
            reporter.log(line);
        }
        if output.success { Ok(()) } else { Err(format!("cannot log in to {}", registry(image))) }
    }

    /// Creates the network, starts the services and the job container, and probes it.
    pub async fn start(&mut self, reporter: &Reporter, job: &Spec, services: &[(String, Spec)]) -> Result<(), String> {
        for image in std::iter::once(&job.image).chain(services.iter().map(|(_, service)| &service.image)) {
            if !valid_image(image) {
                return Err(format!("`{image}` is not a valid image name"));
            }
        }
        for spec in std::iter::once(job).chain(services.iter().map(|(_, service)| service)) {
            if let Some(credentials) = &spec.credentials {
                self.login(reporter, &spec.image, credentials).await?;
            }
        }
        self.run(reporter, &network_create(&self.names, &self.config.network), &[]).await?;
        self.network_created = true;
        for (id, service) in services {
            self.run(reporter, &["pull".to_owned(), service.image.clone()], &[]).await?;
            let env = EnvFile::write(&self.private.join(format!("service-{id}.env")), &service.env).map_err(|error| error.to_string())?;
            self.started.push(self.names.service(id));
            self.run(reporter, &service_run(&self.names, &self.config, id, &service.image, env.path(), &env.names()), &env.inherited)
                .await?;
        }
        self.run(reporter, &["pull".to_owned(), job.image.clone()], &[]).await?;
        let env = EnvFile::write(&self.private.join("job.env"), &job.env).map_err(|error| error.to_string())?;
        self.started.push(self.names.job.clone());
        let create = job_create(&self.names, &self.config, &self.dirs, &self.view, &job.image, env.path(), &env.names());
        self.run(reporter, &create, &env.inherited).await?;
        drop(env);
        self.run(reporter, &["start".to_owned(), self.names.job.clone()], &[]).await?;
        self.probe().await;
        Ok(())
    }

    /// Which shells the job container has, and its `PATH`.
    async fn probe(&mut self) {
        let script = "echo \"PATH=$PATH\"; for s in bash sh pwsh powershell python python3; do command -v \"$s\" >/dev/null 2>&1 && echo \"$s\"; done";
        let mut command = self.cli();
        command.args(["exec", &self.names.job, "sh", "-c", script]);
        if let Ok(output) = process::capture(command, None, SHORT).await {
            for line in output.stdout.lines() {
                match line.strip_prefix("PATH=") {
                    Some(path) if !path.is_empty() => path.clone_into(&mut self.base_path),
                    Some(_) => {}
                    None => _ = self.shells.insert(line.trim().to_owned()),
                }
            }
        }
    }

    /// Freezes the job container's processes, while the runner works on its directories.
    pub async fn pause(&self) -> Result<(), String> {
        self.pause_command("pause").await
    }

    /// Lets the job container's processes go on.
    pub async fn resume(&self) -> Result<(), String> {
        self.pause_command("unpause").await
    }

    async fn pause_command(&self, verb: &str) -> Result<(), String> {
        let mut command = self.cli();
        command.args([verb, &self.names.job]);
        let output = process::capture(command, None, SHORT).await.map_err(|error| format!("`{verb}` failed: {error}"))?;
        if output.success {
            Ok(())
        } else {
            let detail = output.stderr.lines().chain(output.stdout.lines()).find(|line| !line.trim().is_empty()).unwrap_or_default();
            Err(format!("`{} {verb}` failed: {}", self.program_name(), detail.trim()))
        }
    }

    /// How a step of this job is stopped: inside the container.
    pub fn stopper(&self) -> Stopper {
        Stopper::Container {
            program: self.program.clone(),
            env: vec![
                ("DOCKER_CONFIG".to_owned(), self.private.join("docker")),
                ("REGISTRY_AUTH_FILE".to_owned(), self.private.join("auth.json")),
            ],
            container: self.names.job.clone(),
        }
    }

    /// The `exec` command of `step`, and its env file, which must outlive the command.
    pub fn command(&self, step: &StepCommand) -> std::io::Result<(Command, EnvFile)> {
        let env = EnvFile::write(&self.private.join(format!("step-{}.env", crate::client::idempotency_key())), &step.env)?;
        let mut command = self.cli_with(&env.inherited);
        command.args(step_exec(&self.names, step, env.path(), &env.names()));
        Ok((command, env))
    }

    /// Hands the mounted directories back to the runner's user (a rootful engine's steps
    /// write them as root), then removes the containers and the network.
    pub async fn stop(&self, reporter: &Reporter) {
        if self.started.contains(&self.names.job) {
            // In case a built-in step was interrupted while the container was paused.
            let _ = self.resume().await;
        }
        if self.started.contains(&self.names.job)
            && let Some((uid, gid)) = owner().await
        {
            let mut command = self.cli();
            let owner = format!("{uid}:{gid}");
            command.args(["exec", &self.names.job, "chown", "-R", &owner, &self.view.workspace, &self.view.temp, &self.view.files]);
            let _ = process::capture(command, None, SHORT).await;
        }
        if !self.started.is_empty() {
            let mut command = self.cli();
            command.args(remove(&self.started));
            if let Err(error) = process::capture(command, None, SHORT).await {
                reporter.log(&format!("##[warning]cannot remove the task's containers: {error}"));
            }
        }
        if self.network_created {
            let mut command = self.cli();
            command.args(["network", "rm", &self.names.network]);
            let _ = process::capture(command, None, SHORT).await;
        }
    }
}

/// The runner user's `uid:gid` on Unix, from `id`.
async fn owner() -> Option<(String, String)> {
    if !cfg!(unix) {
        return None;
    }
    let id = process::find_program("id")?;
    let uid = process::output(&id, &["-u"], SHORT).await.ok()?.stdout;
    let gid = process::output(&id, &["-g"], SHORT).await.ok()?.stdout;
    (!uid.is_empty() && !gid.is_empty()).then_some((uid, gid))
}

/// Removes containers and networks this runner left behind (labelled `rbit.runner` with
/// its name), at start-up, when no task runs.
pub async fn janitor(config: &RunnerConfig) {
    let Some(program) = process::find_program(&config.container.program) else { return };
    let filter = format!("label=rbit.runner={}", config.name);
    for (list, remove) in
        [(["ps", "--all"].as_slice(), ["rm", "--force"].as_slice()), (["network", "ls"].as_slice(), ["network", "rm"].as_slice())]
    {
        let mut command = Command::new(&program);
        command.args(list).args(["--filter", &filter, "--format", "{{.ID}}"]);
        let Ok(output) = process::capture(command, None, SHORT).await else { continue };
        let ids: Vec<&str> = output.stdout.lines().map(str::trim).filter(|id| !id.is_empty()).collect();
        if output.success && !ids.is_empty() {
            tracing::info!(count = ids.len(), "removing containers and networks left by an earlier run");
            let mut command = Command::new(&program);
            command.args(remove).args(&ids);
            let _ = process::capture(command, None, SHORT).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names {
        Names::new(TaskId(42), "build-1")
    }

    fn config() -> ContainerConfig {
        ContainerConfig { memory: Some("4g".into()), cpus: Some(2.0), ..ContainerConfig::default() }
    }

    fn dirs() -> Dirs {
        Dirs { workspace: "/work/tasks/42/workspace".into(), temp: "/work/tasks/42/temp".into(), files: "/work/tasks/42/files".into() }
    }

    /// Nothing that would give a job the host.
    fn assert_confined(args: &[String]) {
        let line = args.join(" ");
        let forbidden = [
            "--privileged",
            "docker.sock",
            "--network host",
            "--network=host",
            "--net=",
            "--net ",
            "--pid=",
            "--pid ",
            "--userns",
            "--cap-add",
            "--device",
        ];
        for forbidden in forbidden {
            assert!(!line.contains(forbidden), "{forbidden} in {line}");
        }
    }

    #[test]
    fn the_job_container_is_confined_and_mounted_under_w() {
        let view = view("alice/app");
        assert_eq!(view, View { workspace: "/__w/app".into(), temp: "/__w/_temp".into(), files: "/__w/_files".into() });
        let args = job_create(&names(), &config(), &dirs(), &view, "docker.io/library/rust:1.94", Path::new("/p/job.env"), &["KEY".into()]);
        assert_confined(&args);
        let line = args.join(" ");
        for part in [
            "create --name rbit-42-job --network rbit-42",
            "--mount type=bind,source=/work/tasks/42/workspace,target=/__w/app",
            "--mount type=bind,source=/work/tasks/42/temp,target=/__w/_temp",
            "--mount type=bind,source=/work/tasks/42/files,target=/__w/_files",
            "--workdir /__w/app",
            "--label rbit.task=42 --label rbit.runner=build-1",
            "--security-opt no-new-privileges --cap-drop MKNOD --pids-limit 4096 --memory 4g --cpus 2",
            "--env-file /p/job.env -e KEY",
        ] {
            assert!(line.contains(part), "{part:?} missing from {line}");
        }
        assert!(line.ends_with("--entrypoint tail docker.io/library/rust:1.94 -f /dev/null"), "{line}");
    }

    #[test]
    fn services_and_networks_are_per_task() {
        assert_eq!(
            network_create(&names(), "bridge").join(" "),
            "network create --driver bridge --label rbit.task=42 --label rbit.runner=build-1 rbit-42"
        );
        let args = service_run(&names(), &ContainerConfig::default(), "postgres", "postgres:17", Path::new("/p/s.env"), &[]);
        assert_confined(&args);
        assert_eq!(
            args.join(" "),
            "run --detach --name rbit-42-postgres --network rbit-42 --network-alias postgres --label rbit.task=42 \
             --label rbit.runner=build-1 --security-opt no-new-privileges --cap-drop MKNOD --pids-limit 4096 --env-file /p/s.env postgres:17"
        );
        assert_eq!(remove(&["rbit-42-postgres".into(), "rbit-42-job".into()]).join(" "), "rm --force rbit-42-postgres rbit-42-job");
    }

    #[test]
    fn steps_exec_with_their_environment_in_a_file() {
        let step = StepCommand {
            program: "bash".into(),
            args: vec!["--noprofile".into(), "/__w/_temp/s.sh".into()],
            raw_args: None,
            env: BTreeMap::from([("SECRET".into(), "s3cr3t".into())]),
            cwd: "/__w/app/sub".into(),
        };
        let args = step_exec(&names(), &step, Path::new("/p/step.env"), &[]);
        assert_confined(&args);
        assert_eq!(args.join(" "), "exec --env-file /p/step.env --workdir /__w/app/sub rbit-42-job bash --noprofile /__w/_temp/s.sh");
        assert!(!args.iter().any(|arg| arg.contains("s3cr3t")));
    }

    #[test]
    fn env_files_are_private_and_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.env");
        let env = BTreeMap::from([("A".into(), "1".into()), ("KEY".into(), "line1\nline2".into()), ("B".into(), "x=y".into())]);
        let file = EnvFile::write(&path, &env).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "A=1\nB=x=y\n");
        assert_eq!(file.names(), ["KEY"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        drop(file);
        assert!(!path.exists());
    }

    #[test]
    fn nothing_of_the_job_reaches_the_engine_or_the_runners_variables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.env");
        let write = |name: &str, value: &str| EnvFile::write(&path, &BTreeMap::from([(name.to_owned(), value.to_owned())]));
        // A newline in a name would make a bare `NAME` line, which copies the runner's own
        // variable into the container; `=` or a space would change the line.
        for name in ["AWS_SECRET_ACCESS_KEY\nX", "A=B", "has space", "#comment", "", "1ST", "é"] {
            let error = write(name, "v").unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{name:?}");
            assert!(!path.exists(), "nothing is written for {name:?}");
        }
        // Multi-line values travel in the CLI's environment: never for what the CLI reads.
        for name in ["LD_PRELOAD", "ld_preload", "GCONV_PATH", "GLIBC_TUNABLES", "DOCKER_HOST", "PATH", "https_proxy", "XDG_RUNTIME_DIR"] {
            let error = write(name, "/work/evil.so:\n").unwrap_err();
            assert!(error.to_string().contains("container engine"), "{name}: {error}");
        }
        // Single-line values of those names only go into the file, to the container.
        let fine = write("PATH", "/usr/local/bin:/usr/bin").unwrap();
        assert_eq!(fine.names(), Vec::<String>::new());
        drop(fine);
        let key = write("SSH_KEY", "-----BEGIN-----\nabc\n-----END-----").unwrap();
        assert_eq!(key.names(), ["SSH_KEY"], "an ordinary multi-line value still works");
    }

    #[test]
    fn images_and_registries() {
        assert!(valid_image("ghcr.io/gitrbit/rbit-runner:1") && valid_image("alpine"));
        assert!(!valid_image("--privileged") && !valid_image("alpine sh") && !valid_image(""));
        assert_eq!(registry("ghcr.io/o/i:1"), "ghcr.io");
        assert_eq!(registry("localhost:5000/i"), "localhost:5000");
        assert_eq!(registry("library/ubuntu"), "docker.io");
        assert_eq!(registry("ubuntu:24.04"), "docker.io");
        assert_eq!(view("alice/_weird").workspace, "/__w/workspace");
    }
}
