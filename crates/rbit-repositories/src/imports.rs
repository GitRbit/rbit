//! Import admission and lifecycle policy: what is checked before a repository is
//! created, what the local repository is called and described as, who may see an
//! import's progress, and how one run starts, ends and is reported.
//!
//! Problems with the source (an unknown repository, a refused token, an internal
//! address) are reported before anything is created. The token is checked here but
//! never stored. Source parsing, the outbound address guard, the API client, creating
//! the repository row, the record's lifecycle gate, the copy itself and the scheduler
//! runtime stay with composition behind [`ImportAdmission`] and [`ImportRun`].
use std::future::Future;

/// Longest token accepted, in bytes.
pub const MAX_TOKEN_BYTES: usize = 1024;
/// Longest repository description (as repository creation accepts), in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 350;
/// Recorded for imports a restart interrupted.
pub const INTERRUPTED: &str = "the import was interrupted when the server restarted; the token is not kept, so start a new import";
/// Recorded for imports the server stopped.
pub const STOPPED: &str = "the import was stopped because the server shut down";
/// Recorded when the importer can no longer write to the repository when the run starts.
pub const UNAVAILABLE: &str = "the repository is no longer available to the importing user";
/// The refusal while importing is switched off.
pub const DISABLED: &str = "importing is disabled on this instance";

/// The token to use: trimmed, `None` when empty; refused when too long or when it holds
/// whitespace or control characters.
pub fn checked_token(token: &str) -> Result<Option<&str>, &'static str> {
    let token = token.trim();
    if token.len() > MAX_TOKEN_BYTES || token.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("the token is not valid");
    }
    Ok((!token.is_empty()).then_some(token))
}

/// The local name: the one asked for, or the source's when none was given.
pub fn local_name(requested: &str, source: &str) -> String {
    match requested.trim() {
        "" => source.to_owned(),
        name => name.to_owned(),
    }
}

/// The local description: the source's, trimmed and cut to [`MAX_DESCRIPTION_CHARS`]
/// characters, or empty.
pub fn local_description(source: Option<&str>) -> String {
    source.map(|description| truncate_chars(description.trim(), MAX_DESCRIPTION_CHARS).to_owned()).unwrap_or_default()
}

/// The first `max_chars` characters of `text`.
fn truncate_chars(text: &str, max_chars: usize) -> &str {
    text.char_indices().nth(max_chars).map_or(text, |(index, _)| &text[..index])
}

/// An import's page is shown to its importer and the repository's administrators only.
pub fn may_view_status(can_admin: bool, viewer_is_importer: bool) -> bool {
    can_admin || viewer_is_importer
}

/// Why an import could not start: a problem to show on the form, or a failure of
/// composition (creating the repository or its record) with its own identity.
#[derive(Debug, PartialEq, Eq)]
pub enum AdmissionError<E> {
    Invalid(String),
    Failure(E),
}

/// What starting an import needs, bound by composition to one request.
pub trait ImportAdmission: Sync {
    type Error: Send;
    type Source: Send + Sync;
    type Client: Send + Sync;
    type Metadata: Send + Sync;
    type Created: Send + Sync;
    fn enabled(&self) -> bool;
    fn source(&self) -> Result<Self::Source, String>;
    /// `owner/name` on the source, for messages.
    fn source_name(source: &Self::Source) -> String;
    /// The source repository's own name.
    fn source_repository_name(source: &Self::Source) -> String;
    /// Checks that the source's API host resolves to permitted addresses only.
    fn check_host(&self, source: &Self::Source) -> impl Future<Output = Result<(), String>> + Send;
    fn client(&self, source: &Self::Source, token: Option<&str>) -> Result<Self::Client, String>;
    fn metadata(&self, client: &Self::Client) -> impl Future<Output = Result<Self::Metadata, String>> + Send;
    fn description(metadata: &Self::Metadata) -> Option<&str>;
    fn create(&self, name: String, description: String) -> impl Future<Output = Result<Self::Created, Self::Error>> + Send;
    /// Records the queued import behind the restart gate.
    fn record(&self, created: &Self::Created, source: &Self::Source) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Starts the import in the background.
    fn spawn(&self, created: &Self::Created, client: Self::Client, metadata: Self::Metadata);
}

/// Validates the request, reads the source's metadata, creates the local repository,
/// records the import and starts it. Nothing is created for a problem with the source.
pub async fn admit<A: ImportAdmission>(admission: &A, token: &str, name: &str) -> Result<A::Created, AdmissionError<A::Error>> {
    if !admission.enabled() {
        return Err(AdmissionError::Invalid(DISABLED.to_owned()));
    }
    let source = admission.source().map_err(AdmissionError::Invalid)?;
    let token = checked_token(token).map_err(|error| AdmissionError::Invalid(error.to_owned()))?;
    admission.check_host(&source).await.map_err(AdmissionError::Invalid)?;
    let client = admission.client(&source, token).map_err(AdmissionError::Invalid)?;
    let metadata =
        admission.metadata(&client).await.map_err(|error| AdmissionError::Invalid(format!("{}: {error}", A::source_name(&source))))?;
    let name = local_name(name, &A::source_repository_name(&source));
    let description = local_description(A::description(&metadata));
    let created = admission.create(name, description).await.map_err(AdmissionError::Failure)?;
    admission.record(&created, &source).await.map_err(AdmissionError::Failure)?;
    admission.spawn(&created, client, metadata);
    Ok(created)
}

/// One run's surroundings, bound by composition to one import.
pub trait ImportRun: Sync {
    type Context: Send + Sync;
    /// Marks the import running (a failure is only logged).
    fn mark_started(&self) -> impl Future<Output = ()> + Send;
    /// The repository opened for the importer, if they may still write to it.
    fn open_writable(&self) -> impl Future<Output = Option<Self::Context>> + Send;
    /// Runs the copy until it ends or the server stops; `Err` says why it stopped.
    fn execute(&self, context: Self::Context) -> impl Future<Output = Result<(), String>> + Send;
    /// Logs and counts the outcome.
    fn observe(&self, outcome: &Result<(), String>);
    /// The message as the progress page shows it.
    fn clean(message: &str) -> String;
}

/// Runs one import; returns why it failed, if it did.
pub async fn run<R: ImportRun>(run: &R) -> Option<String> {
    run.mark_started().await;
    let Some(context) = run.open_writable().await else { return Some(UNAVAILABLE.to_owned()) };
    let outcome = run.execute(context).await;
    run.observe(&outcome);
    outcome.err().map(|error| R::clean(&error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_names_descriptions_and_visibility_keep_their_rules() {
        assert_eq!(checked_token("  "), Ok(None));
        assert_eq!(checked_token(" ghp_abc "), Ok(Some("ghp_abc")));
        assert_eq!(checked_token("a b"), Err("the token is not valid"));
        assert_eq!(checked_token("a\u{7}"), Err("the token is not valid"));
        assert_eq!(checked_token(&"x".repeat(MAX_TOKEN_BYTES)), Ok(Some("x".repeat(MAX_TOKEN_BYTES).as_str())));
        assert!(checked_token(&"x".repeat(MAX_TOKEN_BYTES + 1)).is_err());
        assert_eq!(local_name("  ", "tools"), "tools");
        assert_eq!(local_name(" mine ", "tools"), "mine");
        assert_eq!(local_description(None), "");
        assert_eq!(local_description(Some("  hi  ")), "hi");
        assert_eq!(local_description(Some(&"é".repeat(400))).chars().count(), MAX_DESCRIPTION_CHARS);
        assert!(may_view_status(true, false) && may_view_status(false, true) && !may_view_status(false, false));
    }
}
