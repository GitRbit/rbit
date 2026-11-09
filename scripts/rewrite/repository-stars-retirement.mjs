/** Retired star paths must not return; see docs/design/rewrite/repositories-social-acceptance.md. */

// Star rules live in rbit_repositories::stars; storage implements its port. Production
// code reaches the star store only through the service composition module.
const STAR_STORE = /\bsqlite::stars\b|\bStarStore\b/u;
const OPEN_STORE = /\.stars\(\)/u;
const STORAGE = "crates/rbit-storage/";
const SERVICE = "crates/rbit-services/src/stars.rs";

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryStarsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || !path.endsWith(".rs") || path.startsWith(STORAGE)) continue;
    const text = code(source);
    if (STAR_STORE.test(text)) {
      violations.push(`${path}: star store type outside storage; use rbit_repositories::stars`);
    }
    if (path !== SERVICE && OPEN_STORE.test(text)) {
      violations.push(`${path}: star store opened outside the star service; use StarService`);
    }
  }
  return violations;
}
