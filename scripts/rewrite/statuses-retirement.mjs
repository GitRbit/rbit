/** Retired commit status paths must not return; see docs/design/rewrite/collaboration-statuses-acceptance.md. */

// Status records and decisions are defined once, in rbit_collaboration::statuses.
const OWNER = "crates/rbit-collaboration/src/";
const MODEL = /\b(?:struct|enum)\s+(?:CommitStatus|StatusState|StatusContext|StatusSource|StatusId|StatusError|RequiredChecks|CombinedStatus|StatusPage)\b|\bfn\s+(?:combined_state|required_checks|parse_sha|creator_ids)\b/u;
// Status writes go through StatusApplication; only storage runs the capped insert.
const DIRECT_WRITE = /\.commit_statuses\(\)\s*\.create(?:_as_system)?\(|\bCommitStatusStore::create(?:_as_system)?\b/u;
const STORAGE = "crates/rbit-storage/";
// The service adapter composes effects only: no store calls, checks or caps of its own.
const SERVICE_ADAPTER = /^crates\/rbit-services\/src\/statuses(?:\.rs|\/)/u;
const ADAPTER_POLICY = /\.commit_statuses\(\)\s*\.|\b(?:parse_sha|MAX_STATUSES_PER_CONTEXT|combined_state|ensure_writable|at_cap)\b/u;
// Submission policy text is owned by the application.
const POLICY_TEXT = /reached the maximum number of statuses|no commit found for SHA/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkStatusesRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || !production(path)) continue;
    const text = code(source);
    if (!path.startsWith(OWNER) && MODEL.test(text)) {
      violations.push(`${path}: commit status records and decisions belong to rbit_collaboration::statuses`);
    }
    if (!path.startsWith(STORAGE) && DIRECT_WRITE.test(text)) {
      violations.push(`${path}: direct commit status write bypasses rbit_collaboration::statuses::StatusApplication`);
    }
    if (SERVICE_ADAPTER.test(path) && ADAPTER_POLICY.test(text)) {
      violations.push(`${path}: status policy or persistence returned to the service adapter; use StatusApplication`);
    }
    if (!path.startsWith(OWNER) && POLICY_TEXT.test(text)) {
      violations.push(`${path}: status submission policy text belongs to rbit_collaboration::statuses`);
    }
  }
  return violations;
}
