/** Retired scheduling decisions must not return; see docs/design/rewrite/actions-scheduling-acceptance.md. */

// Run admission caps, concurrency-group entry and release, and task lease checks are
// decided in rbit-actions (`scheduling`). The atomic SQL stays in storage.
const ADAPTERS = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/"];
const LEASE_ADAPTER = "crates/rbit-services/src/actions/task_leases.rs";
const RULES = [
  [/this event already started its share of jobs|jobs waiting for a runner; at most|\bconst\s+MAX_JOB_LABEL_CHARS\s*:/u,
    "run admission cap decided outside rbit_actions::scheduling"],
  [/\ballowance\.(?:runs|jobs)\s*-=/u, "event allowance taken outside rbit_actions::scheduling::Allowance"],
  // The release rule: held or awaiting approval, and nothing else, means no run is active.
  [/\blet\s+mut\s+busy\b|matches!\([^,()]+,\s*Status::Blocked\s*\|\s*Status::AwaitingApproval\s*\)/u,
    "concurrency group decided outside rbit_actions::scheduling::{enter_group, next_to_release}"],
  [/the job has been rerun; this task no longer belongs to it/u, "task lease decided outside rbit_actions::scheduling::leases"],
];
const LEASE_FUNCTIONS = /\bfn\s+(?:running_task|leased_task)\b/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsSchedulingRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !ADAPTERS.some((root) => path.startsWith(root))) continue;
    const text = code(source);
    for (const [pattern, message] of RULES) if (pattern.test(text)) violations.push(`${path}: ${message}`);
    if (path !== LEASE_ADAPTER && LEASE_FUNCTIONS.test(text)) {
      violations.push(`${path}: task lease check outside the rbit_actions::scheduling::leases adapter`);
    }
  }
  return violations;
}
