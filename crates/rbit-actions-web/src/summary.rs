use askama::Template;

/// Passive step-summary content composed into a host-owned full document.
///
/// `html` is trusted host output from the existing bounded Markdown sanitizer,
/// never raw package/plugin/user HTML. This type does not sanitize, attest trust,
/// authorize reads or choose the host's document/CSP/navigation. Callers are the
/// trusted compiled first-party host adapter. Source fallback and every other
/// string remain escaped by Askama. Ownership moves existing summary buffers.
#[derive(Debug, Template)]
#[template(path = "actions/summary-content.html")]
pub struct SummaryContent {
    pub heading: String,
    pub run_url: String,
    pub html: Option<String>,
    pub source: String,
    pub previous: Option<String>,
    pub next: Option<String>,
}
// Compiled HTML already escapes ordinary values; only host-sanitized html is safe.
// The marker prevents double escaping when the host nests this concrete template.
impl askama::filters::HtmlSafe for SummaryContent {}

#[cfg(test)]
mod tests;
