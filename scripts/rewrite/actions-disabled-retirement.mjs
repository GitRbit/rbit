/** Disabled Actions must stay decided in one place; see docs/design/rewrite/actions-disabled-acceptance.md. */

// Whether Actions is on is read only through rbit_actions::availability::Availability::of;
// everything else follows the presence of the Actions service it provides.
const OWNER = "crates/rbit-actions/";
const SERVER = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/"];
const READ = /\.actions\.enabled\b(?!\s*=[^=])/gu;
const THROUGH_GATE = /Availability::of\(\s*[\w.]*\.actions\.enabled\s*\)/gu;

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
export function checkActionsDisabledRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || path.startsWith(OWNER) || !SERVER.some((root) => path.startsWith(root))) continue;
    const text = production(code(source));
    const reads = (text.match(READ) ?? []).length, gated = (text.match(THROUGH_GATE) ?? []).length;
    if (reads > gated) violations.push(`${path}: [actions] enabled read outside rbit_actions::availability::Availability::of`);
    if (/needs Actions, which this instance does not have/u.test(text)) {
      violations.push(`${path}: disabled-Actions refusal text outside rbit_actions::availability`);
    }
  }
  return violations;
}
