//! Secrets and variables: the storage, file and audit adapter of `rbit_actions::secrets`,
//! which decides the key file, sealing, value and scope rules and what a task opens
//! (§6.3 and §11.10 of `docs/design/actions.md`).
//!
//! * The key file lives at `<data_dir>/actions/secrets.key`, never in SQLite. This adapter
//!   reads it, creates it with mode `0600` and, on Unix, refuses a key readable by group or
//!   others; on Windows the data directory's ACLs are the operator's job, and a warning says
//!   so once.
//! * [`SecretsKey`] adapts the sealing key to the stored records: a scope and id become
//!   the ciphertext's address, and its parts a [`SealedSecret`].
//! * Values never reach a log, an audit entry or `Debug` output. Settings list names only.
//! * A task of an untrusted run receives no secrets and no variables (`Grants`). A trusted
//!   task receives the repository's, then the owner's visible to the repository, opened by
//!   `rbit_actions::secrets::open_task_secrets`.
//!
//! Workstream U (interface and secrets) owns this file.

use std::{collections::BTreeMap, io, path::Path, sync::Arc};

use rbit_actions::secrets::{KeyFile, LoadedKey, SecretAddress};
use rbit_core::{
    RepoId, Timestamp, User,
    actions::{Run, SealedSecret, SecretId, SecretInfo, SecretName, SecretScope, SecretVisibility, Variable, VariableName},
};
use zeroize::Zeroizing;

use super::ActionsService;
use crate::{
    RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    orgs::OrgContext,
};

// The key, sealing, value limits, scoping and opening rules are `rbit_actions::secrets`.
#[cfg(test)]
use rbit_actions::secrets::KEY_PREFIX;
pub use rbit_actions::secrets::{KeyUnavailable, MAX_PER_SCOPE, MAX_VALUE_BYTES, MAX_VARIABLE_BYTES, UNREADABLE};

/// The secrets key over the stored records (`rbit_actions::secrets::SecretsKey`).
pub(crate) struct SecretsKey {
    key: rbit_actions::secrets::SecretsKey,
    id: [u8; 8],
}

impl std::fmt::Debug for SecretsKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.key.fmt(f)
    }
}

/// The state of the key once it was looked at (see [`ActionsService::secrets_key`]).
pub(crate) type KeyState = Result<Arc<SecretsKey>, KeyUnavailable>;

impl SecretsKey {
    fn new(key: rbit_actions::secrets::SecretsKey) -> Self {
        Self { id: key.id(), key }
    }

    #[cfg(test)]
    fn from_bytes(bytes: [u8; 32]) -> Self {
        Self::new(rbit_actions::secrets::SecretsKey::from_bytes(bytes))
    }

    #[cfg(test)]
    fn parse(text: &str) -> Result<Self, String> {
        rbit_actions::secrets::SecretsKey::parse(text).map(Self::new)
    }

    #[cfg(test)]
    fn encode(&self) -> Zeroizing<String> {
        self.key.encode()
    }

    /// Seals `value` as secret `id` of `scope`.
    pub(crate) fn seal(&self, scope: SecretScope, id: SecretId, value: &str) -> Result<SealedSecret, ServiceError> {
        let (nonce, ciphertext) = self.key.seal(address(scope, id), value).map_err(ServiceError::internal)?;
        Ok(SealedSecret { key_id: self.id, nonce, ciphertext })
    }

    /// Opens secret `id` of `scope`, or `None` when another key sealed it or the
    /// ciphertext does not belong to this scope and id.
    pub(crate) fn open(&self, scope: SecretScope, id: SecretId, sealed: &SealedSecret) -> Option<Zeroizing<String>> {
        self.key.open(address(scope, id), sealed.key_id, sealed.nonce, &sealed.ciphertext)
    }

    /// Whether this key sealed a secret with `key_id`.
    pub(crate) fn sealed(&self, key_id: [u8; 8]) -> bool {
        self.key.sealed(key_id)
    }
}

/// Where secret `id` of `scope` is stored: the associated data of its ciphertext.
fn address(scope: SecretScope, id: SecretId) -> SecretAddress<'static> {
    SecretAddress { scope_kind: scope.kind(), scope_id: scope.id(), secret_id: id.0 }
}

/// Loads the key at `path`, or creates it when there is none and `sealed_exist` says no
/// secret was sealed yet (`rbit_actions::secrets::load_key`).
pub(crate) async fn load_key(path: &Path, sealed_exist: impl Future<Output = Result<bool, ServiceError>>) -> KeyState {
    match rbit_actions::secrets::load_key(&DiskKeyFile(path), sealed_exist).await? {
        LoadedKey::Existing(key) => Ok(Arc::new(SecretsKey::new(key))),
        LoadedKey::Created(key) => {
            tracing::info!(path = %path.display(), "created the Actions secrets key; back it up separately from the data directory");
            Ok(Arc::new(SecretsKey::new(key)))
        }
    }
}

/// The key file on disk.
struct DiskKeyFile<'a>(&'a Path);

impl KeyFile for DiskKeyFile<'_> {
    fn shown(&self) -> String {
        self.0.display().to_string()
    }

    async fn read(&self) -> io::Result<Option<Zeroizing<String>>> {
        match tokio::fs::read_to_string(self.0).await {
            Ok(text) => Ok(Some(Zeroizing::new(text))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn check_private(&self) -> Result<(), KeyUnavailable> {
        check_mode(self.0)
    }

    async fn create(&self, contents: &str) -> io::Result<()> {
        write_key(self.0, contents).await
    }
}

/// Writes a new key file, failing if one appeared meanwhile; `0600` on Unix.
async fn write_key(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).await?;
    tokio::io::AsyncWriteExt::write_all(&mut file, contents.as_bytes()).await?;
    file.sync_all().await
}

/// Refuses a key file group or others may read (Unix).
#[cfg(unix)]
fn check_mode(path: &Path) -> Result<(), KeyUnavailable> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path).map_err(|error| KeyUnavailable(format!("reading {} failed: {error}", path.display())))?;
    let mode = metadata.permissions().mode();
    // The group's and others' read, write and execute bits.
    let shared = mode & 0o077;
    if shared == 0 {
        Ok(())
    } else {
        Err(KeyUnavailable(format!(
            "the secrets key file {} may be read by others (mode {:o}); run `chmod 600` on it",
            path.display(),
            mode & 0o777
        )))
    }
}

/// Windows has no mode bits; the docs require restricted ACLs on the data directory.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn check_mode(path: &Path) -> Result<(), KeyUnavailable> {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        tracing::warn!(path = %path.display(), "cannot check who may read the Actions secrets key on this platform; restrict the data directory's ACLs");
    });
    Ok(())
}

/// Where secrets and variables are managed, as loaded for the acting user.
#[derive(Debug, Clone, Copy)]
pub enum ActionsTarget<'a> {
    Repository(&'a RepoContext),
    Organization(&'a OrgContext),
    /// A user's own account (personal settings).
    User(&'a User),
    // Actions: environments.
    /// A deployment environment of a repository.
    Environment(&'a RepoContext, &'a rbit_core::actions::Environment),
}

impl ActionsTarget<'_> {
    pub fn scope(&self) -> SecretScope {
        match self {
            Self::Repository(repo) => SecretScope::Repository(repo.repo.repo.id),
            Self::Organization(org) => SecretScope::Owner(org.org.account.id),
            Self::User(user) => SecretScope::Owner(user.id()),
            Self::Environment(_, environment) => SecretScope::Environment(environment.id),
        }
    }

    /// Repository administrators, organisation owners, and users for themselves.
    pub fn can_manage(&self) -> bool {
        match self {
            Self::Repository(repo) | Self::Environment(repo, _) => repo.access.can_admin(),
            Self::Organization(org) => org.can_manage,
            Self::User(_) => true,
        }
    }

    /// `owner/name` or the account's name, for audit entries.
    pub(super) fn name(&self) -> String {
        match self {
            Self::Repository(repo) => repo.repo.full_name(),
            Self::Organization(org) => org.org.name().to_string(),
            Self::User(user) => user.name().to_string(),
            Self::Environment(repo, environment) => format!("{} ({})", repo.repo.full_name(), environment.name),
        }
    }

    pub(super) fn require_manager(&self) -> Result<(), ServiceError> {
        if self.can_manage() { Ok(()) } else { Err(ServiceError::Forbidden) }
    }
}

/// A secret as settings list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedSecret {
    pub info: SecretInfo,
    /// Whether the current key sealed it; otherwise it must be set again.
    pub readable: bool,
    /// The repositories an owner secret with selected visibility is given to.
    pub repositories: Vec<RepoId>,
}

impl ActionsService {
    /// The secrets key, loaded (or created) on first use and kept; `Err` says why secrets
    /// cannot be used. [`ActionsService::spawn_background`] looks at it at start-up.
    pub(crate) async fn secrets_key(&self) -> KeyState {
        self.inner
            .secrets_key
            .get_or_init(|| async {
                let path = self.config().secrets_key_path(&self.ctx().config.storage);
                let store = self.ctx().database.actions_secrets();
                let state = load_key(&path, async move { Ok(store.any_secret().await?) }).await;
                if let Err(problem) = &state {
                    tracing::error!(%problem, "Actions secrets are unavailable");
                }
                state
            })
            .await
            .clone()
    }

    /// Why secrets cannot be stored or used now, for settings pages; `None` when they can.
    pub async fn secrets_problem(&self) -> Option<KeyUnavailable> {
        self.secrets_key().await.err()
    }

    /// The secrets of `target`, names only, for a manager.
    pub async fn list_secrets(&self, target: ActionsTarget<'_>) -> Result<Vec<ListedSecret>, ServiceError> {
        target.require_manager()?;
        let store = self.ctx().database.actions_secrets();
        let key = self.secrets_key().await.ok();
        let mut listed = Vec::new();
        for info in store.secrets(target.scope()).await? {
            let repositories = match info.visibility {
                SecretVisibility::Selected => store.secret_repositories(info.id).await?,
                SecretVisibility::All => Vec::new(),
            };
            let readable = key.as_ref().is_some_and(|key| key.sealed(info.key_id));
            listed.push(ListedSecret { info, readable, repositories });
        }
        Ok(listed)
    }

    /// Creates or replaces secret `name` of `target` with `value` for `actor` (audited, the
    /// value never logged). Repository secrets are always visible to their repository.
    pub async fn put_secret(
        &self,
        actor: &User,
        target: ActionsTarget<'_>,
        name: &str,
        value: &str,
        visibility: SecretVisibility,
    ) -> Result<SecretInfo, ServiceError> {
        target.require_manager()?;
        let name: SecretName = name.parse().map_err(|error| ServiceError::Invalid(format!("{error}")))?;
        check_value("secret", value, MAX_VALUE_BYTES)?;
        let scope = target.scope();
        rbit_actions::secrets::check_visibility(matches!(scope, SecretScope::Owner(_)), visibility == SecretVisibility::All)
            .map_err(ServiceError::Invalid)?;
        let key = self.secrets_key().await.map_err(|problem| ServiceError::Invalid(format!("secrets cannot be stored: {problem}")))?;
        let store = self.ctx().database.actions_secrets();
        let exists = store.secrets(scope).await?.iter().any(|info| info.name == name);
        if !exists && store.count_secrets(scope).await? >= MAX_PER_SCOPE {
            return Err(ServiceError::Invalid(format!("at most {MAX_PER_SCOPE} secrets may be stored here")));
        }
        // Sealing runs inside the store's transaction once the id exists; a failure there
        // leaves an unopenable placeholder, so it is surfaced and the row removed.
        let mut sealing = Ok(());
        let info = store
            .put_secret(scope, &name, visibility, actor.id(), Timestamp::now(), |id| match key.seal(scope, id, value) {
                Ok(sealed) => sealed,
                Err(error) => {
                    sealing = Err(error);
                    SealedSecret { key_id: [0; 8], nonce: [0; 12], ciphertext: Vec::new() }
                }
            })
            .await?;
        if let Err(error) = sealing {
            store.delete_secret(scope, &name).await?;
            return Err(error);
        }
        let entry = AuditEntry::new(AuditAction::ActionsSecretSet).by(actor).on(target.name()).detail(name.to_string());
        self.audit().record(entry).await;
        Ok(info)
    }

    /// Deletes secret `name` of `target` for `actor` (audited). Works without the key, so
    /// secrets sealed with a lost key can be cleared.
    pub async fn delete_secret(&self, actor: &User, target: ActionsTarget<'_>, name: &str) -> Result<(), ServiceError> {
        target.require_manager()?;
        let name: SecretName = name.parse().map_err(|_| ServiceError::NotFound)?;
        if !self.ctx().database.actions_secrets().delete_secret(target.scope(), &name).await? {
            return Err(ServiceError::NotFound);
        }
        let entry = AuditEntry::new(AuditAction::ActionsSecretDeleted).by(actor).on(target.name()).detail(name.to_string());
        self.audit().record(entry).await;
        Ok(())
    }

    /// Gives owner secret `name` to `repos` (meaningful with selected visibility); only the
    /// owner's own repositories are kept.
    pub async fn set_secret_repositories(
        &self,
        actor: &User,
        target: ActionsTarget<'_>,
        name: &str,
        repos: &[RepoId],
    ) -> Result<(), ServiceError> {
        target.require_manager()?;
        let SecretScope::Owner(owner) = target.scope() else {
            return Err(ServiceError::Invalid("only owner secrets are given to selected repositories".to_owned()));
        };
        let name: SecretName = name.parse().map_err(|_| ServiceError::NotFound)?;
        if !self.ctx().database.actions_secrets().set_secret_repositories(owner, &name, repos).await? {
            return Err(ServiceError::NotFound);
        }
        let detail = format!("{name} to {} repositories", repos.len());
        self.audit().record(AuditEntry::new(AuditAction::ActionsSecretSet).by(actor).on(target.name()).detail(detail)).await;
        Ok(())
    }

    /// The variables of `target`, for a manager.
    pub async fn list_variables(&self, target: ActionsTarget<'_>) -> Result<Vec<Variable>, ServiceError> {
        target.require_manager()?;
        Ok(self.ctx().database.actions_secrets().variables(target.scope()).await?)
    }

    /// Creates or replaces variable `name` of `target` for `actor` (audited).
    pub async fn put_variable(&self, actor: &User, target: ActionsTarget<'_>, name: &str, value: &str) -> Result<Variable, ServiceError> {
        target.require_manager()?;
        let name: VariableName = name.parse().map_err(|error| ServiceError::Invalid(format!("{error}")))?;
        check_value("variable", value, MAX_VARIABLE_BYTES)?;
        let scope = target.scope();
        let store = self.ctx().database.actions_secrets();
        let exists = store.variables(scope).await?.iter().any(|variable| variable.name == name);
        if !exists && store.count_variables(scope).await? >= MAX_PER_SCOPE {
            return Err(ServiceError::Invalid(format!("at most {MAX_PER_SCOPE} variables may be stored here")));
        }
        let variable = store.put_variable(scope, &name, value, actor.id(), Timestamp::now()).await?;
        let entry = AuditEntry::new(AuditAction::ActionsVariableSet).by(actor).on(target.name()).detail(name.to_string());
        self.audit().record(entry).await;
        Ok(variable)
    }

    /// Deletes variable `name` of `target` for `actor` (audited).
    pub async fn delete_variable(&self, actor: &User, target: ActionsTarget<'_>, name: &str) -> Result<(), ServiceError> {
        target.require_manager()?;
        let name: VariableName = name.parse().map_err(|_| ServiceError::NotFound)?;
        if !self.ctx().database.actions_secrets().delete_variable(target.scope(), &name).await? {
            return Err(ServiceError::NotFound);
        }
        let entry = AuditEntry::new(AuditAction::ActionsVariableDeleted).by(actor).on(target.name()).detail(name.to_string());
        self.audit().record(entry).await;
        Ok(())
    }

    /// The secrets a task of `run` receives, by name: the repository's, then the owner's
    /// visible to it. Untrusted runs receive none. Fails closed, naming the secret, when one
    /// cannot be opened.
    pub async fn task_secrets(&self, run: &Run) -> Result<BTreeMap<String, String>, ServiceError> {
        if !rbit_actions::trust::Grants::of_stored(run.trusted).secrets_and_variables {
            return Ok(BTreeMap::new());
        }
        let Some(owner) = self.run_owner(run).await? else {
            return Ok(BTreeMap::new());
        };
        let stored = self.ctx().database.actions_secrets().secrets_for_task(run.repo_id, owner).await?;
        if stored.is_empty() {
            return Ok(BTreeMap::new());
        }
        let key = self.secrets_key().await.map_err(|problem| ServiceError::Invalid(format!("secrets are unavailable: {problem}")))?;
        let names: Vec<String> = stored.iter().map(|secret| secret.info.name.to_string()).collect();
        let given = names.iter().map(String::as_str).zip(&stored);
        let open = |secret: &rbit_storage::sqlite::actions_secrets::TaskSecret| {
            key.open(secret.info.scope, secret.info.id, &secret.sealed).map(|value| value.as_str().to_owned())
        };
        let secrets = rbit_actions::secrets::open_task_secrets(given, open)
            .map_err(|name| ServiceError::Invalid(format!("secret {name} {UNREADABLE}")))?;
        Ok(secrets)
    }

    /// The variables a task of `run` receives (the `vars` context): the repository's, then
    /// the owner's. Untrusted runs receive none.
    pub async fn task_variables(&self, run: &Run) -> Result<BTreeMap<String, String>, ServiceError> {
        if !rbit_actions::trust::Grants::of_stored(run.trusted).secrets_and_variables {
            return Ok(BTreeMap::new());
        }
        let Some(owner) = self.run_owner(run).await? else {
            return Ok(BTreeMap::new());
        };
        let mut variables = BTreeMap::new();
        for variable in self.ctx().database.actions_secrets().variables_for_task(run.repo_id, owner).await? {
            variables.entry(variable.name.to_string()).or_insert(variable.value);
        }
        Ok(variables)
    }

    /// The owner of `run`'s repository, or `None` once the repository is gone.
    async fn run_owner(&self, run: &Run) -> Result<Option<rbit_core::AccountId>, ServiceError> {
        Ok(self.ctx().database.admin().find_repository(run.repo_id).await?.map(|repo| repo.repo.owner_id))
    }
}

/// Values are 1 to `max` bytes without NUL (`rbit_actions::secrets::check_value`).
fn check_value(kind: &str, value: &str, max: usize) -> Result<(), ServiceError> {
    rbit_actions::secrets::check_value(kind, value, max).map_err(ServiceError::Invalid)
}

#[cfg(test)]
mod tests {
    use rbit_core::AccountId;

    use super::*;

    fn key() -> SecretsKey {
        SecretsKey::from_bytes([7; 32])
    }

    #[test]
    fn a_sealed_value_opens_only_for_its_own_scope_and_id() {
        let key = key();
        let scope = SecretScope::Repository(RepoId(4));
        let sealed = key.seal(scope, SecretId(9), "hunter2").unwrap();
        assert_eq!(sealed.key_id, key.id);
        assert_eq!(sealed.ciphertext.len(), "hunter2".len() + 16);
        assert_eq!(key.open(scope, SecretId(9), &sealed).unwrap().as_str(), "hunter2");
        assert!(key.open(scope, SecretId(10), &sealed).is_none());
        assert!(key.open(SecretScope::Owner(AccountId(4)), SecretId(9), &sealed).is_none());
        assert!(key.open(SecretScope::Repository(RepoId(5)), SecretId(9), &sealed).is_none());
    }

    #[test]
    fn another_key_or_a_changed_ciphertext_does_not_open() {
        let scope = SecretScope::Owner(AccountId(1));
        let sealed = key().seal(scope, SecretId(1), "value").unwrap();
        let other = SecretsKey::from_bytes([8; 32]);
        assert!(!other.sealed(sealed.key_id));
        assert!(other.open(scope, SecretId(1), &sealed).is_none());
        // A forged key id does not help either: the tag fails.
        assert!(other.open(scope, SecretId(1), &SealedSecret { key_id: other.id, ..sealed.clone() }).is_none());
        let mut tampered = sealed;
        tampered.ciphertext[0] ^= 1;
        assert!(key().open(scope, SecretId(1), &tampered).is_none());
    }

    #[test]
    fn nonces_are_fresh_for_every_seal() {
        let key = key();
        let scope = SecretScope::Repository(RepoId(1));
        let first = key.seal(scope, SecretId(1), "same").unwrap();
        let second = key.seal(scope, SecretId(1), "same").unwrap();
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn key_files_round_trip_and_bad_ones_say_why() {
        let key = key();
        let text = key.encode();
        assert!(text.starts_with(KEY_PREFIX) && text.ends_with('\n'));
        assert_eq!(SecretsKey::parse(&text).unwrap().id, key.id);
        assert_eq!(SecretsKey::parse(&format!("  {}  ", text.trim())).unwrap().id, key.id);
        assert!(SecretsKey::parse("rbit-actions-key-v2:AAAA").unwrap_err().contains("does not start with"));
        assert!(SecretsKey::parse("rbit-actions-key-v1:%%%").unwrap_err().contains("base64"));
        assert!(SecretsKey::parse("rbit-actions-key-v1:AAAA").unwrap_err().contains("32 bytes"));
        assert!(!format!("{key:?}").contains("7, 7"));
    }

    #[tokio::test]
    async fn a_missing_key_is_created_once_and_then_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("actions").join("secrets.key");
        let created = load_key(&path, async { Ok(false) }).await.unwrap();
        let loaded = load_key(&path, async { Ok(false) }).await.unwrap();
        assert_eq!(created.id, loaded.id);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[tokio::test]
    async fn a_missing_key_is_never_created_while_secrets_are_stored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");
        let problem = load_key(&path, async { Ok(true) }).await.unwrap_err();
        assert!(problem.0.contains("missing while secrets are stored"), "{problem}");
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn a_broken_key_file_is_refused_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");
        std::fs::write(&path, "not a key").unwrap();
        // Private, as a key must be, so the content is what is refused (Unix checks the
        // mode first).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let problem = load_key(&path, async { Ok(false) }).await.unwrap_err();
        assert!(problem.0.contains("unusable"), "{problem}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a key");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_key_others_may_read_is_refused() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");
        std::fs::write(&path, key().encode().as_bytes()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let problem = load_key(&path, async { Ok(false) }).await.unwrap_err();
        assert!(problem.0.contains("chmod 600"), "{problem}");
    }

    #[test]
    fn values_are_checked() {
        assert!(check_value("secret", "x", MAX_VALUE_BYTES).is_ok());
        assert!(check_value("secret", "", MAX_VALUE_BYTES).is_err());
        assert!(check_value("secret", "a\0b", MAX_VALUE_BYTES).is_err());
        assert!(check_value("secret", &"x".repeat(MAX_VALUE_BYTES), MAX_VALUE_BYTES).is_ok());
        assert!(check_value("secret", &"x".repeat(MAX_VALUE_BYTES + 1), MAX_VALUE_BYTES).is_err());
        assert!(check_value("variable", &"x".repeat(MAX_VARIABLE_BYTES + 1), MAX_VARIABLE_BYTES).is_err());
    }
}
