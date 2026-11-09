/** Retired workflow discovery paths must not return; see docs/design/rewrite/actions-discovery-language-acceptance.md. */

// Workflow discovery and run planning are decided in rbit-workflow (`discovery`, `plan::plan_run`).
// These adapters only list trees, read blobs and map errors.
const ADAPTERS = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/", "crates/rbit-actions/src/"];
// Unit-test modules build fixtures by parsing and planning workflows directly.
const TEST_MODULE = /\/(?:[a-z_]+_)?tests\.rs$/u;
const ACTIONS = "crates/rbit-services/src/actions/";
// [pattern, message, root the rule applies below]
const RULES = [
  // A list or constant of workflow directories; a single fixture path in a test is fine.
  [/\[\s*"\.(?:rbit|forgejo|github)\/workflows"\s*,|\b(?:const|static)\s+\w+\s*:[^=;]*=[^;]*"\.(?:rbit|forgejo|github)\/workflows"/u,
    "workflow directory list; use rbit_workflow::discovery::WORKFLOW_DIRS", ""],
  [/\bMAX_WORKFLOW_FILES\s*:/u, "workflow file cap redefined; use rbit_workflow::discovery::MAX_WORKFLOW_FILES", ""],
  [/\bextension\s*==\s*"ya?ml"/u, "workflow file filter; use rbit_workflow::discovery::is_workflow_file_name", ""],
  [/\brbit_workflow::parse\s*\(|\bparse::parse\s*\(/u, "direct workflow parse; read files with rbit_workflow::discovery::load", ""],
  [/\b(?:plan_with|concurrency_group)\s*\(|\brbit_workflow::(?:plan::)?(?:\{[^}]*)?\b(?:plan_with|concurrency_group)\b/u,
    "direct run planning; use rbit_workflow::plan::plan_run", ""],
  [/\bWORKFLOW_DIRECTORIES\b/u, "retired workflow directory alias", ""],
  [/\bfn\s+(?:read_workflow_file|discover|discover_one|plan_workflow)\b/u, "retired discovery function redefined in Actions services", ACTIONS],
  [/\bstruct\s+(?:WorkflowFile|WorkflowFileName|Plan)\b/u, "retired discovery type redefined in Actions services", ACTIONS],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsDiscoveryRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !ADAPTERS.some((root) => path.startsWith(root)) || TEST_MODULE.test(path)) continue;
    const text = code(source);
    for (const [pattern, message, root] of RULES) {
      if (path.startsWith(root) && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
