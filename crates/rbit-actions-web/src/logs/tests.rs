use super::*;
use std::fmt;

#[derive(Template)]
#[template(source = "<section id=\"job\">{{ body }}</section>", ext = "html")]
struct JobShell<'a> {
    body: StepLogBody<'a>,
}
#[derive(Template)]
#[template(source = "<section id=\"step\">{{ body }}</section>", ext = "html")]
struct StepShell<'a> {
    body: StepLogBody<'a>,
}
fn render(log: Option<&StepLog>, log_url: &str) -> String {
    StepLogBody { log, log_url }.render().unwrap()
}
#[test]
fn unavailable_empty_and_expired_logs_are_distinct_and_never_emit_retained_private_lines() {
    assert!(render(None, "").contains("This step has not started."));
    let mut log = StepLog::default();
    assert!(render(Some(&log), "").contains("No output."));
    log.lines.push("RETAINED_PRIVATE_LOG".into());
    log.expired = true;
    let output = render(Some(&log), "/logs");
    assert!(output.contains("This log was removed by the log retention."));
    assert!(!output.contains("RETAINED_PRIVATE_LOG"));
    assert!(!output.contains("code-line"));
}
#[test]
fn both_host_shells_preserve_one_borrowed_escaped_code_body_and_exact_navigation() {
    let log = StepLog {
        lines: vec!["<span class=\"text-danger\">Error: &lt;script&gt;not_executed()&lt;/script&gt;</span>".into()],
        first: 17,
        more_before: true,
        truncated: true,
        expired: false,
    };
    let url = "/logs?run=1&job=2&step=0";
    let component = render(Some(&log), url);
    let preview = JobShell { body: StepLogBody { log: Some(&log), log_url: url } }.render().unwrap();
    let standalone = StepShell { body: StepLogBody { log: Some(&log), log_url: url } }.render().unwrap();
    assert_eq!(preview, format!("<section id=\"job\">{component}</section>"));
    assert_eq!(standalone, format!("<section id=\"step\">{component}</section>"));
    assert!(component.contains("<span class=\"text-danger\">Error: &lt;script&gt;not_executed()&lt;/script&gt;</span>"));
    assert!(!component.contains("<script>"));
    assert!(!component.contains("&amp;lt;"));
    assert!(component.contains("data-line=\"17\""));
    assert!(component.contains("href=\"/logs?run=1&#38;job=2&#38;step=0\""));
    assert!(component.contains("Earlier lines are left out."));
    assert!(component.contains("later lines were dropped."));
    assert!(!render(Some(&log), "").contains("Show more of this step"), "standalone shell must not invent a self link");
}
#[test]
fn block_boundary_preserves_step_relative_numbers_and_bounded_column_class() {
    let mut lines = vec!["x".into(); 129];
    lines[128] = "x".repeat(257);
    let log = StepLog { lines, first: 9, ..StepLog::default() };
    let output = render(Some(&log), "");
    assert_eq!(output.matches("class=\"vblock\"").count(), 2);
    assert!(output.contains("data-rows=\"128\"") && output.contains("data-rows=\"1\""));
    assert!(output.contains("data-line=\"9\"") && output.contains("data-line=\"137\""));
    assert!(output.contains("code-lines cols-256"));
    assert_eq!(output.matches("class=\"code-line\"").count(), 129);
}
struct Writer {
    bytes: usize,
    limit: usize,
}
impl fmt::Write for Writer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.limit - self.bytes {
            return Err(fmt::Error);
        }
        self.bytes += value.len();
        Ok(())
    }
}
#[test]
fn nested_log_render_propagates_host_output_limit_without_truncation_success() {
    let log = StepLog { lines: vec!["x".repeat(16 * 1024)], first: 1, ..StepLog::default() };
    let template = JobShell { body: StepLogBody { log: Some(&log), log_url: "" } };
    let mut writer = Writer { bytes: 0, limit: 1024 };
    assert!(matches!(template.render_into(&mut writer), Err(askama::Error::Fmt)));
    assert!(writer.bytes <= writer.limit);
    let mut exact = Writer { bytes: 0, limit: template.render().unwrap().len() };
    template.render_into(&mut exact).unwrap();
    assert_eq!(exact.bytes, exact.limit);

    // Exercise the actual host document cap as well as a small failing writer.
    let cap = 2 * 1024 * 1024;
    let large_log = StepLog { lines: vec!["x".repeat(cap)], first: 1, ..StepLog::default() };
    let large = JobShell { body: StepLogBody { log: Some(&large_log), log_url: "" } };
    let mut bounded = Writer { bytes: 0, limit: cap };
    assert!(matches!(large.render_into(&mut bounded), Err(askama::Error::Fmt)));
    assert!(bounded.bytes <= cap);
}
