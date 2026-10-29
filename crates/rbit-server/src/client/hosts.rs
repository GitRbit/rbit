//! The logins of `rbit auth login`: one token per instance, in `hosts.toml`.

use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context as _, bail};
use serde::{Deserialize, Serialize};

/// Every stored login, by instance URL (`https://git.example.com`).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Hosts {
    #[serde(default)]
    pub hosts: BTreeMap<String, Login>,
    /// The instance commands use when neither a flag nor a remote names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Login {
    pub user: String,
    pub token: String,
}

/// `hosts.toml` in the user's configuration directory.
pub fn path() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("RBIT_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("hosts.toml"));
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    let Some(base) = base else { bail!("no configuration directory: set RBIT_CONFIG_DIR") };
    Ok(base.join("rbit").join("hosts.toml"))
}

impl Hosts {
    pub fn load() -> anyhow::Result<Self> {
        let path = path()?;
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("{} is not valid", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    /// Writes the file readable by the user only (on Unix), through a temporary file.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let text = toml::to_string(self)?;
        let temp = path.with_extension("toml.tmp");
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        {
            use std::io::Write as _;
            let mut file = options.open(&temp).with_context(|| format!("cannot write {}", temp.display()))?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&temp, &path).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    }
}

/// `url` as logins are keyed: scheme, host and port, no trailing slash.
pub fn normalize(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.contains("://") { url.to_owned() } else { format!("https://{url}") }
}
