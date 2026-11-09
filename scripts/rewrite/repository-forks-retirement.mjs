/** Retired fork-store bypasses must not return; see docs/design/rewrite/repositories-forks-acceptance.md. */

// Fork flags, parents, listings and the fork network are read through ForkQueries; only
// storage and the service fork module (its read store and relationship effect) open
// the fork store.
const FORK_STORE = /\.forks\(\)|\bsqlite::forks\b|\bForkStore\b/u;
const NETWORK = /\bnetwork_readable\b/u;
// The native copy runs only inside the fork creation workflow's effects.
const NATIVE_COPY = /\bfork_repository\b/u;
const STORAGE = "crates/rbit-storage/";
const SERVICE_FORKS = /^crates\/rbit-services\/src\/forks(?:\.rs|\/)/u;
const GIT = "crates/rbit-git/";

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryForksRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || path.startsWith(STORAGE)) continue;
    const text = code(source);
    const owner = SERVICE_FORKS.test(path);
    if (!owner && FORK_STORE.test(text)) {
      violations.push(`${path}: fork store read bypasses rbit_repositories::forks::ForkQueries; use crate::forks or ForkService`);
    }
    if (!path.startsWith("crates/rbit-repositories/") && NETWORK.test(text)) {
      violations.push(`${path}: fork network read bypasses ForkQueries::network`);
    }
    if (!owner && !path.startsWith(GIT) && NATIVE_COPY.test(text)) {
      violations.push(`${path}: native fork copy outside the fork creation workflow`);
    }
  }
  return violations;
}
