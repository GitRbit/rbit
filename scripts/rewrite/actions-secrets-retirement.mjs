/** Retired secret and environment rules must not return; see docs/design/rewrite/actions-secrets-acceptance.md. */

// Secret limits, scoping and opening, and environment protection rules, are decided in
// rbit-actions (`secrets`, `environments`). Sealing, storage and the SQL gates are adapters.
const OWNER = "crates/rbit-actions/";
const SOURCES = /^crates\/[^/]+\/src\//u;
// The REST API refuses a `visibility` field on repository secrets with the same words; that
// is a request-shape rule of the API, so value and scope rules are checked in the services
// and server adapters only.
const ADAPTERS = ["crates/rbit-services/", "crates/rbit-server/"];
// [pattern, message, adapters only]
const RULES = [
  // `BranchPolicy` stays the stored record in rbit-core, and the environment store keeps its
  // defensive reviewer cap; rbit-actions reads the policy through `RefPolicy`.
  [/\bconst\s+(?:MAX_PER_SCOPE|MAX_VARIABLE_BYTES|MAX_WAIT_TIMER_MINUTES|UNREADABLE)\s*:/u, "secret or environment limit redefined"],
  [/"a repository secret is visible to its repository only"|\{kind\} needs a value|\{kind\} must not contain NUL/u, "secret value or scope rule decided outside rbit_actions::secrets", true],
  [/is not allowed to deploy to |does not let whoever started a run approve it|"an environment has at most |"the wait timer is at most /u,
    "environment protection rule decided outside rbit_actions::environments"],
  [/refs\/heads\/\{(?:pattern|bang)/u, "selected pattern rewriting outside rbit_actions::environments::full_ref_patterns"],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsSecretsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !SOURCES.test(path) || path.startsWith(OWNER)) continue;
    const text = code(source);
    for (const [pattern, message, adaptersOnly] of RULES) {
      if (adaptersOnly && !ADAPTERS.some((root) => path.startsWith(root))) continue;
      if (pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
