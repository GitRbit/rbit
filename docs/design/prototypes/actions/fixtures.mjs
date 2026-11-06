// Deliberately synthetic public examples, never private runtime data or real credentials.
export const runs = [
  { number: 842, title: "Keep repository permissions consistent", workflow: "Continuous integration", state: "failure", branch: "fix/repository-access", sha: "a34c91e", event: "pull request", actor: "sam", time: "12 minutes ago", duration: "4m 32s" },
  { number: 841, title: "Update contributor documentation", workflow: "Continuous integration", state: "running", branch: "docs/contributing", sha: "e81b640", event: "push", actor: "alex", time: "18 minutes ago", duration: "2m 08s" },
  { number: 840, title: "Publish v0.3.1", workflow: "Release", state: "approval", branch: "v0.3.1", sha: "98b7e2c", event: "manual", actor: "morgan", time: "24 minutes ago", duration: "Waiting for review" },
  { number: 839, title: "Reduce redundant repository lookups", workflow: "Continuous integration", state: "success", branch: "main", sha: "bc28f5a", event: "push", actor: "alex", time: "1 hour ago", duration: "3m 41s" },
  { number: 838, title: "Weekly dependency review", workflow: "Security", state: "success", branch: "main", sha: "d629a0f", event: "schedule", actor: "schedule", time: "2 hours ago", duration: "58s" },
  { number: 837, title: "Keep keyboard focus after navigation", workflow: "Continuous integration", state: "cancelled", branch: "fix/keyboard-navigation", sha: "6c429ad", event: "pull request", actor: "sam", time: "3 hours ago", duration: "1m 12s" },
];
export const jobs = [
  { name: "Format and lint", state: "success", duration: "48s", stage: 0, needs: "No dependencies" },
  { name: "Tests · Ubuntu / stable", state: "success", duration: "3m 12s", stage: 1, needs: "Needs format and lint" },
  { name: "Tests · Windows / stable", state: "failure", duration: "4m 32s", stage: 1, needs: "Needs format and lint" },
  { name: "Tests · Ubuntu / minimum Rust", state: "success", duration: "3m 26s", stage: 1, needs: "Needs format and lint" },
  { name: "Tests · Windows / minimum Rust", state: "success", duration: "4m 11s", stage: 1, needs: "Needs format and lint" },
  { name: "Package", state: "skipped", duration: "—", stage: 2, needs: "Needs all test jobs" },
];
export function selectedRun(ctx) { return runs.find((run) => String(run.number) === ctx.number) ?? runs[0]; }
export const steps = [
  { name: "Set up job", duration: "2s", state: "success" },
  { name: "Check out repository", duration: "4s", state: "success" },
  { name: "Install Rust toolchain", duration: "18s", state: "success" },
  { name: "Restore compiler cache", duration: "6s", state: "success" },
  { name: "Run workspace tests", duration: "3m 58s", state: "failure" },
  { name: "Upload test results", duration: "4s", state: "success" },
  { name: "Complete job", duration: "<1s", state: "success" },
];
export function logLines(state, earlier = false) {
  return Array.from({ length: 80 }, (_, index) => {
    const number = (earlier ? 49841 : 49921) + index;
    let text = index % 7 === 0 ? "test repositories::access::retains_private_visibility ... ok" : `test transport::protocol::case_${number} ... ok`;
    if (!earlier && index === 61) text = "---- repositories::access::denies_revoked_session stdout ----";
    if (!earlier && index === 62) text = "thread 'denies_revoked_session' panicked at tests/repository_access.rs:187:9";
    if (!earlier && index === 63) text = "assertion failed: response.status() == StatusCode::NOT_FOUND";
    if (!earlier && index === 64) text = "  left: 200  |  right: 404";
    if (!earlier && index === 65) text = "note: run with RUST_BACKTRACE=1 to display a backtrace";
    if (!earlier && index === 74) text = "test result: FAILED. 613 passed; 1 failed; 0 ignored";
    if (!earlier && index === 77) text = "error: test failed, to rerun pass `-p rbit-services --test integration`";
    if (!earlier && index === 79) text = state === "running" ? "Waiting for the next test result…" : "Process completed with exit code 101.";
    return { number, text, failure: !earlier && (index >= 61 && index <= 64 || index === 74 || index === 77) };
  });
}
export function rawLog() {
  const tail = logLines("failed");
  return Array.from({ length: 50000 }, (_, index) => `${index + 1} ${index < 49920 ? `test transport::protocol::case_${index + 1} ... ok` : tail[index - 49920].text}`).join("\n");
}
