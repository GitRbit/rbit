/** Retired artifact/cache store paths must not return; see docs/design/rewrite/actions-artifacts-caches-acceptance.md. */

// Uploads, quotas, eviction, retention and download conversion are decided in rbit-actions
// (archives, artifacts, caches, downloads). Services only compose them over storage and files.
const OWNER = "crates/rbit-actions/";
const PORTS = ["crates/rbit-services/src/actions/archive_files.rs", "crates/rbit-services/src/actions/artifact_ports.rs", "crates/rbit-services/src/actions/cache_ports.rs"];
const ACTIONS = "crates/rbit-services/src/actions/";
const SERVER = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/"];
// [pattern, message, roots the rule applies below]
const RULES = [
  [/\.actions_artifacts\(\)\s*\.\s*(?:record|delete|expired)\s*\(/u,
    "artifact row write outside the rbit-actions artifact store; use ArtifactStorage", SERVER],
  [/\.actions_caches\(\)\s*\.\s*(?:record|delete|least_used|unused_since|touch|exact|newest_with_prefix)\s*\(/u,
    "cache row access outside the rbit-actions shared cache; use SharedCache", SERVER],
  [/\bfn\s+(?:evict_caches|remove_cache|latest_per_name|receive)\b|\b(?:const|static)\s+(?:EVICT_ENTRIES|SWEEP_ARTIFACTS|ORPHAN_AGE)\b/u,
    "artifact/cache store decision redefined; use rbit_actions::{archives, artifacts, caches}", [ACTIONS]],
  [/max_artifact_storage_bytes\s*\.\s*saturating_sub|\.clamp\(\s*1\s*,[^)]*artifact_retention_days|cache_retention_days\)\s*\*/u,
    "artifact quota or retention computed outside rbit-actions", SERVER],
  [/\bfn\s+(?:to_zip|to_tar_gz|entry_name)\b|\bstruct\s+ZipWriter\b|\bZIP64_ENTRY\b/u,
    "artifact download conversion redefined; use rbit_actions::downloads", SERVER],
  [/\btokio::fs::(?:rename|remove_file|OpenOptions)\b/u,
    "store file handling belongs to the archive file adapter", ["crates/rbit-services/src/actions/artifacts.rs", "crates/rbit-services/src/actions/caches.rs"]],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Unit-test modules conventionally close a production file; tests may seed stores. */
function production(source) {
  const tests = source.search(/#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s/u);
  return tests < 0 ? source : source.slice(0, tests);
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsArtifactsCachesRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || path.startsWith(OWNER) || PORTS.includes(path)) continue;
    const text = production(code(source));
    for (const [pattern, message, roots] of RULES) {
      if (roots.some((root) => path.startsWith(root)) && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
