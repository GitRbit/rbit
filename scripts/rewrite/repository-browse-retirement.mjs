/** Retired browse decisions must not return; see docs/design/rewrite/repositories-browse-acceptance.md. */

// Ref splitting, landing revision, tree filtering, path arithmetic, paging and image
// types are decided by rbit_repositories::browse; web routes call it.
const LIMITS = /\bconst\s+(?:COMMITS_PER_PAGE|MAX_RAW_BYTES)\b/u;
const LOCAL_HELPERS = /\bfn\s+(?:join_path|parent_path|file_name|matching_entries|image_content_type)\b/u;
const SPLIT_RULE = /strip_prefix\(r\.name\.as_str\(\)\)/u;
const ROUTES = /^crates\/rbit-web\/src\/routes\/repo(?:\.rs|\/)/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryBrowseRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!ROUTES.test(path)) continue;
    const text = code(source);
    if (LIMITS.test(text) || LOCAL_HELPERS.test(text) || SPLIT_RULE.test(text)) {
      violations.push(`${path}: browse decision returned to the web routes; use rbit_repositories::browse`);
    }
  }
  return violations;
}
