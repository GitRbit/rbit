/** Retired star and contribution paths must not return; see docs/design/rewrite/repositories-social-acceptance.md. */

// Star rules live in rbit_repositories::stars; storage implements its port. Production
// code reaches the star store only through the service composition module.
const STAR_STORE = /\bsqlite::stars\b|\bStarStore\b/u;
const OPEN_STORE = /\.stars\(\)/u;
const STORAGE = "crates/rbit-storage/";
const SERVICE = "crates/rbit-services/src/stars.rs";
// Contribution selection, the private-email rule and day tallying live in
// rbit_repositories::contributions; the service keeps scans and caches only.
const CONTRIBUTION_LISTING = /\bcontribution_repositories\b/u;
const CONTRIBUTION_SERVICE = "crates/rbit-services/src/contributions.rs";
const TALLY = /\beq_ignore_ascii_case\b|\bdiv_euclid\b|\brem_euclid\b|\b86_?400\b|\bHashSet\b|\bis_organization\(/u;

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
    if (CONTRIBUTION_LISTING.test(text)) {
      violations.push(`${path}: contribution listing outside storage; use rbit_repositories::contributions::repositories`);
    }
    // The in-file test module may use calendar constants for its fixtures.
    if (path === CONTRIBUTION_SERVICE && TALLY.test(text.split("#[cfg(test)]")[0])) {
      violations.push(`${path}: contribution selection or tallying returned to the service; use rbit_repositories::contributions`);
    }
  }
  return violations;
}
