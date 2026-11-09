/** Retired runner-contract copies must not return; see docs/design/rewrite/actions-backends-acceptance.md. */

// Cache, artifact and backend decisions shared by the server and rbit-runner are decided once,
// in the runner contract (rbit_workflow::protocol and rbit_workflow::spec::CacheScopes).
const CONTRACT = "crates/rbit-workflow/";
// Server-side adapters: they apply the contract and own storage, never a second copy of it.
const SERVER = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/", "crates/rbit-actions/src/", "crates/rbit-storage/src/"];
// [pattern, message, roots the rule applies below ("" is every crate but the contract)]
const RULES = [
  [/\b(?:const|static)\s+MAX_(?:CACHE_)?KEY_CHARS\b|\b(?:const|static)\s+MAX_RESTORE_KEYS\b/u,
    "cache key limit redefined; use rbit_workflow::protocol::{MAX_CACHE_KEY_CHARS, MAX_RESTORE_KEYS}", [""]],
  [/\bfn\s+(?:valid_key|valid_version|scope_ref|ref_to_scope|ref_scope|digest_from_hex)\b/u,
    "cache/artifact contract rule redefined; use rbit_workflow::protocol", [""]],
  [/\bfor\s+prefix\s+in\s+(?:&?\w+\.)?restore_keys\b/u,
    "cache restore order redefined; iterate rbit_workflow::protocol::restore_probes", [""]],
  [/\bCacheScopes\s*\{\s*(?:repository_id|read|write)\b/u, "cache scopes built outside rbit_workflow::spec::CacheScopes::for_run", SERVER],
  [/\b(?:const|static)\s+\w+\s*:\s*&(?:'static\s+)?str\s*=\s*"[wh]"\s*;/u,
    "archive directory redefined; use rbit_workflow::protocol::{ARCHIVE_WORKSPACE_DIR, ARCHIVE_HOME_DIR}", [""]],
  [/\bfilter\(\s*\|[^|]*\|[^\n]*backend\s*==\s*Backend::Host\b/u,
    "host-label classification redefined; use rbit_workflow::protocol::Backend::isolates_jobs", [""]],
  [/"--(?:privileged|security-opt|cap-drop|cap-add|pids-limit)"|"[^"\n]*docker\.sock[^"\n]*"/u,
    "container isolation arguments are built only by rbit-runner's backend", SERVER],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Unit-test modules conventionally close a production file; tests may build fixtures. */
function production(source) {
  const tests = source.search(/#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s/u);
  return tests < 0 ? source : source.slice(0, tests);
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsRunnerRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || path.startsWith(CONTRACT)) continue;
    if (!/^crates\/[^/]+\/src\//u.test(path)) continue;
    const text = production(code(source));
    for (const [pattern, message, roots] of RULES) {
      if (roots.some((root) => path.startsWith(root || "crates/")) && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
