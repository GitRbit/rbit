//! General settings, rename, archive and deletion retain their distinct side-effect boundaries.
use super::{RepoGeneralSettings, RepositorySettingsPort, RepositorySettingsRepository, UpdateRepository};
use crate::{OwnedRepository, RepoName, names::NameError};
use rbit_contracts::Timestamp;
use rbit_identity_models::User;
const MAX_DESCRIPTION_LEN: usize = 350;
#[derive(Debug)]
pub enum RepositorySettingsError<S, E> {
    Forbidden,
    Invalid(String),
    Name(NameError),
    Store(S),
    Effect(E),
}
#[derive(Debug)]
pub struct RepositorySettings<'a> {
    current: &'a OwnedRepository,
    can_admin: bool,
}
impl<'a> RepositorySettings<'a> {
    pub fn new(current: &'a OwnedRepository, can_admin: bool) -> Self {
        Self { current, can_admin }
    }
    fn require_admin<S, E>(&self) -> Result<(), RepositorySettingsError<S, E>> {
        if self.can_admin { Ok(()) } else { Err(RepositorySettingsError::Forbidden) }
    }
    /// Updates description, visibility and default branch together.
    ///
    /// A new default branch must exist; the bare repository's `HEAD` is pointed at it so
    /// that `git clone` checks it out. A fork of a private repository cannot become
    /// public; the existing ancestor lookup remains in the adapter.
    pub async fn update<P: RepositorySettingsPort>(
        &self,
        port: &P,
        input: UpdateRepository,
    ) -> Result<(), RepositorySettingsError<P::StoreError, P::Error>> {
        self.require_admin()?;
        let description = input.description.trim();
        if description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(RepositorySettingsError::Invalid(format!("description must be at most {MAX_DESCRIPTION_LEN} characters")));
        }
        if description.chars().any(char::is_control) {
            return Err(RepositorySettingsError::Invalid("description must be a single line of text".to_owned()));
        }

        let current = &self.current.repo;
        if input.visibility.is_public()
            && !current.visibility.is_public()
            && port.has_private_ancestor().await.map_err(RepositorySettingsError::Effect)?
        {
            return Err(RepositorySettingsError::Invalid("a fork of a private repository must stay private".to_owned()));
        }
        let default_branch = match input.default_branch.map(|b| b.trim().to_owned()) {
            Some(branch) if branch != current.default_branch => {
                if !port.branch_exists(&branch).await.map_err(RepositorySettingsError::Effect)? {
                    return Err(RepositorySettingsError::Invalid(format!("branch \"{branch}\" does not exist")));
                }
                Some(branch)
            }
            _ => None,
        };

        let settings = RepoGeneralSettings {
            description: (!description.is_empty()).then(|| description.to_owned()),
            visibility: input.visibility,
            default_branch: default_branch.clone().unwrap_or_else(|| current.default_branch.clone()),
        };

        if let Some(branch) = &default_branch {
            port.set_head(branch).await.map_err(RepositorySettingsError::Effect)?;
        }
        if let Err(error) = port.store().update_general(current.id, &settings).await {
            if default_branch.is_some()
                && let Err(restore) = port.set_head(&current.default_branch).await
            {
                port.restore_failed(&restore);
            }
            return Err(RepositorySettingsError::Store(error));
        }

        port.updated(&settings);
        Ok(())
    }

    /// Renames the repository and returns the new name. On-disk paths use ids, so nothing moves.
    pub async fn rename<P: RepositorySettingsPort>(
        &self,
        port: &P,
        new_name: &str,
    ) -> Result<RepoName, RepositorySettingsError<P::StoreError, P::Error>> {
        self.require_admin()?;
        let name: RepoName = new_name.trim().parse().map_err(RepositorySettingsError::Name)?;
        if name != self.current.repo.name {
            port.store().rename(self.current.repo.id, &name).await.map_err(RepositorySettingsError::Store)?;
            port.renamed(&name);
        }
        Ok(name)
    }

    /// Archives the repository (`archived`) or unarchives it, as `actor`. `false` when it
    /// already was in that state: nothing is recorded or published then.
    pub async fn set_archived<P: RepositorySettingsPort>(
        &self,
        actor: &User,
        port: &P,
        archived: bool,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<bool, RepositorySettingsError<P::StoreError, P::Error>> {
        self.require_admin()?;
        let id = self.current.repo.id;
        let changed = if archived {
            port.store().archive(id, actor.id(), now()).await.map_err(RepositorySettingsError::Store)?
        } else {
            port.store().unarchive(id).await.map_err(RepositorySettingsError::Store)?
        };
        if !changed {
            return Ok(false);
        }
        port.archive_committed(actor, archived).await;
        Ok(true)
    }

    /// Permanently deletes the repository: its bare repository on disk, then its metadata
    /// (stars cascade). `confirmation` must be the full `owner/name`; `actor` is the
    /// administrator doing it (the access check itself is `repo`'s).
    ///
    /// Disk first: if that fails nothing is lost from the database and the deletion can
    /// simply be retried; if the database step fails afterwards, a retry still succeeds
    /// because removing an already-missing directory is not an error.
    pub async fn delete<P: RepositorySettingsPort>(
        &self,
        actor: &User,
        port: &P,
        confirmation: &str,
    ) -> Result<(), RepositorySettingsError<P::StoreError, P::Error>> {
        self.require_admin()?;
        let full_name = self.current.full_name();
        if confirmation.trim() != full_name {
            return Err(RepositorySettingsError::Invalid(format!("type \"{full_name}\" to confirm")));
        }
        let current = &self.current.repo;
        // The settings page may have pinned this repository while loading its refs.
        // Close our own handle before deletion waits for other requests and jobs.
        port.close_reader();
        port.delete_disk().await.map_err(RepositorySettingsError::Effect)?;
        port.store().delete(current.id).await.map_err(RepositorySettingsError::Store)?;
        port.deleted(actor, full_name);
        Ok(())
    }
}
