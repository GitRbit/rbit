//! `POST register` and `POST declare` (§12.6): what a runner may register or declare, in
//! which order the steps happen, and the refusal each failure is answered with.
//!
//! * A registration token that does not parse, does not exist, was revoked, expired or is
//!   used up is `unauthenticated` (the caller throttles those); labels outside the token's
//!   pin are `forbidden`. A runner registered with labels moves the scheduler's version.
//! * A declaration may not name labels outside the runner's pin. A runner whose row is gone
//!   is forgotten and `unauthenticated`. A change of labels or host labels moves the
//!   scheduler's version, so jobs the runner can now take are offered to it.

use std::{fmt, future::Future};

use rbit_workflow::protocol::{Capabilities, DeclareRequest, DeclareResponse, DeclaredLabel, RegisterRequest, RegisterResponse};

use super::policy::{labels_changed, unpinned_label};

/// The answer to a registration token that cannot be used.
pub const UNKNOWN_REGISTRATION: &str = "the registration token is unknown, revoked, expired or used up";
/// The answer to a runner credential that is not, or no longer, known.
pub const UNKNOWN_CREDENTIAL: &str = "the runner credential is missing, malformed or unknown";

/// A refused runner call, by protocol error code, or what a port failed with.
pub enum RunnerRefusal<E> {
    BadRequest(String),
    Unauthenticated(&'static str),
    Forbidden(String),
    Port(E),
}

// Port errors may not implement Debug; diagnostics never format them.
impl<E> fmt::Debug for RunnerRefusal<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(message) => f.debug_tuple("BadRequest").field(message).finish(),
            Self::Unauthenticated(message) => f.debug_tuple("Unauthenticated").field(message).finish(),
            Self::Forbidden(message) => f.debug_tuple("Forbidden").field(message).finish(),
            Self::Port(_) => f.write_str("Port(..)"),
        }
    }
}

/// Labels and capabilities as stored, from what a runner sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared<L, C> {
    pub labels: Vec<L>,
    /// The labels that run jobs on the host backend, without isolation.
    pub host_labels: Vec<L>,
    pub capabilities: C,
}

/// How declared labels and capabilities are stored.
pub trait RunnerLabels: Sync {
    type Error: Send;
    type Label: Ord + Clone + Send + Sync + fmt::Display;
    type Capabilities: Send + Sync;

    /// A declared label's name, parsed; the error is the runner's message.
    fn parse_label(&self, name: &str) -> Result<Self::Label, String>;
    /// What is stored of `declared` capabilities, with the host labels.
    fn capabilities(&self, declared: &Capabilities, host_labels: &[Self::Label]) -> Self::Capabilities;
}

/// What a runner declared, as stored; the first label that does not parse refuses it.
pub fn declared<P: RunnerLabels>(
    port: &P,
    labels: &[DeclaredLabel],
    capabilities: &Capabilities,
) -> Result<Declared<P::Label, P::Capabilities>, String> {
    let parsed = labels.iter().map(|label| port.parse_label(&label.name)).collect::<Result<Vec<_>, _>>()?;
    let host_labels: Vec<_> =
        labels.iter().zip(&parsed).filter(|(label, _)| !label.backend.isolates_jobs()).map(|(_, parsed)| parsed.clone()).collect();
    let capabilities = port.capabilities(capabilities, &host_labels);
    Ok(Declared { labels: parsed, host_labels, capabilities })
}

/// What recording a registration did.
#[derive(Debug)]
pub enum Registration<R> {
    Registered(R),
    /// The token does not exist, was revoked, expired or is used up.
    InvalidToken,
    /// The token pins other labels.
    LabelsNotAllowed,
}

/// `POST register`.
pub trait RunnerRegistration: RunnerLabels {
    type Secret: Send + Sync;
    type Credential: Send + Sync;
    type Runner: Send + Sync;

    fn parse_secret(&self, token: &str) -> Option<Self::Secret>;
    fn new_credential(&self) -> Self::Credential;
    /// One use of the token, for a new runner in its scope, in one transaction.
    fn register(
        &self,
        secret: &Self::Secret,
        credential: &Self::Credential,
        request: &RegisterRequest,
        declared: &Declared<Self::Label, Self::Capabilities>,
    ) -> impl Future<Output = Result<Registration<Self::Runner>, Self::Error>> + Send;
    /// Makes the runner known to authentication, logs and audits its registration.
    fn admit(&self, runner: &Self::Runner, credential: &Self::Credential) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn has_labels(&self, runner: &Self::Runner) -> bool;
    fn bump_tasks_version(&self);
    /// The answer: the runner's id, its credential (shown once) and the intervals.
    fn response(&self, runner: &Self::Runner, credential: &Self::Credential) -> RegisterResponse;
}

/// `POST declare`, by one authenticated runner.
pub trait RunnerDeclaration: RunnerLabels {
    type Runner: Send + Sync;

    /// The labels its registration token pins, `None` for any.
    fn pinned(&self) -> Option<&[Self::Label]>;
    /// Its labels and host labels as known before this declaration.
    fn current(&self) -> (&[Self::Label], &[Self::Label]);
    /// Unix seconds now.
    fn now(&self) -> i64;
    /// Stores the declaration; `None` when the runner's row is gone.
    fn declare(
        &self,
        request: &DeclareRequest,
        declared: &Declared<Self::Label, Self::Capabilities>,
        now: i64,
    ) -> impl Future<Output = Result<Option<Self::Runner>, Self::Error>> + Send;
    /// Stops authenticating the runner.
    fn forget(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn bump_tasks_version(&self);
    /// Makes the stored runner current for authentication (written last at `now`) and logs it.
    fn refresh(&self, stored: &Self::Runner, now: i64) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn response(&self, stored: &Self::Runner) -> DeclareResponse;
}

/// Registration and declaration of runners.
#[derive(Debug)]
pub struct RunnerRegistry;

impl RunnerRegistry {
    /// `POST register`: one use of a registration token for a new runner in its scope.
    pub async fn register<P: RunnerRegistration>(port: &P, request: &RegisterRequest) -> Result<RegisterResponse, RunnerRefusal<P::Error>> {
        request.validate().map_err(|error| RunnerRefusal::BadRequest(error.message))?;
        let Some(secret) = port.parse_secret(request.token.expose()) else {
            return Err(RunnerRefusal::Unauthenticated(UNKNOWN_REGISTRATION));
        };
        let declared = declared(port, &request.labels, &request.capabilities).map_err(RunnerRefusal::BadRequest)?;
        let credential = port.new_credential();
        let runner = match port.register(&secret, &credential, request, &declared).await.map_err(RunnerRefusal::Port)? {
            Registration::Registered(runner) => runner,
            Registration::InvalidToken => return Err(RunnerRefusal::Unauthenticated(UNKNOWN_REGISTRATION)),
            Registration::LabelsNotAllowed => {
                return Err(RunnerRefusal::Forbidden("the registration token allows other labels than these".to_owned()));
            }
        };
        port.admit(&runner, &credential).await.map_err(RunnerRefusal::Port)?;
        if port.has_labels(&runner) {
            port.bump_tasks_version();
        }
        Ok(port.response(&runner, &credential))
    }

    /// `POST declare`: the runner's name, labels and capabilities at its start.
    pub async fn declare<P: RunnerDeclaration>(port: &P, request: &DeclareRequest) -> Result<DeclareResponse, RunnerRefusal<P::Error>> {
        request.validate().map_err(|error| RunnerRefusal::BadRequest(error.message))?;
        let declared = declared(port, &request.labels, &request.capabilities).map_err(RunnerRefusal::BadRequest)?;
        if let Some(label) = unpinned_label(port.pinned(), &declared.labels) {
            return Err(RunnerRefusal::Forbidden(format!("this runner's registration token does not allow label `{label}`")));
        }
        let now = port.now();
        let Some(stored) = port.declare(request, &declared, now).await.map_err(RunnerRefusal::Port)? else {
            port.forget().await.map_err(RunnerRefusal::Port)?;
            return Err(RunnerRefusal::Unauthenticated(UNKNOWN_CREDENTIAL));
        };
        if labels_changed(port.current(), (&declared.labels, &declared.host_labels)) {
            port.bump_tasks_version();
        }
        port.refresh(&stored, now).await.map_err(RunnerRefusal::Port)?;
        Ok(port.response(&stored))
    }
}
