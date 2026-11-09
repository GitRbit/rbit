/** Retired release and archive paths must not return; see docs/design/rewrite/repositories-releases-acceptance.md. */

const OWNER = "crates/rbit-repositories/src/";
const STORAGE = "crates/rbit-storage/";
// Release records, release/archive rules and their helpers are defined by the applications only.
const DEFINITION = new RegExp(String.raw`\b(?:struct|enum)\s+(?:ReleaseAsset|ReleaseAuthor|ReleaseSource|ReleaseId|ReleaseAssetId|NewRelease|ReleaseChanges|NewAsset|ReleaseInput|ReleaseDetail|ArchiveError|ArchiveRefusal)\b`
  + String.raw`|\bfn\s+(?:validate_asset_name|validate_release_name|validate_release_notes|human_bytes|archive_base_name)\b`
  + String.raw`|\bconst\s+(?:MAX_CONCURRENT_ARCHIVES|RELEASES_PER_PAGE)\b`, "u");
// Release policy text is owned by rbit_repositories::releases.
const POLICY_TEXT = /choose the tag this release is for|a release can have at most|there is no tag called|there is no branch or commit called|push it or choose a branch/u;
// Release persistence is reached through the release adapter; webhooks read payloads and imports copy releases (their own rows).
const STORE = /\bdatabase\s*\.\s*releases\s*\(\s*\)/u;
const STORE_USERS = [/^crates\/rbit-services\/src\/releases(?:\.rs|\/)/u, /^crates\/rbit-services\/src\/webhooks\.rs$/u, /^crates\/rbit-services\/src\/imports\//u];
// The release service composes effects only: no visibility, permission, validation or limit decisions.
const RELEASE_ADAPTER = /^crates\/rbit-services\/src\/releases(?:\.rs|\/)/u;
const RELEASE_POLICY = /\bis_draft\b|\bvalidate_(?:asset_name|release_name|release_notes)\b|\bcount_assets\b|\bfind_by_tag\b|\bensure_writable\b|\bErr\(\s*ServiceError::(?:Forbidden|Invalid|Conflict|Archived)\b/u;
// The archive service supplies slots and Git only; admission order and naming are the application's.
const ARCHIVE_ADAPTER = "crates/rbit-services/src/archives.rs";
const ARCHIVE_POLICY = /\bimpl\s+From<\s*Refused\s*>|\bthiserror\b|\bsanitized\b|\btrim_matches\b/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** The adapter's unit-test module seeds rows directly; rules apply to the code above it. */
function beforeTests(text) {
  const index = text.search(/#\[cfg\(test\)\]\s*mod\s+tests\b/u);
  return index < 0 ? text : text.slice(0, index);
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryReleasesRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (!path.startsWith(OWNER) && DEFINITION.test(text)) {
      violations.push(`${path}: release/archive records and rules are defined in rbit_repositories::{releases, archives}`);
    }
    if (production(path) && !path.startsWith(OWNER) && POLICY_TEXT.test(text)) {
      violations.push(`${path}: release policy text belongs to rbit_repositories::releases`);
    }
    if (production(path) && !path.startsWith(STORAGE) && !STORE_USERS.some((users) => users.test(path)) && STORE.test(text)) {
      violations.push(`${path}: release persistence bypasses ReleaseService and rbit_repositories::releases`);
    }
    if (RELEASE_ADAPTER.test(path) && RELEASE_POLICY.test(beforeTests(text))) {
      violations.push(`${path}: release policy returned to the service adapter; use rbit_repositories::releases::Releases`);
    }
    if (path === ARCHIVE_ADAPTER && ARCHIVE_POLICY.test(text)) {
      violations.push(`${path}: archive admission or naming returned to the service adapter; use RepositoryArchives`);
    }
  }
  return violations;
}
