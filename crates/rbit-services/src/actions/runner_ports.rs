//! The rbit-actions runner registry's ports: registration secrets and runner credentials
//! (`rbit-auth`), `actions_runners`, the in-memory runner cache, logs and the audit log.
//! The order of the steps and every refusal are decided in `rbit_actions::runners`.

use std::sync::atomic::Ordering;

use rbit_actions::runners::{Declared, Registration, RunnerDeclaration, RunnerLabels, RunnerRegistration};
use rbit_auth::{RegistrationSecret, RunnerCredential};
use rbit_core::{
    Timestamp,
    actions::{Runner, RunnerBackend, RunnerCapabilities, RunnerLabel},
};
use rbit_storage::sqlite::actions_runners::{Declaration, NewRunner, RegistrationRefusal};
use rbit_workflow::protocol::{
    Backend, Capabilities, DeclareRequest, DeclareResponse, PROTOCOL_VERSION, RegisterRequest, RegisterResponse, RunnerId,
};

use super::{
    ActionsService,
    runners::{CachedRunner, RunnerError},
};
use crate::audit::{AuditAction, AuditEntry};

type Stored = Declared<RunnerLabel, RunnerCapabilities>;

fn parse_label(name: &str) -> Result<RunnerLabel, String> {
    name.parse::<RunnerLabel>().map_err(|error| error.to_string())
}

/// What is stored of a runner's declared capabilities.
fn capabilities(declared: &Capabilities, host_labels: &[RunnerLabel]) -> RunnerCapabilities {
    let backends = declared
        .backends
        .iter()
        .map(|backend| match backend {
            Backend::Container => RunnerBackend::Container,
            Backend::Host => RunnerBackend::Host,
        })
        .collect();
    RunnerCapabilities {
        backends,
        os: declared.os.clone(),
        arch: declared.arch.clone(),
        capacity: declared.capacity,
        host_labels: host_labels.to_vec(),
        exclusive_labels: declared.exclusive_labels.iter().filter_map(|name| name.parse().ok()).collect(),
    }
}

/// `POST register` through the service.
pub(super) struct Registering<'a> {
    service: &'a ActionsService,
}

impl<'a> Registering<'a> {
    pub(super) fn new(service: &'a ActionsService) -> Self {
        Self { service }
    }
}

impl RunnerLabels for Registering<'_> {
    type Error = RunnerError;
    type Label = RunnerLabel;
    type Capabilities = RunnerCapabilities;

    fn parse_label(&self, name: &str) -> Result<RunnerLabel, String> {
        parse_label(name)
    }

    fn capabilities(&self, declared: &Capabilities, host_labels: &[RunnerLabel]) -> RunnerCapabilities {
        capabilities(declared, host_labels)
    }
}

impl RunnerRegistration for Registering<'_> {
    type Secret = RegistrationSecret;
    type Credential = RunnerCredential;
    type Runner = Runner;

    fn parse_secret(&self, token: &str) -> Option<RegistrationSecret> {
        RegistrationSecret::parse(token)
    }

    fn new_credential(&self) -> RunnerCredential {
        RunnerCredential::generate()
    }

    async fn register(
        &self,
        secret: &RegistrationSecret,
        credential: &RunnerCredential,
        request: &RegisterRequest,
        declared: &Stored,
    ) -> Result<Registration<Runner>, RunnerError> {
        let new = NewRunner {
            registration_digest: &secret.digest(),
            uuid: &random_uuid(),
            token_digest: &credential.digest(),
            name: &request.name,
            version: &request.capabilities.version,
            labels: &declared.labels,
            ephemeral: request.capabilities.ephemeral,
            capabilities: &declared.capabilities,
            protocol: PROTOCOL_VERSION,
            now: Timestamp::now(),
        };
        Ok(match self.service.ctx().database.actions_runners().register(&new).await? {
            Ok(runner) => Registration::Registered(runner),
            Err(RegistrationRefusal::InvalidToken) => Registration::InvalidToken,
            Err(RegistrationRefusal::LabelsNotAllowed) => Registration::LabelsNotAllowed,
        })
    }

    async fn admit(&self, runner: &Runner, credential: &RunnerCredential) -> Result<(), RunnerError> {
        self.service.runner_cache().await?.insert(runner, credential.digest());
        if runner.capabilities.runs_on_host() {
            tracing::warn!(runner = runner.id.0, name = %runner.name, "runner registered with host labels: their jobs run without isolation");
        }
        tracing::info!(runner = runner.id.0, name = %runner.name, scope = runner.scope.kind(), "runner registered");
        let target = self.service.scope_target(runner.scope).await?;
        let entry = AuditEntry::new(AuditAction::ActionsRunnerRegistered).on(target).detail(format!("{} (#{})", runner.name, runner.id));
        self.service.audit_log().record(entry).await;
        Ok(())
    }

    fn has_labels(&self, runner: &Runner) -> bool {
        !runner.labels.is_empty()
    }

    fn bump_tasks_version(&self) {
        self.service.bump_tasks_version();
    }

    fn response(&self, runner: &Runner, credential: &RunnerCredential) -> RegisterResponse {
        RegisterResponse { runner_id: RunnerId(runner.id.0), credential: credential.encode().into(), intervals: self.service.intervals() }
    }
}

/// `POST declare` by one authenticated runner.
pub(super) struct Declaring<'a> {
    service: &'a ActionsService,
    runner: &'a CachedRunner,
}

impl<'a> Declaring<'a> {
    pub(super) fn new(service: &'a ActionsService, runner: &'a CachedRunner) -> Self {
        Self { service, runner }
    }
}

impl RunnerLabels for Declaring<'_> {
    type Error = RunnerError;
    type Label = RunnerLabel;
    type Capabilities = RunnerCapabilities;

    fn parse_label(&self, name: &str) -> Result<RunnerLabel, String> {
        parse_label(name)
    }

    fn capabilities(&self, declared: &Capabilities, host_labels: &[RunnerLabel]) -> RunnerCapabilities {
        capabilities(declared, host_labels)
    }
}

impl RunnerDeclaration for Declaring<'_> {
    type Runner = Runner;

    fn pinned(&self) -> Option<&[RunnerLabel]> {
        self.runner.pinned_labels.as_deref()
    }

    fn current(&self) -> (&[RunnerLabel], &[RunnerLabel]) {
        (&self.runner.labels, &self.runner.host_labels)
    }

    fn now(&self) -> i64 {
        Timestamp::now().0
    }

    async fn declare(&self, request: &DeclareRequest, declared: &Stored, now: i64) -> Result<Option<Runner>, RunnerError> {
        let declaration = Declaration {
            name: &request.name,
            labels: &declared.labels,
            version: &request.capabilities.version,
            ephemeral: request.capabilities.ephemeral,
            capabilities: &declared.capabilities,
            protocol: PROTOCOL_VERSION,
            now: Timestamp(now),
        };
        Ok(self.service.ctx().database.actions_runners().declare(self.runner.id, &declaration).await?)
    }

    async fn forget(&self) -> Result<(), RunnerError> {
        self.service.runner_cache().await?.remove(self.runner.id);
        Ok(())
    }

    fn bump_tasks_version(&self) {
        self.service.bump_tasks_version();
    }

    async fn refresh(&self, stored: &Runner, now: i64) -> Result<(), RunnerError> {
        if let Some(fresh) = self.service.runner_cache().await?.refresh(stored) {
            fresh.seen_written.store(now, Ordering::SeqCst);
        }
        if stored.capabilities.runs_on_host() {
            tracing::warn!(runner = stored.id.0, name = %stored.name, "runner declares host labels: their jobs run without isolation");
        }
        Ok(())
    }

    fn response(&self, stored: &Runner) -> DeclareResponse {
        DeclareResponse {
            runner_id: RunnerId(stored.id.0),
            intervals: self.service.intervals(),
            // Actions: shared cache.
            shared_cache: true,
        }
    }
}

/// A random (version 4) UUID, as the runner row stores it.
fn random_uuid() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("operating system random number generator is unavailable");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().fold(String::with_capacity(32), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

#[cfg(test)]
mod tests {
    use rbit_actions::runners::declared;
    use rbit_workflow::protocol::DeclaredLabel;

    use super::*;

    struct Labels;

    impl RunnerLabels for Labels {
        type Error = RunnerError;
        type Label = RunnerLabel;
        type Capabilities = RunnerCapabilities;

        fn parse_label(&self, name: &str) -> Result<RunnerLabel, String> {
            parse_label(name)
        }

        fn capabilities(&self, declared: &Capabilities, host_labels: &[RunnerLabel]) -> RunnerCapabilities {
            capabilities(declared, host_labels)
        }
    }

    #[test]
    fn declarations_are_stored_with_their_host_labels() {
        let labels = [
            DeclaredLabel { name: "ubuntu-latest".into(), backend: Backend::Container },
            DeclaredLabel { name: "self-hosted".into(), backend: Backend::Host },
        ];
        let capabilities = Capabilities {
            backends: vec![Backend::Container, Backend::Host],
            os: "linux".into(),
            arch: "x86_64".into(),
            capacity: 2,
            version: "0.2.0".into(),
            ephemeral: false,
            exclusive_labels: Vec::new(),
        };
        let declared = declared(&Labels, &labels, &capabilities).unwrap();
        assert_eq!(declared.labels.iter().map(RunnerLabel::as_str).collect::<Vec<_>>(), ["ubuntu-latest", "self-hosted"]);
        assert_eq!(declared.capabilities.host_labels, vec!["self-hosted".parse::<RunnerLabel>().unwrap()]);
        assert_eq!(declared.capabilities.backends, [RunnerBackend::Container, RunnerBackend::Host]);
        assert!(declared.capabilities.runs_on_host());
    }

    #[test]
    fn uuids_are_version_four() {
        let uuid = random_uuid();
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "4");
        assert_ne!(uuid, random_uuid());
    }
}
