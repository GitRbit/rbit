// All dependencies are required evidence. A skipped job is never a passing gate.
export const REQUIRED_CHECKS = Object.freeze([
  { id: "rewrite-contracts", label: "Architecture and contracts", platform: "Linux · Windows" },
  { id: "lint", label: "Formatting and Clippy", platform: "Linux" },
  { id: "deny", label: "Licenses and security advisories", platform: "Linux" },
  { id: "tests-linux-build", label: "Test build and archive", platform: "Linux" },
  { id: "tests-linux-shards", label: "Workspace test shards", platform: "Linux" },
  { id: "tests-linux-extras", label: "Doc tests, server build and upgrade rehearsal", platform: "Linux" },
  { id: "tests-linux", label: "Workspace tests (required check)", platform: "Linux" },
  { id: "tests-windows-build", label: "Test build and archive", platform: "Windows" },
  { id: "tests-windows-shards", label: "Workspace test shards", platform: "Windows" },
  { id: "tests-windows-extras", label: "Doc tests", platform: "Windows" },
  { id: "tests-windows", label: "Workspace tests (required check)", platform: "Windows" },
  { id: "assets", label: "Front-end build and tests", platform: "Linux" },
  { id: "release", label: "Release binaries and version smoke", platform: "Linux" },
  { id: "runner-image", label: "Runner image and tool smoke", platform: "Linux · Docker" },
]);
const states = new Set(["success", "failure", "cancelled", "skipped"]);

export function evaluateResults(serialized) {
  let needs, inputIssue = null;
  try { needs = JSON.parse(serialized); }
  catch { inputIssue = "Dependency results are not valid JSON."; }
  if (!inputIssue && (!needs || typeof needs !== "object" || Array.isArray(needs))) inputIssue = "Dependency results must be an object.";
  if (!inputIssue && Object.keys(needs).some((id) => !REQUIRED_CHECKS.some((check) => check.id === id))) inputIssue = "The required-check manifest does not cover every dependency.";
  const rows = REQUIRED_CHECKS.map((check) => {
    const job = needs && Object.hasOwn(needs, check.id) ? needs[check.id] : undefined;
    const result = job === undefined ? "missing" : states.has(job?.result) ? job.result : "unknown";
    return { ...check, result };
  });
  return { rows, inputIssue, passed: !inputIssue && rows.every((row) => row.result === "success") };
}

const labels = { success: "✅ Passed", failure: "❌ Failed", cancelled: "⏹ Cancelled", skipped: "⏭ Skipped", missing: "❔ Missing", unknown: "❔ Unknown" };
function runIdentity(env) {
  // Only validated runner identities enter Markdown; no PR titles, refs or outputs.
  const repository = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(env.GITHUB_REPOSITORY ?? "") ? env.GITHUB_REPOSITORY : null;
  const sha = /^[a-f0-9]{40,64}$/.test(env.GITHUB_SHA ?? "") ? env.GITHUB_SHA : null;
  const run = /^\d+$/.test(env.GITHUB_RUN_ID ?? "") ? env.GITHUB_RUN_ID : null;
  const attempt = /^\d+$/.test(env.GITHUB_RUN_ATTEMPT ?? "") ? env.GITHUB_RUN_ATTEMPT : null;
  const server = /^https:\/\/[A-Za-z0-9.-]+(?::\d+)?$/.test(env.GITHUB_SERVER_URL ?? "") ? env.GITHUB_SERVER_URL : null;
  const url = server && repository ? `${server}/${repository}` : null;
  return { revision: sha ? url ? `[\`${sha.slice(0, 12)}\`](${url}/commit/${sha})` : `\`${sha.slice(0, 12)}\`` : "Unavailable",
    run: run && url ? `[Open run, logs and artifacts](${url}/actions/runs/${run})${attempt ? ` · attempt ${attempt}` : ""}` : "Unavailable" };
}

export function summaryMarkdown(report, env) {
  const identity = runIdentity(env), passed = report.rows.filter((row) => row.result === "success").length;
  return ["## CI validation", "", report.passed ? "**✅ Ready — all required validations passed.**" : "**❌ Blocked — required validation evidence is incomplete or unsuccessful.**", "",
    `Revision: ${identity.revision} · ${passed}/${report.rows.length} required checks passed`, "", identity.run, "",
    "| Validation | Platforms | Result |", "| :--- | :--- | :--- |",
    ...report.rows.map((row) => `| ${row.label} | ${row.platform} | ${labels[row.result]} |`), "",
    ...(report.inputIssue ? [`**Configuration error:** ${report.inputIssue}`, ""] : []),
    "Skipped, cancelled, missing and unknown results block this gate. Matrix rows show the combined job result; inspect child checks for platform details.", "",
    "Artifacts, when produced: **app-css** and **rbit-linux-x86_64** (server and runner). Availability follows the producing job; a failed run does not imply an artifact exists.", "",
    "Timing and cache statistics remain in individual job logs; this summary does not infer unreported measurements.", ""].join("\n");
}

export function annotations(report) {
  return [...(report.inputIssue ? ["::error title=CI required configuration::Required dependency metadata is invalid or incomplete."] : []),
    ...report.rows.filter((row) => row.result !== "success").map((row) => `::error title=CI required - ${row.id}::${row.label}: ${row.result}. Open the dependency check for details; all required checks must succeed.`)];
}
