/** Retired edit decisions must not return; see docs/design/rewrite/repositories-edits-acceptance.md. */

// Who may commit and where is decided by rbit_repositories::edits; its refusal texts
// appear in production code nowhere else.
const POLICY_TEXT = /was chosen twice|files can be uploaded at once|choose at least one file to upload|enter a name for the new branch|is not a valid branch name|a branch called \{name\}|is a mirror and cannot be edited/u;
const POLICY_OWNER = "crates/rbit-repositories/src/";
// The service keeps one refusal path: no local copies of the gate classification.
const SERVICE = "crates/rbit-services/src/edits.rs";
const LOCAL_POLICY = /\bfn writable\b[^}]*can_write\(\)|\bCommitTarget::NewBranch\(_\) => EditFailure::|\bis_archived\(\)/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryEditsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || !path.endsWith(".rs") || path.startsWith(POLICY_OWNER)) continue;
    const text = code(source);
    if (POLICY_TEXT.test(text)) {
      violations.push(`${path}: browser edit policy text belongs to rbit_repositories::edits`);
    }
    if (path === SERVICE && LOCAL_POLICY.test(text.split("#[cfg(test)]")[0])) {
      violations.push(`${path}: edit access or gate classification returned to the service; use RepositoryEdits`);
    }
  }
  return violations;
}
