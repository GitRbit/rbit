/** Retired pull request creation paths must not return; see docs/design/rewrite/collaboration-pulls-progress.md. */

const OWNER = "crates/rbit-collaboration/src/";
// Branch-name and head-repository decisions are rbit_collaboration::pulls::{branch_ref, select_head};
// rbit-git keeps its own ref validation for the refs it writes.
const SERVICES = "crates/rbit-services/src/";
const DECISIONS = /\bfn\s+(?:branch_ref|select_head)\b/u;
// Opening policy text is owned by rbit_collaboration::pulls::PullCreation.
// (Base changes and reopening share some refusals and keep their own owners.)
const POLICY_TEXT = /pull requests cannot be opened against a mirror|choose existing branches to compare|already contains every commit of/u;
// Only storage and the creation adapter see the capped insert's outcome.
const INSERT_OUTCOME = /\bCreatedPull::(?:Created|Duplicate)\b/u;
const INSERT_OWNERS = ["crates/rbit-storage/", "crates/rbit-services/src/pulls/creation.rs"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkPullsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || !production(path)) continue;
    const text = code(source);
    if (path.startsWith(SERVICES) && DECISIONS.test(text)) {
      violations.push(`${path}: branch and head selection belong to rbit_collaboration::pulls`);
    }
    if (!path.startsWith(OWNER) && POLICY_TEXT.test(text)) {
      violations.push(`${path}: pull request opening policy text belongs to rbit_collaboration::pulls::PullCreation`);
    }
    if (!INSERT_OWNERS.some(owner => path.startsWith(owner)) && INSERT_OUTCOME.test(text)) {
      violations.push(`${path}: pull request insert handled outside the PullCreation adapter`);
    }
  }
  return violations;
}
