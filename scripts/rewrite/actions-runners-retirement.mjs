/** Retired runner registry paths must not return; see docs/design/rewrite/actions-runners-acceptance.md. */

// Registration, declaration, token terms, manager rules and runner bookkeeping are decided in
// rbit_actions::runners. Services compose them; runner_ports.rs is the only store adapter.
const OWNER = "crates/rbit-actions/";
const PORTS = "crates/rbit-services/src/actions/runner_ports.rs";
const SERVER = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/"];
// [pattern, message, roots]
const RULES = [
  [/\bactions_runners\(\)\s*\.\s*(?:register|declare)\s*\(/u, "runner registration or declaration bypasses rbit_actions::runners::RunnerRegistry", SERVER],
  [/\b(?:const|static)\s+(?:SEEN_GRANULARITY_SECONDS|REMEMBERED_KEYS|DEFAULT_REGISTRATION_LIFETIME|MAX_REGISTRATION_LIFETIME)\b/u,
    "runner rule redefined; use rbit_actions::runners", SERVER],
  [/\b(?:struct|enum)\s+(?:Declared|RecentKeys)\b|\bfn\s+(?:from_validation|invalid_registration|labels_changed|seen_is_due)\b/u,
    "runner registry decision redefined; use rbit_actions::runners", SERVER],
  [/registration token is unknown, revoked|registration token allows other labels|registration token does not allow label|registration token lasts from a minute|registration token must allow at least one|labels, or none to allow any/u,
    "runner refusal text outside rbit_actions::runners", SERVER],
  [/\blong_poll_seconds\s*\.\s*clamp\s*\(|\bseen\s*-\s*written\b/u, "runner interval or last-seen rule outside rbit_actions::runners", SERVER],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** An inline unit-test module conventionally closes a production file; `mod x;` declarations do not. */
function production(source) {
  const tests = source.search(/#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/u);
  return tests < 0 ? source : source.slice(0, tests);
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsRunnersRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || path.startsWith(OWNER) || path === PORTS) continue;
    const text = production(code(source));
    for (const [pattern, message, roots] of RULES) {
      if (roots.some((root) => path.startsWith(root)) && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
