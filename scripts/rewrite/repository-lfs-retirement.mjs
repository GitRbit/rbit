/** Retired LFS decisions must not return; see docs/design/rewrite/repositories-lfs-acceptance.md. */

// Batch answers, upload and verify checks and lock paging live in rbit_repositories::lfs;
// their limits and refusal texts appear in production code nowhere else.
const POLICY_TEXT = /a batch can name at most|the size cannot be negative|the upload is larger than the announced|but the object has \{|the content does not match the object id|bytes, not \{|the cursor is not valid/u;
const LIMITS = /\b(?:const|static)\s+(?:MAX_BATCH_OBJECTS|MAX_LOCKS_PER_PAGE)\b/u;
// The service maps decisions; it does not re-decide batch outcomes per object.
const SERVICE_DECISIONS = /\(LfsOperation::(?:Upload|Download),\s*(?:Some|None|_)/u;
const POLICY_OWNER = "crates/rbit-repositories/src/";

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryLfsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || !path.endsWith(".rs") || path.startsWith(POLICY_OWNER)) continue;
    const text = code(source);
    if (POLICY_TEXT.test(text) || LIMITS.test(text)) {
      violations.push(`${path}: LFS limits and refusal texts belong to rbit_repositories::lfs`);
    }
    if (SERVICE_DECISIONS.test(text)) {
      violations.push(`${path}: per-object batch decision returned outside rbit_repositories::lfs::decide_batch`);
    }
  }
  return violations;
}
