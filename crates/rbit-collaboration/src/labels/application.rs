//! Catalog policy and exact legacy validation/count/throttle/write ordering.

use rbit_contracts::{RepoId, Timestamp};

use super::{DEFAULT_LABELS, Label, LabelId, LabelInput, LabelRepository, LabelUsage, MAX_LABELS_PER_REPO, label_fields};

/// Access already resolved for this request. Reads retain their caller's visibility gate.
#[derive(Debug, Clone, Copy)]
pub struct LabelAccess {
    pub repo: RepoId,
    pub can_write: bool,
    pub archived: bool,
}

impl LabelAccess {
    fn require_writer<E, P>(self) -> Result<(), LabelError<E, P>> {
        if !self.can_write {
            return Err(LabelError::Forbidden);
        }
        if self.archived {
            return Err(LabelError::Archived);
        }
        Ok(())
    }
}

/// Policy errors remain distinct from persistence and caller-owned throttle errors.
#[derive(Debug)]
pub enum LabelError<E, P = E> {
    Store(E),
    Throttle(P),
    Invalid(String),
    Forbidden,
    Archived,
    NotFound,
}

/// A statically dispatched catalog application; no runtime or adapter ownership.
#[derive(Debug)]
pub struct LabelApplication<F> {
    repository: F,
    access: LabelAccess,
}

impl<R: LabelRepository, F: Fn() -> R> LabelApplication<F> {
    pub fn new(repository: F, access: LabelAccess) -> Self {
        Self { repository, access }
    }

    pub async fn list(&self) -> Result<Vec<Label>, LabelError<R::Error>> {
        let repository = (self.repository)();
        repository.labels(self.access.repo).await.map_err(LabelError::Store)
    }

    pub async fn usage(&self) -> Result<Vec<LabelUsage>, LabelError<R::Error>> {
        // Usage counts open issues, not pull requests, exactly as the existing UI.
        let repository = (self.repository)();
        repository.label_usage(self.access.repo, "issue").await.map_err(LabelError::Store)
    }

    pub async fn named(&self, name: &str) -> Result<Label, LabelError<R::Error>> {
        let repository = (self.repository)();
        repository.find_label_by_name(self.access.repo, name).await.map_err(LabelError::Store)?.ok_or(LabelError::NotFound)
    }

    pub async fn named_many(&self, names: &[String]) -> Result<Vec<Label>, LabelError<R::Error>> {
        let repository = (self.repository)();
        let labels = repository.labels_named(self.access.repo, names).await.map_err(LabelError::Store)?;
        if let Some(missing) = names.iter().find(|name| !labels.iter().any(|label| label.name.eq_ignore_ascii_case(name))) {
            return Err(LabelError::Invalid(format!("there is no label called “{missing}”")));
        }
        Ok(labels)
    }

    pub async fn create<P>(
        &self,
        input: &LabelInput,
        spend: impl FnOnce() -> Result<(), P>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Label, LabelError<R::Error, P>> {
        self.access.require_writer()?;
        let fields = label_fields(input).map_err(LabelError::Invalid)?;
        let repository = (self.repository)();
        if repository.count_labels(self.access.repo).await.map_err(LabelError::Store)? >= MAX_LABELS_PER_REPO {
            return Err(LabelError::Invalid(format!("a repository can have at most {MAX_LABELS_PER_REPO} labels")));
        }
        spend().map_err(LabelError::Throttle)?;
        repository.create_label(self.access.repo, &fields, now()).await.map_err(LabelError::Store)
    }

    pub async fn update<P>(
        &self,
        id: LabelId,
        input: &LabelInput,
        spend: impl FnOnce() -> Result<(), P>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Label, LabelError<R::Error, P>> {
        self.access.require_writer()?;
        let fields = label_fields(input).map_err(LabelError::Invalid)?;
        spend().map_err(LabelError::Throttle)?;
        let repository = (self.repository)();
        repository.update_label(self.access.repo, id, &fields, now()).await.map_err(LabelError::Store)?.ok_or(LabelError::NotFound)
    }

    /// Deletion neither spends content throttle nor appends timeline/audit events.
    pub async fn delete(&self, id: LabelId) -> Result<Label, LabelError<R::Error>> {
        self.access.require_writer()?;
        let repository = (self.repository)();
        repository.delete_label(self.access.repo, id).await.map_err(LabelError::Store)?.ok_or(LabelError::NotFound)
    }

    pub async fn seed_defaults<P>(
        &self,
        spend: impl FnOnce() -> Result<(), P>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<u64, LabelError<R::Error, P>> {
        self.access.require_writer()?;
        let repository = (self.repository)();
        if repository.count_labels(self.access.repo).await.map_err(LabelError::Store)? > 0 {
            return Ok(0);
        }
        spend().map_err(LabelError::Throttle)?;
        let labels = DEFAULT_LABELS
            .iter()
            .map(|(name, color, description)| super::LabelFields {
                name: (*name).to_owned(),
                color: (*color).to_owned(),
                description: (*description).to_owned(),
            })
            .collect::<Vec<_>>();
        repository.create_missing_labels(self.access.repo, &labels, now()).await.map_err(LabelError::Store)
    }
}
