/** Retired lifecycle and listing paths must not return; see docs/design/rewrite/repositories-lifecycle-acceptance.md. */

// Repository listings and their limits are decided by rbit_repositories::discovery;
// only storage runs the listing statements.
const LISTING = /\.(?:list_by_owner|list_for_member|count_for_member|list_recent_public)\(|\bstore\.page_for_member\(|\bconst DASHBOARD_LIMIT\b/u;
// Lenient explore query parsing lives with RepoSearch.
const QUERY_PARSING = /\b(?:RepoSort|ArchivedFilter)::parse\b/u;
// Administrator deletion goes through AdminRepositoryDeletion and its effect adapter.
const ADMIN_DELETE = /\bstore\.delete_repository\(/u;
const ADMIN_ADAPTER = "crates/rbit-services/src/admin/repository_deletion.rs";
const POLICY_TEXT = /type the repository's full name exactly to confirm/u;
const POLICY_OWNER = "crates/rbit-repositories/src/";
const STORAGE = "crates/rbit-storage/";

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryLifecycleRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || !path.endsWith(".rs") || path.startsWith(STORAGE) || path.startsWith(POLICY_OWNER)) continue;
    const text = code(source);
    if (LISTING.test(text)) {
      violations.push(`${path}: repository listing bypasses rbit_repositories::discovery::RepositoryListings`);
    }
    if (path.startsWith("crates/rbit-web/") && QUERY_PARSING.test(text)) {
      violations.push(`${path}: explore query parsing belongs to RepoSearch::from_query`);
    }
    if (path !== ADMIN_ADAPTER && ADMIN_DELETE.test(text)) {
      violations.push(`${path}: administrator deletion bypasses AdminRepositoryDeletion`);
    }
    if (POLICY_TEXT.test(text)) {
      violations.push(`${path}: repository deletion policy text belongs to rbit_repositories::settings`);
    }
  }
  return violations;
}
