/** Retired trigger decisions must not return to adapters; see docs/design/rewrite/actions-triggers-acceptance.md. */

// Which pushed refs, pull request events and runs count is decided in rbit-actions
// (`triggers`); the services adapter projects records and does the I/O.
const ADAPTERS = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/"];
// [pattern, message]
const RULES = [
  [/\b(?:const|static)\s+(?:MAX_REF_UPDATES|MAX_CHANGED_PATHS|EVALUATION_BUDGET|PAYLOAD_COMMITS|MAX_WORKFLOW_RUN_CHAIN|MAX_DISPATCH_EVENT_TYPE_CHARS|MAX_CLIENT_PAYLOAD_PROPERTIES|MAX_CLIENT_PAYLOAD_CHARS|TICK|PAUSE_AFTER_FAILURES|DUE_PER_TICK|RETRY_UNREADABLE)\s*:/u,
    "trigger limit redefined; use rbit_actions::triggers"],
  [/\.take\(\s*MAX_REF_UPDATES\s*\)|\bMAX_CHANGED_PATHS\s*\+\s*1\b|\.len\(\)\s*>\s*MAX_CHANGED_PATHS\b/u,
    "trigger cap applied in an adapter; use considered_refs or changed_paths_known"],
  [/\(\s*None\s*,\s*Some\(\s*branch\s*\)\s*\)\s*if\s+branch\s*!=\s*default_branch/u,
    "comparison base decided in an adapter; use comparison_base"],
  [/IssueAction::Synchronized\s*\{[^}]*\}\s*if\s+\w+\.starts_runs\(\)/u,
    "pull request synchronize gating decided in an adapter; use pull_trigger"],
  [/Status::Waiting\s*\|\s*Status::Blocked\s*\|\s*Status::AwaitingApproval/u,
    "pull request close cancellation decided in an adapter; use cancelled_when_pull_closes"],
  [/Status::Waiting\s*\|\s*Status::Blocked\s*\)/u,
    "schedule waiting rule decided in an adapter; use events::schedule_waits_for"],
  [/\bdepth\s*(?:<|>=)\s*MAX_WORKFLOW_RUN_CHAIN\b|chars\(\)\.count\(\)\s*>\s*MAX_(?:DISPATCH_EVENT_TYPE|CLIENT_PAYLOAD)_CHARS\b|\.len\(\)\s*>\s*MAX_CLIENT_PAYLOAD_PROPERTIES\b/u,
    "dispatch or chain limit applied in an adapter; use rbit_actions::triggers::events"],
  [/"the ref must be a branch or tag of this repository"|strip_prefix\("refs\/heads\/"\)[\s\S]{0,200}?starts_with\("refs\/"\)/u,
    "dispatch ref resolution decided in an adapter; use events::dispatch_ref"],
];

/** Comments are not code; strip line and block comments, keeping `scheme://` addresses. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/(?<!:)\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsTriggersRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !ADAPTERS.some((root) => path.startsWith(root))) continue;
    const text = code(source);
    for (const [pattern, message] of RULES) {
      if (pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
