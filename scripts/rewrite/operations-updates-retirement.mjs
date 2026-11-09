/** The retired services release checker must not return; see docs/design/rewrite/operations-updates-acceptance.md. */

// Release checks are decided and composed in rbit-server (`src/updates`); services keep
// only the UpdateNotices snapshot that pages read.
const CHECKER = "crates/rbit-server/src/updates";
const SNAPSHOT = "crates/rbit-services/src/updates.rs";
const ADAPTERS = ["crates/rbit-services/", "crates/rbit-web/", "crates/rbit-ssh/", "crates/rbit-e2e/"];
// [pattern, message, applies to path]
const RULES = [
  [/api\.github\.com\/repos\/GitRbit\/rbit\/releases|"rbit-update-check"/u,
    "release feed or check User-Agent outside the server release checker", (path) => !path.startsWith(CHECKER)],
  [/\bUpdateService\b/u, "retired services update checker", () => true],
  [/\bfn\s+(?:newer_release|failure_delay)\b|\bcmp_precedence\s*\(/u,
    "release decision outside rbit-server updates", (path) => ADAPTERS.some((root) => path.startsWith(root))],
  [/\breqwest\b|\bsemver\b|\btokio\b|\bspawn\w*\s*\(|\bCancellationToken\b|\bTaskTracker\b/u,
    "the services update snapshot performs no checks or tasks", (path) => path === SNAPSHOT],
  [/\.updates\s*\.\s*(?:spawn_background|shutdown)\s*\(/u,
    "update checks are composed by the server; start them with rbit-server updates::start", () => true],
];

/** Comments are not code; strip line and block comments, keeping `scheme://` addresses. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/(?<!:)\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkOperationsUpdatesRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !path.startsWith("crates/")) continue;
    const text = code(source);
    for (const [pattern, message, applies] of RULES) {
      if (applies(path) && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
