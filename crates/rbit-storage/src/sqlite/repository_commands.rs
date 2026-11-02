//! Trusted builtin fixed SQL commands, not public-store/Services/Git/Fork adoption.
//!
//! SQL states are enforced here. Named backend terminals are trusted host assertions,
//! not backend evidence; no execution, crash or physical ownership certificate.
use super::{NewRepository, pool::Pool, repo_settings::RepoGeneralSettings};
use crate::{AdmissionReason, StorageError};
use rbit_authority_fence::{BeginError, SuspendedMutation, WriterRegistration};
use rbit_core::{AccountId, RepoId, RepoName, Repository, Timestamp};
use state::{Acquisition, Cell, Coordinator, Life, PhaseOwner};
use std::{collections::VecDeque, fmt, sync::Arc};
use tokio::time::Instant;

mod sql;
mod state;
#[cfg(test)]
mod tests;

pub enum RepositoryPlan {
    Create(NewRepository),
    Delete { repo: RepoId, via: RepositoryDeleteSource },
    Transfer { repo: RepoId, new_owner: AccountId },
    General { repo: RepoId, settings: RepoGeneralSettings },
    Rename { repo: RepoId, name: RepoName },
    Archive { repo: RepoId, by: AccountId, at: Timestamp },
    Unarchive { repo: RepoId },
    Settings(RepositorySettingsPlan),
}
#[derive(Debug, Clone, Copy)]
pub enum RepositoryDeleteSource {
    Repositories,
    Settings,
}
#[derive(Debug)]
pub struct RepositorySettingsPlan {
    pub repo: RepoId,
    pub general: Option<RepoGeneralSettings>,
    pub rename: Option<RepoName>,
    pub archival: Option<RepositoryArchiveChange>,
}
#[derive(Debug)]
pub enum RepositoryArchiveChange {
    Archive { by: AccountId, at: Timestamp },
    Unarchive,
}
impl fmt::Debug for RepositoryPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RepositoryPlan { fixed inputs omitted }")
    }
}
#[derive(Debug)]
enum Step {
    Create(NewRepository),
    Delete,
    Transfer(AccountId),
    General(RepoGeneralSettings),
    Rename(RepoName),
    Archive(AccountId, Timestamp),
    Unarchive,
}
struct Script {
    steps: VecDeque<Step>,
    id: Option<RepoId>,
    create: bool,
    settings: bool,
}
impl Script {
    fn from_plan(plan: RepositoryPlan) -> Result<Self, StorageError> {
        let mut steps = VecDeque::with_capacity(3);
        let (id, create, settings) = match plan {
            RepositoryPlan::Create(new) => {
                steps.push_back(Step::Create(new));
                (None, true, false)
            }
            RepositoryPlan::Delete { repo, via } => {
                steps.push_back(Step::Delete);
                (Some(repo), false, matches!(via, RepositoryDeleteSource::Settings))
            }
            RepositoryPlan::Transfer { repo, new_owner } => {
                steps.push_back(Step::Transfer(new_owner));
                (Some(repo), false, false)
            }
            RepositoryPlan::General { repo, settings } => {
                steps.push_back(Step::General(settings));
                (Some(repo), false, true)
            }
            RepositoryPlan::Rename { repo, name } => {
                steps.push_back(Step::Rename(name));
                (Some(repo), false, true)
            }
            RepositoryPlan::Archive { repo, by, at } => {
                steps.push_back(Step::Archive(by, at));
                (Some(repo), false, true)
            }
            RepositoryPlan::Unarchive { repo } => {
                steps.push_back(Step::Unarchive);
                (Some(repo), false, true)
            }
            RepositoryPlan::Settings(plan) => {
                if let Some(value) = plan.general {
                    steps.push_back(Step::General(value));
                }
                if let Some(name) = plan.rename {
                    steps.push_back(Step::Rename(name));
                }
                if let Some(value) = plan.archival {
                    steps.push_back(match value {
                        RepositoryArchiveChange::Archive { by, at } => Step::Archive(by, at),
                        RepositoryArchiveChange::Unarchive => Step::Unarchive,
                    });
                }
                (Some(plan.repo), false, true)
            }
        };
        if steps.is_empty() {
            return Err(StorageError::Invariant("empty repository plan"));
        }
        Ok(Self { steps, id, create, settings })
    }
}

/// Immutable Database-derived factory; clones share two command cells, not payload bytes.
#[derive(Debug, Clone)]
pub struct RepositoryCommands {
    coordinator: Coordinator,
    repositories: Pool,
    settings: Pool,
    writer: WriterRegistration,
}
impl RepositoryCommands {
    pub(in crate::sqlite) fn new(coordinator: Coordinator, repositories: Pool, settings: Pool, writer: WriterRegistration) -> Self {
        Self { coordinator, repositories, settings, writer }
    }
    /// Reserve before any SQL/backend effect. Arbitrary caller String sizes are not bounded here.
    pub fn reserve(&self, plan: RepositoryPlan) -> Result<RepositoryReservation, StorageError> {
        let script = Script::from_plan(plan)?;
        let cell = self.coordinator.reserve()?;
        Ok(RepositoryReservation { factory: self.clone(), cell, script: Some(script) })
    }
}
/// Nonclone no-work reservation. Pure reservations become ineligible on stop, not erased.
pub struct RepositoryReservation {
    factory: RepositoryCommands,
    cell: Arc<Cell>,
    script: Option<Script>,
}
impl fmt::Debug for RepositoryReservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cell.fmt(f)
    }
}
impl RepositoryReservation {
    /// Acquires exactly one enrollment, then suspends it before returning. Spawns no task.
    pub async fn acquire(mut self) -> Result<AdmittedRepositoryCommand, StorageError> {
        self.cell.start()?;
        let mut acquisition = Acquisition { cell: self.cell.clone(), acquired: false, done: false };
        let script = self.script.take().expect("reservation script");
        let mutation = self.factory.writer.begin().await.map_err(|reason| {
            state::admission(match reason {
                BeginError::Stopped => AdmissionReason::Stopped,
                BeginError::Capacity => AdmissionReason::Capacity,
            })
        })?;
        acquisition.acquired = true;
        let token = mutation.suspend();
        self.cell.lock().life = Life::Ready;
        self.cell.changed();
        acquisition.done = true;
        let owner = RepositoryFlowOwner { cell: self.cell.clone(), armed: true };
        let command = RepositoryCommand {
            factory: self.factory.clone(),
            cell: self.cell.clone(),
            script,
            token: Some(token),
            compensation_used: false,
            finished: false,
        };
        Ok(AdmittedRepositoryCommand { command, owner })
    }
}
impl Drop for RepositoryReservation {
    fn drop(&mut self) {
        if self.script.is_some() {
            let selected = *self.cell.lock();
            if selected.life == Life::Reserved && !selected.unknown {
                self.cell.release(false);
            } else {
                self.cell.unknown();
            }
        }
    }
}
/// Only the actual owning wrapper holds this; never put it in a frontend reply waiter.
pub struct RepositoryFlowOwner {
    cell: Arc<Cell>,
    armed: bool,
}
impl fmt::Debug for RepositoryFlowOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cell.fmt(f)
    }
}
impl Drop for RepositoryFlowOwner {
    fn drop(&mut self) {
        if self.armed {
            self.cell.unknown();
        }
    }
}
#[derive(Debug)]
pub struct AdmittedRepositoryCommand {
    pub command: RepositoryCommand,
    pub owner: RepositoryFlowOwner,
}
/// Opaque restart-only unknown cell. No release/reopen API.
pub struct RepositoryCommandQuarantine {
    cell: Arc<Cell>,
}
impl fmt::Debug for RepositoryCommandQuarantine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cell.fmt(f)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySqlBoundary {
    Committed,
    RolledBack,
    NoTransaction,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RepositorySqlProgress {
    pub committed_forward_steps: u8,
    pub last_forward_boundary: Option<RepositorySqlBoundary>,
    pub create_compensation: Option<RepositorySqlBoundary>,
}
#[derive(Debug)]
pub enum RepositoryOutput {
    Created(Repository),
    Unit,
    Changed(bool),
}
#[derive(Debug)]
pub struct RepositoryPhaseReport {
    pub repository_id: Option<RepoId>,
    pub boundary: RepositorySqlBoundary,
    pub result: Result<RepositoryOutput, StorageError>,
}
#[derive(Debug)]
pub enum RepositoryPhaseResult {
    Settled(RepositoryPhaseReport),
    Unconfirmed { original: StorageError, rollback_error: Option<StorageError> },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryLogicalSettlement {
    EnrollmentAcknowledged,
    FailureRecorded,
}
#[derive(Debug)]
pub struct RepositorySqlCompletion {
    pub progress: RepositorySqlProgress,
    pub logical: RepositoryLogicalSettlement,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RepositoryCommandDrainError {
    #[error("repository command drain deadline elapsed")]
    Deadline,
    #[error("repository command outcome unconfirmed; no clean ownership certificate")]
    Unconfirmed,
    #[error("repository command failure recorded; no global clean certificate")]
    FailureRecorded,
}

/// Fixed SQL state, not backend evidence. Suitable as an opaque Send+'static Git context.
pub struct RepositoryCommand {
    factory: RepositoryCommands,
    cell: Arc<Cell>,
    script: Script,
    token: Option<SuspendedMutation>,
    compensation_used: bool,
    finished: bool,
}
impl fmt::Debug for RepositoryCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cell.fmt(f)
    }
}
impl RepositoryCommand {
    pub fn repository_id(&self) -> Option<RepoId> {
        self.script.id
    }
    pub async fn execute_next(&mut self) -> RepositoryPhaseResult {
        if self.script.steps.is_empty() || !self.enter_phase() {
            return self.invalid_phase();
        }
        let step = self.script.steps.pop_front().expect("nonempty fixed script");
        self.run_phase(step, false).await
    }
    /// One fixed captured-ID metadata delete; never an arbitrary rollback/retry callback.
    pub async fn compensate_created(&mut self) -> RepositoryPhaseResult {
        if !self.script.create || self.script.id.is_none() || self.compensation_used || !self.script.steps.is_empty() || !self.enter_phase()
        {
            return self.invalid_phase();
        }
        self.compensation_used = true;
        self.cell.lock().logical_failure = true;
        self.run_phase(Step::Delete, true).await
    }
    fn enter_phase(&self) -> bool {
        let mut selected = self.cell.lock();
        if selected.life != Life::Ready || selected.unknown || self.token.is_none() {
            return false;
        }
        selected.life = Life::Busy;
        true
    }
    fn invalid_phase(&self) -> RepositoryPhaseResult {
        self.cell.unknown();
        RepositoryPhaseResult::Unconfirmed { original: StorageError::Invariant("repository command phase"), rollback_error: None }
    }
    async fn run_phase(&mut self, step: Step, compensation: bool) -> RepositoryPhaseResult {
        let mut phase = PhaseOwner { cell: self.cell.clone(), done: false };
        let token = self.token.take().expect("entered phase token");
        let mutation = token.resume().await;
        if self.cell.lock().unknown {
            drop(mutation);
            self.cell.finish_phase();
            phase.done = true;
            return RepositoryPhaseResult::Unconfirmed {
                original: StorageError::OutcomeUnavailable { scope: "repository command" },
                rollback_error: None,
            };
        }
        let pool = if self.script.settings && !compensation { &self.factory.settings } else { &self.factory.repositories };
        let result = sql::execute(pool, &step, self.script.id).await;
        match result {
            sql::Settled::Unconfirmed { original, rollback_error } => {
                self.cell.unknown();
                drop(mutation);
                self.cell.finish_phase();
                phase.done = true;
                RepositoryPhaseResult::Unconfirmed { original, rollback_error }
            }
            known => {
                self.token = Some(mutation.suspend());
                let report = self.record_known(known, compensation);
                self.cell.finish_phase();
                phase.done = true;
                let unknown = self.cell.lock().unknown;
                if unknown {
                    self.cell.unknown();
                    RepositoryPhaseResult::Unconfirmed {
                        original: report.result.err().unwrap_or(StorageError::OutcomeUnavailable { scope: "repository command" }),
                        rollback_error: None,
                    }
                } else {
                    RepositoryPhaseResult::Settled(report)
                }
            }
        }
    }
    fn record_known(&mut self, result: sql::Settled, compensation: bool) -> RepositoryPhaseReport {
        let (boundary, result) = match result {
            sql::Settled::Committed(raw) => (
                RepositorySqlBoundary::Committed,
                match raw {
                    sql::Raw::Created(row) => {
                        self.script.id = Some(RepoId(row.id));
                        row.try_into().map(RepositoryOutput::Created)
                    }
                    sql::Raw::Unit => Ok(RepositoryOutput::Unit),
                    sql::Raw::Changed(changed) => Ok(RepositoryOutput::Changed(changed)),
                },
            ),
            sql::Settled::RolledBack(error) => (RepositorySqlBoundary::RolledBack, Err(error)),
            sql::Settled::NoTransaction(error) => (RepositorySqlBoundary::NoTransaction, Err(error)),
            sql::Settled::Unconfirmed { .. } => unreachable!("unknown handled before settlement"),
        };
        if result.is_err() {
            self.script.steps.clear();
        }
        {
            let mut selected = self.cell.lock();
            selected.logical_failure |= result.is_err();
            if compensation {
                selected.progress.create_compensation = Some(boundary);
            } else {
                selected.progress.last_forward_boundary = Some(boundary);
                if boundary == RepositorySqlBoundary::Committed {
                    selected.progress.committed_forward_steps += 1;
                }
            }
        }
        RepositoryPhaseReport { repository_id: self.script.id, boundary, result }
    }
    /// Trusted host assertion of no external effects; source additionally requires no SQL entry.
    pub async fn cancel_before_effects(self, owner: RepositoryFlowOwner) -> Result<(), RepositoryCommandQuarantine> {
        let state = *self.cell.lock();
        if state.progress != RepositorySqlProgress::default() || self.compensation_used {
            return Err(self.quarantine(owner));
        }
        self.finish(owner, false, true).await.map(|_| ())
    }
    /// Trusted successful/no-needed actual backend termination; not backend proof.
    pub async fn finish_after_successful_backend(
        self,
        owner: RepositoryFlowOwner,
    ) -> Result<RepositorySqlCompletion, RepositoryCommandQuarantine> {
        if !self.script.steps.is_empty() {
            return Err(self.quarantine(owner));
        }
        self.finish(owner, false, false).await
    }
    /// Trusted ordinary backend failure ended; may have effects. Always logical failure.
    pub async fn finish_failed_backend(self, owner: RepositoryFlowOwner) -> Result<RepositorySqlCompletion, RepositoryCommandQuarantine> {
        self.finish(owner, true, false).await
    }
    async fn finish(
        mut self,
        mut owner: RepositoryFlowOwner,
        failed_backend: bool,
        cancel: bool,
    ) -> Result<RepositorySqlCompletion, RepositoryCommandQuarantine> {
        if !Arc::ptr_eq(&self.cell, &owner.cell) {
            return Err(self.quarantine(owner));
        }
        {
            let mut selected = self.cell.lock();
            if selected.life != Life::Ready || selected.unknown || self.token.is_none() {
                drop(selected);
                return Err(self.quarantine(owner));
            }
            selected.life = Life::Busy;
        }
        let mut phase = PhaseOwner { cell: self.cell.clone(), done: false };
        let token = self.token.take().expect("terminal token");
        let mutation = token.resume().await; // SAME enrollment, no SQL/admission.
        let (progress, failure) = {
            let mut selected = self.cell.lock();
            if selected.unknown {
                drop(selected);
                drop(mutation);
                self.cell.finish_phase();
                phase.done = true;
                return Err(self.quarantine(owner));
            }
            selected.logical_failure |= failed_backend;
            (selected.progress, selected.logical_failure)
        };
        owner.armed = false;
        if failure {
            drop(mutation);
        } else {
            mutation.complete();
        }
        if !self.cell.release(failure) {
            phase.done = true;
            return Err(self.quarantine(owner));
        }
        self.finished = true;
        phase.done = true;
        let logical =
            if failure { RepositoryLogicalSettlement::FailureRecorded } else { RepositoryLogicalSettlement::EnrollmentAcknowledged };
        debug_assert!(!cancel || !failure);
        Ok(RepositorySqlCompletion { progress, logical })
    }
    pub fn quarantine(self, owner: RepositoryFlowOwner) -> RepositoryCommandQuarantine {
        self.cell.unknown();
        drop(owner);
        RepositoryCommandQuarantine { cell: self.cell.clone() }
    }
}
impl Drop for RepositoryCommand {
    fn drop(&mut self) {
        if !self.finished {
            self.cell.unknown();
        }
    }
}

pub(in crate::sqlite) use state::Coordinator as RepositoryCoordinator;
pub(in crate::sqlite) async fn drain(
    coordinator: &RepositoryCoordinator,
    deadline: Option<Instant>,
) -> Result<(), RepositoryCommandDrainError> {
    coordinator.drain(deadline).await
}
