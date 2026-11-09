/** Retired pull request model, creation and merge-gate paths must not return; see docs/design/rewrite/collaboration-pulls-progress.md. */

const OWNER = "crates/rbit-collaboration/src/";
// Branch-name and head-repository decisions are rbit_collaboration::pulls::{branch_ref, select_head};
// rbit-git keeps its own ref validation for the refs it writes.
const SERVICES = "crates/rbit-services/src/";
const DECISIONS = /\bfn\s+(?:branch_ref|select_head)\b/u;
// Opening policy text is owned by rbit_collaboration::pulls::PullCreation.
// (Base changes and reopening share some refusals and keep their own owners.)
// Review submission/dismissal and base-change refusals are owned by rbit_collaboration::pulls too.
const POLICY_TEXT = /pull requests cannot be opened against a mirror|choose existing branches to compare|already contains every commit of|you cannot approve or request changes|write a comment or leave line comments first|say why the review is dismissed|only approvals and change requests can be dismissed|The base branch changed from|"choose an existing branch"/u;
// Only storage and the creation adapter see the capped insert's outcome.
const INSERT_OUTCOME = /\bCreatedPull::(?:Created|Duplicate)\b/u;
const INSERT_OWNERS = ["crates/rbit-storage/", "crates/rbit-services/src/pulls/creation.rs"];
// Pull request records and merge requirements are defined once, in rbit_collaboration::pulls.
const MODELS = /\b(?:struct|enum)\s+(?:PullRequest|Mergeability|ChecksStatus|MergeStrategy|ReviewState|ReviewTally|Requirement|MergeFacts|MergeEvaluation|CommitIdentity|HeadSpec)\b|\bfn\s+(?:noreply_domain|default_merge_title|default_squash_title)\b/u;
// `evaluate` and `tally` are common names elsewhere; the old model file must only re-export them.
const CORE_MODEL = "crates/rbit-core/src/pull.rs";
const CORE_DECISIONS = /\bfn\s+(?:evaluate|tally)\b/u;
// Merge gating stays with rbit_collaboration::pulls::{current_mergeability, merge_gate, merge_message}.
const MERGING = "crates/rbit-services/src/pulls/merging.rs";
const MERGE_POLICY = /mergeable_base_sha\s*!=|can_merge_with_bypass\(\)|this pull request cannot be merged|input\.title\.trim\(\)\.is_empty\(\)/u;

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
      violations.push(`${path}: pull request opening, review and base-change policy text belongs to rbit_collaboration::pulls`);
    }
    if (!path.startsWith(OWNER) && (MODELS.test(text) || (path === CORE_MODEL && CORE_DECISIONS.test(text)))) {
      violations.push(`${path}: pull request records and merge requirements belong to rbit_collaboration::pulls`);
    }
    if (path === MERGING && MERGE_POLICY.test(text)) {
      violations.push(`${path}: merge gating returned to the merge service; use rbit_collaboration::pulls`);
    }
    if (!INSERT_OWNERS.some(owner => path.startsWith(owner)) && INSERT_OUTCOME.test(text)) {
      violations.push(`${path}: pull request insert handled outside the PullCreation adapter`);
    }
  }
  return violations;
}
