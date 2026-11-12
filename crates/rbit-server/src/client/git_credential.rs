//! `rbit auth setup-git` and the credential helper it installs, so `git push` to an rbit
//! instance uses the stored login and never stops at a password prompt (agent plan 2.5).
//!
//! * `setup-git` sets `credential.<instance>.helper` to `!<this rbit> auth git-credential`
//!   in the user's global Git configuration, after clearing other helpers for that URL (so a
//!   desktop credential manager cannot pop up a dialog for it).
//! * `git-credential get` answers from `RBIT_TOKEN`, `GH_TOKEN` or the stored login for the
//!   host Git asks about. Without one it answers `quit=1`, which tells Git to stop instead of
//!   prompting: a missing login fails at once with a clear error. A token the server refuses
//!   fails the push the same way (`Authentication failed`); the helper never prompts.

use std::io::Write as _;

use anyhow::{Context as _, bail};

use super::{hosts, util::git};

/// `rbit auth setup-git --hostname URL`.
pub fn setup(hostname: &str) -> anyhow::Result<()> {
    let host = hosts::normalize(hostname);
    let exe = std::env::current_exe().context("cannot find the rbit executable")?;
    // Git runs `!command` through its shell; forward slashes work on every platform.
    let exe = exe.to_string_lossy().replace('\\', "/");
    let helper = format!("!\"{exe}\" auth git-credential");
    let key = format!("credential.{host}.helper");
    // An empty value first resets the helpers inherited from broader configuration.
    for value in ["", helper.as_str()] {
        let status =
            std::process::Command::new("git").args(["config", "--global", "--add", &key, value]).status().context("cannot run git")?;
        if !status.success() {
            bail!("git config --global --add {key} failed");
        }
    }
    let set = git(&["config", "--global", "--get-all", &key]).unwrap_or_default();
    eprintln!("Git now asks rbit for credentials for {host}:\n{set}");
    Ok(())
}

/// The fields Git sends on standard input (`key=value` lines, then a blank line).
fn read_request(input: impl std::io::BufRead) -> Vec<(String, String)> {
    input
        .lines()
        .map_while(Result::ok)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once('=').map(|(key, value)| (key.to_owned(), value.to_owned())))
        .collect()
}

/// The answer to `get` for a request naming `protocol` and `host`, given the token found.
fn answer(token: Option<&str>) -> String {
    match token {
        Some(token) => format!("username=x-access-token\npassword={token}\n"),
        // Stop here: no other helper, and no prompt.
        None => "quit=1\n".to_owned(),
    }
}

/// `rbit auth git-credential <get|store|erase>`.
pub fn credential(operation: &str) -> anyhow::Result<i32> {
    let request = read_request(std::io::stdin().lock());
    if operation != "get" {
        // Logins are stored by `rbit auth login`, not by Git; nothing to store or erase.
        return Ok(0);
    }
    let field = |name: &str| request.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
    let (Some(protocol), Some(host)) = (field("protocol"), field("host")) else {
        std::io::stdout().write_all(answer(None).as_bytes())?;
        return Ok(0);
    };
    let instance = hosts::normalize(&format!("{protocol}://{host}"));
    let from_env = std::env::var("RBIT_TOKEN").ok().or_else(|| std::env::var("GH_TOKEN").ok()).filter(|token| !token.trim().is_empty());
    let stored = hosts::Hosts::load().ok().and_then(|stored| stored.hosts.get(&instance).map(|login| login.token.clone()));
    let token = from_env.or(stored);
    if token.is_none() {
        eprintln!("rbit: no login for {instance}; run `rbit auth login --hostname {instance}`");
    }
    std::io::stdout().write_all(answer(token.as_deref()).as_bytes())?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_requests_are_read_and_answered_without_prompting() {
        let request = read_request("protocol=https\nhost=git.example.com\npath=a/b.git\n\nignored=1\n".as_bytes());
        assert_eq!(
            request,
            [("protocol".into(), "https".into()), ("host".into(), "git.example.com".into()), ("path".into(), "a/b.git".into())]
        );
        assert_eq!(answer(Some("rbit_abc")), "username=x-access-token\npassword=rbit_abc\n");
        assert_eq!(answer(None), "quit=1\n", "no login stops Git instead of prompting");
    }
}
