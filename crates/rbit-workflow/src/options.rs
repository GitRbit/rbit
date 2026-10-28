//! `container.options`, `services.<id>.options` and `volumes`: what of GitHub's container
//! options rbit passes on.
//!
//! GitHub hands `options` to `docker create` as written. A shared self-hosted runner cannot:
//! an option such as `--privileged`, `--cap-add`, `--device`, `--network host`, `-v /:/host`,
//! `--security-opt` or `--pid host` gives a job the host. [`docker_options`] passes the
//! options that only shape the container (user, host name, environment, health checks,
//! resources within the runner's limits, DNS, `tmpfs`, `init`, …) and refuses any other with
//! a readable error. The runner puts its own limits after them, so `--memory`, `--cpus` and
//! `--pids-limit` can only stay within the runner's.
//!
//! `volumes` takes named volumes only, `name:/path[:ro]`: a host path would reach the
//! runner's file system. Each name is the task's own (the runner prefixes it with the task),
//! shared by the job's container and services and removed with them, as on a hosted runner
//! where every job has a fresh machine.

/// Options that take a value, by every spelling.
const WITH_VALUE: &[&[&str]] = &[
    &["--user", "-u"],
    &["--hostname", "-h"],
    &["--env", "-e"],
    &["--workdir", "-w"],
    &["--cpus"],
    &["--memory", "-m"],
    &["--memory-swap"],
    &["--memory-reservation"],
    &["--shm-size"],
    &["--ulimit"],
    &["--add-host"],
    &["--dns"],
    &["--dns-search"],
    &["--dns-option"],
    &["--label", "-l"],
    &["--health-cmd"],
    &["--health-interval"],
    &["--health-timeout"],
    &["--health-retries"],
    &["--health-start-period"],
    &["--health-start-interval"],
    &["--tmpfs"],
    &["--stop-signal"],
    &["--stop-timeout"],
    &["--cpu-shares", "-c"],
    &["--cpuset-cpus"],
    &["--pids-limit"],
    &["--platform"],
    &["--entrypoint"],
];
/// Options that take none.
const FLAGS: &[&str] = &["--init", "--read-only", "--no-healthcheck"];

/// The arguments `text` (one container's `options`) stands for, each option checked; for a
/// job's own container (`service: false`) also without `--entrypoint`, which the runner
/// sets, as on GitHub.
pub fn docker_options(text: &str, service: bool) -> Result<Vec<String>, String> {
    normalize(split(text)?, service)
}

/// Whether `args` (options as [`docker_options`] gives them) are all allowed: the runner
/// checks what the server sends.
pub fn check(args: &[String], service: bool) -> Result<(), String> {
    normalize(args.to_vec(), service).map(drop)
}

/// `words` with each option checked and spelled in full, values apart.
fn normalize(words: Vec<String>, service: bool) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut words = words.into_iter();
    while let Some(word) = words.next() {
        if FLAGS.contains(&word.as_str()) {
            args.push(word);
            continue;
        }
        let (flag, inline) = match word.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_owned(), Some(value.to_owned())),
            _ => (word.clone(), None),
        };
        let Some(spellings) = WITH_VALUE.iter().find(|spellings| spellings.contains(&flag.as_str())) else {
            return Err(refused(&flag));
        };
        let name = spellings[0];
        if name == "--entrypoint" && !service {
            return Err("`--entrypoint` is not supported for the job's container: the runner keeps it running".to_owned());
        }
        let value = match inline {
            Some(value) => value,
            None => words.next().ok_or_else(|| format!("`{flag}` needs a value"))?,
        };
        if name == "--label" && value.trim_start().to_ascii_lowercase().starts_with("rbit.") {
            return Err("`rbit.` labels are the runner's own".to_owned());
        }
        args.push(name.to_owned());
        args.push(value);
    }
    Ok(args)
}

fn refused(flag: &str) -> String {
    if flag.starts_with('-') {
        format!(
            "`{flag}` is not an option rbit passes: it could give the job the runner host (allowed are the user, host name, \
             environment, health checks, resource limits, DNS, labels, tmpfs, init and stop options)"
        )
    } else {
        format!("`{flag}` is not an option")
    }
}

/// A named volume of `volumes`: its name, where it is mounted, and whether read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub target: String,
    pub read_only: bool,
}

/// `name:/path` or `name:/path:ro`; a host path, or anything that is not a plain name and
/// an absolute path, is refused.
pub fn volume(text: &str) -> Result<Volume, String> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let (name, target, read_only) = match parts.as_slice() {
        [name, target] | [name, target, "rw"] => (*name, *target, false),
        [name, target, "ro"] => (*name, *target, true),
        _ => return Err(format!("`{text}` must be `name:/path`, with `:ro` to mount it read-only")),
    };
    if name.starts_with(['/', '.', '~']) || name.contains(['\\', '/']) {
        return Err(format!("`{name}` is a host path: only named volumes are allowed, which the job's containers share"));
    }
    if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')) || name.len() > 64 {
        return Err(format!("`{name}` is not a volume name"));
    }
    if !target.starts_with('/') || target.contains([',', '\n']) || target.split('/').any(|part| part == "..") {
        return Err(format!("`{target}` must be an absolute path in the container"));
    }
    Ok(Volume { name: name.to_owned(), target: target.to_owned(), read_only })
}

/// `text` split as a shell splits words: spaces separate, `'…'` and `"…"` quote, `\` escapes
/// outside single quotes. Also how a custom `shell` (`perl {0}`) is read.
pub fn split(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err("an unclosed `'` in `options`".to_owned()),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => word.push(chars.next().ok_or("a `\\` at the end of `options`")?),
                        Some(c) => word.push(c),
                        None => return Err("an unclosed `\"` in `options`".to_owned()),
                    }
                }
            }
            '\\' => {
                started = true;
                word.push(chars.next().ok_or("a `\\` at the end of `options`")?);
            }
            c if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                started = true;
                word.push(c);
            }
        }
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaping_options_pass_and_those_that_give_the_host_do_not() {
        let args = docker_options(
            "--user 1000:1000 --hostname=buildbox -e FROM=\"two words\" --health-cmd \"redis-cli ping\" --health-retries 5 --init --memory 1g",
            false,
        )
        .unwrap();
        assert_eq!(
            args,
            [
                "--user",
                "1000:1000",
                "--hostname",
                "buildbox",
                "--env",
                "FROM=two words",
                "--health-cmd",
                "redis-cli ping",
                "--health-retries",
                "5",
                "--init",
                "--memory",
                "1g"
            ]
        );
        for bad in [
            "--privileged",
            "--cap-add SYS_ADMIN",
            "--device /dev/kvm",
            "--network host",
            "--net=host",
            "-v /:/host",
            "--volume /:/host",
            "--mount type=bind,source=/,target=/host",
            "--security-opt seccomp=unconfined",
            "--pid host",
            "--ipc=host",
            "--userns host",
            "--cgroupns host",
            "--sysctl net.ipv4.ip_forward=1",
            "--runtime runc",
            "--label rbit.task=1",
        ] {
            assert!(docker_options(bad, true).is_err(), "{bad}");
        }
        assert!(docker_options("--entrypoint sh", false).is_err(), "the job container's entrypoint is the runner's");
        assert_eq!(docker_options("--entrypoint sh", true).unwrap(), ["--entrypoint", "sh"]);
        assert!(docker_options("--user", false).is_err(), "a value is needed");
        assert!(docker_options("--env 'unclosed", false).is_err());
    }

    #[test]
    fn only_named_volumes() {
        assert_eq!(volume("data:/var/lib/data").unwrap(), Volume { name: "data".into(), target: "/var/lib/data".into(), read_only: false });
        assert!(volume("cache:/cache:ro").unwrap().read_only);
        for bad in ["/etc:/x", "./x:/x", "~/x:/x", "C:\\x:/x", "data", "data:relative", "data:/a/../b", "da ta:/x", "data:/x:z"] {
            assert!(volume(bad).is_err(), "{bad}");
        }
    }
}
