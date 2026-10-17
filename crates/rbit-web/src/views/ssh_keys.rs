//! SSH key and deploy key pages.
//!
//! `/settings/keys` lists a user's keys; `/{owner}/{repo}/settings/keys` a repository's
//! deploy keys. Both show each key's fingerprint, type and dates, with an add form above.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::Timestamp;
use rbit_services::ssh_keys::{DeployKey, SshKey};

use crate::views::{
    components::FormError,
    layout::Page,
    repo::RepoHeader,
    settings::{SettingsLink, SettingsSection},
};

/// A human name for an SSH algorithm identifier.
pub fn key_type_label(algorithm: &str) -> &'static str {
    match algorithm {
        "ssh-ed25519" => "Ed25519",
        "ssh-rsa" => "RSA",
        "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521" => "ECDSA",
        "sk-ssh-ed25519@openssh.com" => "Ed25519 security key",
        "sk-ecdsa-sha2-nistp256@openssh.com" => "ECDSA security key",
        _ => "SSH key",
    }
}

/// One key in a list.
#[derive(Debug, Clone)]
pub struct KeyRow {
    pub title: String,
    /// `SHA256:…`, as `ssh-keygen -l` prints it.
    pub fingerprint: String,
    pub type_label: &'static str,
    /// e.g. `Added Oct 15, 2026`.
    pub added: String,
    pub added_iso: String,
    /// e.g. `Last used 3 hours ago` or `Never used`.
    pub last_used: String,
    /// Deploy keys only: `Read-only` or `Read and write`.
    pub access_label: Option<&'static str>,
    pub delete_url: String,
}

impl KeyRow {
    fn new(
        title: &str,
        fingerprint: &str,
        algorithm: &str,
        created_at: Timestamp,
        last_used_at: Option<Timestamp>,
        delete_url: String,
    ) -> Self {
        Self {
            title: title.to_owned(),
            fingerprint: fingerprint.to_owned(),
            type_label: key_type_label(algorithm),
            added: format!("Added {}", created_at.date()),
            added_iso: created_at.iso8601(),
            last_used: last_used_at.map_or_else(|| "Never used".to_owned(), |at| format!("Last used {}", at.relative())),
            access_label: None,
            delete_url,
        }
    }

    pub fn for_user_key(key: &SshKey) -> Self {
        Self::new(
            &key.title,
            &key.fingerprint,
            &key.algorithm,
            key.created_at,
            key.last_used_at,
            format!("/settings/keys/{}/delete", key.id.0),
        )
    }

    /// `repo_url` is `/{owner}/{repo}`.
    pub fn for_deploy_key(key: &DeployKey, repo_url: &str) -> Self {
        let delete_url = format!("{repo_url}/settings/keys/{}/delete", key.id.0);
        let mut row = Self::new(&key.title, &key.fingerprint, &key.algorithm, key.created_at, key.last_used_at, delete_url);
        row.access_label = Some(if key.read_only { "Read-only" } else { "Read and write" });
        row
    }
}

/// The add form's values (blank, or as submitted when re-rendered with an error).
#[derive(Debug, Clone, Default)]
pub struct KeyForm {
    pub title: String,
    pub key: String,
    /// Deploy keys only.
    pub allow_write: bool,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/keys.html")]
pub struct SshKeysPage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub keys: Vec<KeyRow>,
    pub form: KeyForm,
    pub error: FormError,
    /// Shown when the SSH server is enabled, e.g. `ssh -T -p 2222 git@example.com`.
    pub test_command: Option<String>,
}

impl SshKeysPage {
    pub fn new(page: Page, keys: &[SshKey], test_command: Option<String>) -> Self {
        Self {
            page: page.titled("SSH keys"),
            settings_nav: SettingsSection::SshKeys.nav(),
            keys: keys.iter().map(KeyRow::for_user_key).collect(),
            form: KeyForm::default(),
            error: FormError::default(),
            test_command,
        }
    }

    /// Re-renders with `error` above the form, which keeps what was submitted.
    #[must_use]
    pub fn rejected(mut self, form: KeyForm, error: FormError) -> Self {
        self.form = form;
        self.error = error;
        self
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/deploy_keys.html")]
pub struct DeployKeysPage {
    pub page: Page,
    pub header: RepoHeader,
    pub full_name: String,
    /// `/{owner}/{repo}/settings/keys`, where the add form posts.
    pub keys_url: String,
    pub settings_url: String,
    pub keys: Vec<KeyRow>,
    pub form: KeyForm,
    pub error: FormError,
}

impl DeployKeysPage {
    pub fn new(page: Page, header: RepoHeader, keys: &[DeployKey]) -> Self {
        let full_name = format!("{}/{}", header.owner, header.name);
        Self {
            page: page.titled(format!("Deploy keys · {full_name}")),
            keys_url: format!("{}/settings/keys", header.home_url),
            settings_url: header.settings_url.clone(),
            keys: keys.iter().map(|key| KeyRow::for_deploy_key(key, &header.home_url)).collect(),
            full_name,
            header,
            form: KeyForm::default(),
            error: FormError::default(),
        }
    }

    /// Re-renders with `error` above the form, which keeps what was submitted.
    #[must_use]
    pub fn rejected(mut self, form: KeyForm, error: FormError) -> Self {
        self.form = form;
        self.error = error;
        self
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{AccountId, RepoId};
    use rbit_services::ssh_keys::{DeployKeyId, SshKeyId};

    use super::*;

    const NOW: Timestamp = Timestamp(1_792_022_400); // 2026-10-15T00:00:00Z

    #[test]
    fn labels_key_types() {
        assert_eq!(key_type_label("ssh-ed25519"), "Ed25519");
        assert_eq!(key_type_label("ecdsa-sha2-nistp384"), "ECDSA");
        assert_eq!(key_type_label("sk-ssh-ed25519@openssh.com"), "Ed25519 security key");
        assert_eq!(key_type_label("something-new"), "SSH key");
    }

    #[test]
    fn rows_describe_user_and_deploy_keys() {
        let key = SshKey {
            id: SshKeyId(7),
            account_id: AccountId(1),
            title: "laptop".to_owned(),
            public_key: "ssh-ed25519 AAAA".to_owned(),
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:abc".to_owned(),
            created_at: NOW,
            last_used_at: None,
        };
        let row = KeyRow::for_user_key(&key);
        assert_eq!(row.added, "Added Oct 15, 2026");
        assert_eq!(row.last_used, "Never used");
        assert_eq!(row.delete_url, "/settings/keys/7/delete");
        assert_eq!(row.access_label, None);

        let deploy = DeployKey {
            id: DeployKeyId(3),
            repo_id: RepoId(2),
            title: "ci".to_owned(),
            public_key: "ssh-rsa AAAA".to_owned(),
            algorithm: "ssh-rsa".to_owned(),
            fingerprint: "SHA256:def".to_owned(),
            read_only: false,
            created_at: NOW,
            last_used_at: Some(NOW),
        };
        let row = KeyRow::for_deploy_key(&deploy, "/alice/proj");
        assert_eq!(row.type_label, "RSA");
        assert_eq!(row.access_label, Some("Read and write"));
        assert_eq!(row.delete_url, "/alice/proj/settings/keys/3/delete");
        assert!(row.last_used.starts_with("Last used"));
    }
}
