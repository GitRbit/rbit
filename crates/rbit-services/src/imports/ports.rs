//! Source parsing, the address guard, the API client, repository creation, the record's
//! lifecycle gate and the copy itself behind the feature's import admission and run
//! ports. No admission or lifecycle policy lives here.
use super::{ImportInput, ImportService, api, check_source_host, client::SourceClient, job::Job, job::JobServices};
use crate::{CreateRepository, RepoContext, ServiceError};
use rbit_core::{
    Timestamp, User,
    import::{ImportOptions, ImportSource},
    repository::OwnedRepository,
};
use rbit_repositories::imports::{ImportAdmission, ImportRun, STOPPED};
use rbit_storage::sqlite::imports::NewImport;
use std::{sync::Mutex, time::Duration};

/// One "start an import" request.
pub(super) struct Admission<'a> {
    pub(super) service: &'a ImportService,
    pub(super) actor: &'a User,
    pub(super) input: &'a ImportInput,
}

impl ImportAdmission for Admission<'_> {
    type Error = ServiceError;
    type Source = ImportSource;
    type Client = SourceClient;
    type Metadata = api::Repo;
    type Created = OwnedRepository;

    fn enabled(&self) -> bool {
        self.service.is_enabled()
    }

    fn source(&self) -> Result<ImportSource, String> {
        ImportSource::parse(self.input.kind, &self.input.host, &self.input.repository).map_err(|e| e.to_string())
    }

    fn source_name(source: &ImportSource) -> String {
        source.full_name()
    }

    fn source_repository_name(source: &ImportSource) -> String {
        source.name.clone()
    }

    async fn check_host(&self, source: &ImportSource) -> Result<(), String> {
        check_source_host(&self.service.inner.ctx.config, &source.api_host, source.api_port).await.map(|_| ())
    }

    fn client(&self, source: &ImportSource, token: Option<&str>) -> Result<SourceClient, String> {
        let config = &self.service.inner.ctx.config;
        SourceClient::new(
            source.clone(),
            token.map(str::to_owned),
            config.imports.allow_private_addresses,
            Duration::from_secs(config.imports.api_timeout_seconds.max(1)),
            Duration::from_secs(config.imports.rate_limit_max_wait_seconds),
            self.service.inner.shutdown.child_token(),
            config.updates.report_version,
        )
        .map_err(|e| e.to_string())
    }

    async fn metadata(&self, client: &SourceClient) -> Result<api::Repo, String> {
        client.repository().await.map_err(|e| e.to_string())
    }

    fn description(metadata: &api::Repo) -> Option<&str> {
        metadata.description.as_deref()
    }

    async fn create(&self, name: String, description: String) -> Result<OwnedRepository, ServiceError> {
        let repository = CreateRepository { name, description, visibility: self.input.visibility };
        self.service.inner.repositories.create_in(self.actor, &self.input.owner, repository).await
    }

    async fn record(&self, created: &OwnedRepository, source: &ImportSource) -> Result<(), ServiceError> {
        let source_url = source.web_url();
        let record = NewImport {
            repo: created.repo.id,
            importer: self.actor.id(),
            kind: source.kind,
            source_url: &source_url,
            options: self.input.options,
        };
        if let Err(error) = self.service.create_record(&record).await {
            tracing::error!(%error, "cannot record an import; the repository stays empty");
            return Err(error.into());
        }
        tracing::info!(repo = %created.full_name(), source = %source_url, "import queued");
        Ok(())
    }

    fn spawn(&self, created: &OwnedRepository, client: SourceClient, metadata: api::Repo) {
        self.service.spawn(created.clone(), self.actor.clone(), client, metadata, self.input.options);
    }
}

/// One run of a queued import.
pub(super) struct Run<'a> {
    pub(super) service: &'a ImportService,
    pub(super) repo: OwnedRepository,
    pub(super) importer: User,
    /// Taken once by [`ImportRun::execute`].
    pub(super) job: Mutex<Option<(SourceClient, api::Repo, ImportOptions)>>,
}

impl ImportRun for Run<'_> {
    type Context = RepoContext;

    async fn mark_started(&self) {
        if let Err(error) = self.service.store().start(self.repo.repo.id, Timestamp::now()).await {
            tracing::warn!(%error, repo = %self.repo.full_name(), "cannot mark an import as running");
        }
    }

    async fn open_writable(&self) -> Option<RepoContext> {
        let repo = &self.repo;
        match self.service.inner.repositories.open(Some(&self.importer), repo.owner.name.as_str(), repo.repo.name.as_str()).await {
            Ok(context) if context.access.can_write() => Some(context),
            _ => None,
        }
    }

    async fn execute(&self, context: RepoContext) -> Result<(), String> {
        let Some((client, metadata, options)) = self.job.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() else {
            return Err(STOPPED.to_owned());
        };
        tracing::info!(repo = %self.repo.full_name(), "import started");
        let inner = &self.service.inner;
        let services = JobServices { ctx: inner.ctx.clone(), lfs: inner.lfs.clone(), releases: inner.releases.clone() };
        let mut job = Job::new(services, client, metadata, context, self.importer.clone(), options);
        tokio::select! {
            () = inner.shutdown.cancelled() => Err(STOPPED.to_owned()),
            outcome = job.run() => outcome,
        }
    }

    fn observe(&self, outcome: &Result<(), String>) {
        let full_name = self.repo.full_name();
        match outcome {
            Ok(()) => tracing::info!(repo = %full_name, "import finished"),
            Err(error) => tracing::warn!(repo = %full_name, %error, "import failed"),
        }
        metrics::counter!("rbit_imports_total", "outcome" => if outcome.is_ok() { "ok" } else { "error" }).increment(1);
    }

    fn clean(message: &str) -> String {
        rbit_core::import::clean_message(message)
    }
}
