//! SSH public keys for users and deploy keys for repositories.
//!
//! Keys are pasted in OpenSSH format (`ssh-ed25519 AAAA… comment`). They are parsed and
//! validated here once, then stored normalised (algorithm + key data, no comment) with
//! their SHA256 fingerprint. At SSH sign-in the server proves the client holds a private
//! key and looks the fingerprint up with [`SshKeyService::find_by_fingerprint`], which
//! resolves to a user (refused while suspended) or a deploy key (one repository,
//! read-only unless allowed to write).
//!
//! Accepted: Ed25519, ECDSA (P-256/384/521), RSA of at least 2048 bits, and FIDO security
//! keys (`sk-ssh-ed25519@openssh.com`, `sk-ecdsa-sha2-nistp256@openssh.com`). DSA,
//! certificates and unknown algorithms are refused.

use rbit_core::{RepoAccess, RepoId, Timestamp, User, repository::OwnedRepository};
/// The stored keys, re-exported so callers need not depend on the storage layer.
pub use rbit_storage::sqlite::ssh_keys::{DeployKey, DeployKeyId, SshKey, SshKeyId};
use rbit_storage::{
    StorageError,
    sqlite::ssh_keys::{KeyHolder, NewPublicKey, SshKeyStore},
};
use ssh_key::{HashAlg, PublicKey, public::KeyData};

use crate::{RepoContext, ServiceContext, ServiceError};

/// RSA keys shorter than this are refused (NIST deprecated 1024-bit RSA long ago).
pub const MIN_RSA_BITS: u32 = 2048;
const MAX_TITLE_LEN: usize = 100;
/// A pasted key, comment included; real keys (even RSA 16384) are far shorter.
const MAX_KEY_LEN: usize = 16 * 1024;
const MAX_KEYS_PER_USER: i64 = 100;
const MAX_DEPLOY_KEYS_PER_REPO: i64 = 50;

const DSA_UNSUPPORTED: &str = "DSA keys are insecure and not supported; use an Ed25519 key";
const DUPLICATE_KEY: &str = "this key is already in use; a key can belong to only one account or repository";

/// Raw "add key" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct AddSshKey {
    pub title: String,
    pub key: String,
}

/// Raw "add deploy key" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct AddDeployKey {
    pub title: String,
    pub key: String,
    /// Allow pushing with this key (read-only otherwise).
    pub allow_write: bool,
}

/// A validated public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPublicKey {
    /// `algorithm base64`, without the comment.
    pub openssh: String,
    pub algorithm: String,
    /// `SHA256:…`, as printed by `ssh-keygen -l`.
    pub fingerprint: String,
    /// The key's trailing comment (often `user@host`); may be empty.
    pub comment: String,
}

/// Parses and validates one OpenSSH public key line.
pub fn parse_public_key(input: &str) -> Result<ParsedPublicKey, ServiceError> {
    let invalid = |message: &str| Err(ServiceError::Invalid(message.to_owned()));
    let input = input.trim();
    if input.is_empty() {
        return invalid("paste a public key");
    }
    if input.len() > MAX_KEY_LEN {
        return invalid("that is too long to be a public key");
    }
    if input.contains("PRIVATE KEY") {
        return invalid("that is a private key; never share it. Paste the matching public key (the .pub file) instead");
    }
    if input.lines().count() > 1 {
        return invalid("paste exactly one key, on a single line");
    }
    if input.starts_with("ssh-dss ") {
        return invalid(DSA_UNSUPPORTED);
    }

    let Ok(mut key) = PublicKey::from_openssh(input) else {
        return invalid("that is not a valid OpenSSH public key; it should start with ssh-ed25519, ecdsa-sha2-…, ssh-rsa or sk-…");
    };
    match key.key_data() {
        KeyData::Ed25519(_) | KeyData::Ecdsa(_) | KeyData::SkEd25519(_) | KeyData::SkEcdsaSha2NistP256(_) => {}
        KeyData::Rsa(rsa) if rsa.key_size() >= MIN_RSA_BITS => {}
        KeyData::Rsa(_) => return Err(ServiceError::Invalid(format!("RSA keys must be at least {MIN_RSA_BITS} bits"))),
        KeyData::Dsa(_) => return invalid(DSA_UNSUPPORTED),
        KeyData::Certificate(_) => return invalid("certificates are not supported; paste a plain public key"),
        _ => return invalid("this key type is not supported; use Ed25519, ECDSA, RSA (2048 bits or more) or a security key"),
    }

    let comment = key.comment().as_str_lossy().trim().to_owned();
    key.set_comment("");
    let openssh = key.to_openssh().map_err(ServiceError::internal)?.trim().to_owned();
    Ok(ParsedPublicKey {
        openssh,
        algorithm: key.algorithm().as_str().to_owned(),
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        comment,
    })
}

/// The SHA256 fingerprint of a key the SSH server has just verified.
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Who an SSH key authenticates.
#[derive(Debug, Clone)]
pub enum SshIdentity {
    /// A user's personal key: the user's own permissions apply.
    User { key: SshKeyId, user: User },
    /// A deploy key: one repository, read-only unless `read_only` is false.
    Deploy { key: DeployKeyId, repo: OwnedRepository, read_only: bool },
}

#[derive(Debug, Clone)]
pub struct SshKeyService {
    ctx: ServiceContext,
    store: SshKeyStore,
}

impl SshKeyService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let store = ctx.database.ssh_keys();
        Self { ctx, store }
    }

    // -------------------------------------------------------------- user keys

    pub async fn add_user_key(&self, owner: &User, input: AddSshKey) -> Result<SshKey, ServiceError> {
        let parsed = parse_public_key(&input.key)?;
        let title = title(&input.title, &parsed)?;
        if self.store.count_user_keys(owner.id()).await? >= MAX_KEYS_PER_USER {
            return Err(ServiceError::Invalid(format!("you can have at most {MAX_KEYS_PER_USER} SSH keys; delete one you no longer use")));
        }
        let key = self.store.add_user_key(owner.id(), &new_key(title, parsed)).await.map_err(duplicate)?;
        tracing::info!(user = %owner.name(), key = key.id.0, fingerprint = %key.fingerprint, "SSH key added");
        Ok(key)
    }

    /// The user's keys, newest first.
    pub async fn list_user_keys(&self, owner: &User) -> Result<Vec<SshKey>, ServiceError> {
        Ok(self.store.list_user_keys(owner.id()).await?)
    }

    /// [`ServiceError::NotFound`] if the key doesn't exist or isn't the owner's.
    pub async fn delete_user_key(&self, owner: &User, id: SshKeyId) -> Result<(), ServiceError> {
        if !self.store.delete_user_key(owner.id(), id).await? {
            return Err(ServiceError::NotFound);
        }
        tracing::info!(user = %owner.name(), key = id.0, "SSH key deleted");
        Ok(())
    }

    // -------------------------------------------------------------- deploy keys

    /// Repository administrators only; `repo` must have been opened for the actor.
    pub async fn add_deploy_key(&self, actor: &User, repo: &RepoContext, input: AddDeployKey) -> Result<DeployKey, ServiceError> {
        require_admin(repo)?;
        let parsed = parse_public_key(&input.key)?;
        let title = title(&input.title, &parsed)?;
        let repo_id = repo.repo.repo.id;
        if self.store.count_deploy_keys(repo_id).await? >= MAX_DEPLOY_KEYS_PER_REPO {
            return Err(ServiceError::Invalid(format!(
                "a repository can have at most {MAX_DEPLOY_KEYS_PER_REPO} deploy keys; delete one you no longer use"
            )));
        }
        let key = self.store.add_deploy_key(repo_id, &new_key(title, parsed), !input.allow_write).await.map_err(duplicate)?;
        tracing::info!(actor = %actor.name(), repo = %repo.repo.full_name(), key = key.id.0, read_only = key.read_only, "deploy key added");
        Ok(key)
    }

    /// Repository administrators only.
    pub async fn list_deploy_keys(&self, repo: &RepoContext) -> Result<Vec<DeployKey>, ServiceError> {
        require_admin(repo)?;
        Ok(self.store.list_deploy_keys(repo.repo.repo.id).await?)
    }

    /// Repository administrators only. [`ServiceError::NotFound`] if the key isn't this repository's.
    pub async fn delete_deploy_key(&self, actor: &User, repo: &RepoContext, id: DeployKeyId) -> Result<(), ServiceError> {
        require_admin(repo)?;
        if !self.store.delete_deploy_key(repo.repo.repo.id, id).await? {
            return Err(ServiceError::NotFound);
        }
        tracing::info!(actor = %actor.name(), repo = %repo.repo.full_name(), key = id.0, "deploy key deleted");
        Ok(())
    }

    // -------------------------------------------------------------- SSH sign-in

    /// Resolves a verified key's `SHA256:…` fingerprint. `Ok(None)` for unknown keys and for
    /// keys of suspended users.
    pub async fn find_by_fingerprint(&self, fingerprint: &str) -> Result<Option<SshIdentity>, ServiceError> {
        Ok(match self.store.find_by_fingerprint(fingerprint).await? {
            None => None,
            Some(KeyHolder::User { user, suspended: true, .. }) => {
                tracing::info!(user = %user.name(), "suspended user's SSH key refused");
                None
            }
            Some(KeyHolder::User { key, user, suspended: false }) => Some(SshIdentity::User { key: key.id, user }),
            Some(KeyHolder::Deploy { key, repo }) => Some(SshIdentity::Deploy { key: key.id, repo, read_only: key.read_only }),
        })
    }

    /// Records that `identity`'s key was just used. Bookkeeping only: failures are logged,
    /// never surfaced, so a busy database cannot turn a valid key into a refusal.
    pub async fn record_use(&self, identity: &SshIdentity) {
        let now = Timestamp::now();
        let result = match identity {
            SshIdentity::User { key, .. } => self.store.touch_user_key(*key, now).await,
            SshIdentity::Deploy { key, .. } => self.store.touch_deploy_key(*key, now).await,
        };
        if let Err(error) = result {
            tracing::warn!(%error, "failed to record SSH key use");
        }
    }

    /// Opens `owner/name` for a deploy key. The key reaches only its own repository
    /// (anything else is [`ServiceError::NotFound`]) with read access, or write access
    /// when it was added with write permission. Like the user path
    /// ([`crate::RepositoryService::open_following_renames`], exactly as HTTP), an old
    /// name of the repository from before a rename or transfer still reaches it.
    pub async fn open_for_deploy_key(&self, identity: &SshIdentity, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        let SshIdentity::Deploy { repo: key_repo, read_only, .. } = identity else {
            return Err(ServiceError::NotFound);
        };
        let name = name.strip_suffix(".git").unwrap_or(name);
        let store = self.ctx.database.repositories();
        let repo = match store.find_by_path(owner, name).await? {
            Some(repo) => repo,
            None => store.find_by_old_path(owner, name).await?.ok_or(ServiceError::NotFound)?,
        };
        if repo.repo.id != key_repo.repo.id {
            return Err(ServiceError::NotFound);
        }
        let access = if *read_only { RepoAccess::Read } else { RepoAccess::Write };
        let disk_path = self.ctx.repositories.path_for(repo.repo.owner_id, repo.repo.id);
        let reader = self.ctx.repositories.reader(repo.repo.owner_id, repo.repo.id);
        Ok(RepoContext { repo, access, reader, disk_path })
    }

    /// The repository a deploy key belongs to.
    pub fn deploy_key_repo(identity: &SshIdentity) -> Option<RepoId> {
        match identity {
            SshIdentity::Deploy { repo, .. } => Some(repo.repo.id),
            SshIdentity::User { .. } => None,
        }
    }
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::NotFound) }
}

/// The submitted title, or the key's comment when the title was left blank.
fn title(input: &str, parsed: &ParsedPublicKey) -> Result<String, ServiceError> {
    let title = match input.trim() {
        "" => parsed.comment.as_str(),
        title => title,
    };
    if title.is_empty() {
        return Err(ServiceError::Invalid("give the key a title, e.g. “Work laptop”".to_owned()));
    }
    if title.chars().count() > MAX_TITLE_LEN {
        return Err(ServiceError::Invalid(format!("the title must be at most {MAX_TITLE_LEN} characters")));
    }
    Ok(title.to_owned())
}

fn new_key(title: String, parsed: ParsedPublicKey) -> NewPublicKey {
    NewPublicKey { title, public_key: parsed.openssh, algorithm: parsed.algorithm, fingerprint: parsed.fingerprint }
}

fn duplicate(error: StorageError) -> ServiceError {
    match error {
        StorageError::Conflict(_) => ServiceError::Invalid(DUPLICATE_KEY.to_owned()),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated with `ssh-keygen`; the security key and DSA key are synthetic encodings.
    const ED25519: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAU5O8WQTG+ptksHlmxF+ieJyp0kwsQ2tQsz0JAuTWme alice@laptop";
    const ED25519_FINGERPRINT: &str = "SHA256:6yxaAtSseRpGJPcID+R+91fKBqIqIfu/nAYq0PCjYyo";
    const ECDSA_P384: &str = "ecdsa-sha2-nistp384 AAAAE2VjZHNhLXNoYTItbmlzdHAzODQAAAAIbmlzdHAzODQAAABhBBDm0of6pYEAGG6jji/oSTEpwsEDCpzGpDmOsiJva1yQkC2F2rMGrGigfIo1MCspImcIzYnw4qRZLtZmzfLQ6P4qk0JAoXxjlNIz659gdAPGSOTPxljAfVHcz/Y1HVfzkA== ci";
    const RSA_2048: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDYJlbDVEomXZhfyLQCZxDihVO7ip/qHc8utweQVxc4Ue0lPyKfCioraCnNdSWQjKQuj+Wcqmr51LwtaVYEIr2XHNej2eRKwNxhefatYs583m8oDVo/wyyPaHTImqYrRP7b5ZMulKRzm8YBxtryBrAO33yvh0vWrqj6TepKX1pQHH01aHSTPnK7Ep/mFBJrUrYYaoB3F2fvEIfKSWp0v0JAjGWWUaQA0kRsOWSTG3rJt1vMriqB/RqBp9J/urMbIO4IKI6S2st6XZxyfnk7lLyCgqWRCNtRUUbqbezklxOzZel3QYx7aCYGb0jrwbYlhSSTLWwTTISwWAvI++4rAl1J rsa2048";
    const RSA_1024: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAAgQDKo7hP2GR3/vV6v8V5PC/KMHHgZL5vpcn+9NlAmDmZKTRY/nE/f1pmCDPAlLXwQeo8tLPMDb1VlDY9W/GXu47lj0t0icaKRxGXw8mw/OmKZgrpsRXKPDi61GfLb/LnC2FvG0KoTTV7ZwGk8Ozihok+gdA8Yp6pm4RNTSQ+01WEtw== rsa1024";
    const SK_ED25519: &str = "sk-ssh-ed25519@openssh.com AAAAGnNrLXNzaC1lZDI1NTE5QG9wZW5zc2guY29tAAAAIAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gAAAABHNzaDo= yubikey";
    const DSA: &str = "ssh-dss AAAAB3NzaC1kc3MAAAAUAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAQAAAAECAAAAAQM=";

    #[test]
    fn parses_and_normalises_an_ed25519_key() {
        let parsed = parse_public_key(&format!("  {ED25519}\n")).unwrap();
        assert_eq!(parsed.algorithm, "ssh-ed25519");
        assert_eq!(parsed.openssh, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAU5O8WQTG+ptksHlmxF+ieJyp0kwsQ2tQsz0JAuTWme");
        assert_eq!(parsed.comment, "alice@laptop");
        assert_eq!(parsed.fingerprint, ED25519_FINGERPRINT, "matches `ssh-keygen -l`");
        // The comment is not part of the key's identity.
        assert_eq!(parse_public_key(&parsed.openssh).unwrap().fingerprint, parsed.fingerprint);
    }

    #[test]
    fn accepts_ecdsa_rsa_and_security_keys() {
        assert_eq!(parse_public_key(ECDSA_P384).unwrap().algorithm, "ecdsa-sha2-nistp384");
        assert_eq!(parse_public_key(RSA_2048).unwrap().algorithm, "ssh-rsa");
        assert_eq!(parse_public_key(SK_ED25519).unwrap().algorithm, "sk-ssh-ed25519@openssh.com");
    }

    #[test]
    fn refuses_weak_and_unsupported_keys() {
        assert!(matches!(parse_public_key(RSA_1024), Err(ServiceError::Invalid(message)) if message.contains("2048")));
        assert!(matches!(parse_public_key(DSA), Err(ServiceError::Invalid(message)) if message.contains("DSA")));
    }

    #[test]
    fn refuses_garbage_private_keys_and_multiple_keys() {
        for input in ["", "hello", "ssh-ed25519 notbase64", "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----"] {
            assert!(matches!(parse_public_key(input), Err(ServiceError::Invalid(_))), "{input:?}");
        }
        let two = format!("{ED25519}\n{ED25519}");
        assert!(matches!(parse_public_key(&two), Err(ServiceError::Invalid(message)) if message.contains("exactly one")));
        let authorized_keys_options = format!("command=\"/bin/sh\" {ED25519}");
        assert!(parse_public_key(&authorized_keys_options).is_err());
    }

    #[test]
    fn title_falls_back_to_the_comment() {
        let parsed = parse_public_key(ED25519).unwrap();
        assert_eq!(title("  ", &parsed).unwrap(), "alice@laptop");
        assert_eq!(title(" Work ", &parsed).unwrap(), "Work");
        let anonymous = parse_public_key(&parsed.openssh).unwrap();
        assert!(title("", &anonymous).is_err());
        assert!(title(&"x".repeat(MAX_TITLE_LEN + 1), &parsed).is_err());
    }

    // -------------------------------------------------------------- against real stores

    use std::sync::Arc;

    use rbit_core::{Config, Visibility};
    use rbit_storage::{Database, KvStore, NewUser};

    use crate::{CreateRepository, Services};

    struct Fixture {
        _dir: tempfile::TempDir,
        database: Database,
        services: Services,
    }

    impl Fixture {
        async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
            let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
            let mut config = Config::default();
            config.storage.data_dir = dir.path().to_path_buf();
            let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
            Self { _dir: dir, database, services }
        }

        async fn user(&self, name: &str) -> User {
            let new = NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin: false,
            };
            self.database.accounts().create_user(new).await.unwrap()
        }

        async fn repo(&self, owner: &User, name: &str) -> RepoContext {
            let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility: Visibility::Private };
            self.services.repositories.create(owner, input).await.unwrap();
            self.services.repositories.open(Some(owner), owner.name().as_str(), name).await.unwrap()
        }
    }

    fn add(title: &str, key: &str) -> AddSshKey {
        AddSshKey { title: title.to_owned(), key: key.to_owned() }
    }

    #[tokio::test]
    async fn user_keys_authenticate_their_owner_until_suspended() {
        let fixture = Fixture::new().await;
        let keys = &fixture.services.ssh_keys;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;

        let key = keys.add_user_key(&alice, add("", ED25519)).await.unwrap();
        assert_eq!(key.title, "alice@laptop");
        assert_eq!(key.fingerprint, ED25519_FINGERPRINT);

        let duplicate = keys.add_user_key(&bob, add("mine", ED25519)).await;
        assert!(matches!(duplicate, Err(ServiceError::Invalid(message)) if message.contains("already in use")));

        let identity = keys.find_by_fingerprint(ED25519_FINGERPRINT).await.unwrap().unwrap();
        assert!(matches!(&identity, SshIdentity::User { user, .. } if user.id() == alice.id()));
        keys.record_use(&identity).await;
        assert!(keys.list_user_keys(&alice).await.unwrap()[0].last_used_at.is_some());

        fixture.database.admin().set_suspended(alice.id(), true).await.unwrap();
        assert!(keys.find_by_fingerprint(ED25519_FINGERPRINT).await.unwrap().is_none(), "suspended users are refused");

        assert!(matches!(keys.delete_user_key(&bob, key.id).await, Err(ServiceError::NotFound)));
        keys.delete_user_key(&alice, key.id).await.unwrap();
        assert_eq!(keys.list_user_keys(&alice).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn deploy_keys_reach_only_their_repository() {
        let fixture = Fixture::new().await;
        let keys = &fixture.services.ssh_keys;
        let alice = fixture.user("alice").await;
        let project = fixture.repo(&alice, "project").await;
        let other = fixture.repo(&alice, "other").await;

        let read_only = AddDeployKey { title: "ci".to_owned(), key: ECDSA_P384.to_owned(), allow_write: false };
        let added = keys.add_deploy_key(&alice, &project, read_only).await.unwrap();
        assert!(added.read_only);
        let writer = AddDeployKey { title: "deploy".to_owned(), key: RSA_2048.to_owned(), allow_write: true };
        keys.add_deploy_key(&alice, &project, writer).await.unwrap();
        assert_eq!(keys.list_deploy_keys(&project).await.unwrap().len(), 2);

        let identity = keys.find_by_fingerprint(&added.fingerprint).await.unwrap().unwrap();
        assert_eq!(SshKeyService::deploy_key_repo(&identity), Some(project.repo.repo.id));
        let opened = keys.open_for_deploy_key(&identity, "alice", "project.git").await.unwrap();
        assert_eq!(opened.access, RepoAccess::Read);
        assert_eq!(opened.disk_path, project.disk_path);
        assert!(matches!(keys.open_for_deploy_key(&identity, "alice", "other").await, Err(ServiceError::NotFound)));
        assert!(matches!(keys.open_for_deploy_key(&identity, "alice", "missing").await, Err(ServiceError::NotFound)));

        let writer = keys.find_by_fingerprint(&parse_public_key(RSA_2048).unwrap().fingerprint).await.unwrap().unwrap();
        assert_eq!(keys.open_for_deploy_key(&writer, "ALICE", "Project").await.unwrap().access, RepoAccess::Write);

        assert!(matches!(keys.delete_deploy_key(&alice, &other, added.id).await, Err(ServiceError::NotFound)));
        keys.delete_deploy_key(&alice, &project, added.id).await.unwrap();
        assert_eq!(keys.list_deploy_keys(&project).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn only_repository_administrators_manage_deploy_keys() {
        let fixture = Fixture::new().await;
        let keys = &fixture.services.ssh_keys;
        let alice = fixture.user("alice").await;
        let mut project = fixture.repo(&alice, "project").await;
        project.access = RepoAccess::Write;

        let input = AddDeployKey { title: "ci".to_owned(), key: ED25519.to_owned(), allow_write: false };
        assert!(matches!(keys.add_deploy_key(&alice, &project, input).await, Err(ServiceError::NotFound)));
        assert!(matches!(keys.list_deploy_keys(&project).await, Err(ServiceError::NotFound)));
    }
}
