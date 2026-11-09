/** Retired branch protection paths must not return; see docs/design/rewrite/collaboration-protection-acceptance.md. */

// Distinctive rule and decision items are defined only in rbit-access.
const OWNED = /\b(?:(?:struct|enum)\s+(?:BranchPattern|RuleSettings|RuleError|ProtectionId|RuleChange|RuleInput|PushVerdict|PushDecision)|fn\s+(?:decide_push|merge_rules))\b/u;
// Generic names are checked in the legacy protection files only.
const LEGACY_OWNED = /\b(?:(?:struct|enum|type)\s+(?:Refusal|PatternError|Pusher|Bypass|MergeRules|ProtectionRule)\b(?!\s*(?:<[^=]*>)?\s*=\s*(?:rules|rbit_access)::)|fn\s+(?:may_delete|is_protected|apply|namespace_refusal)\b|mod\s+reasons\b)/u;
const LEGACY_FILES = ["crates/rbit-core/src/protection.rs", "crates/rbit-services/src/protection.rs", "crates/rbit-storage/src/sqlite/protection.rs"];
// Decisions moved out of the service adapter; it composes the rbit-access applications.
const ADAPTER = "crates/rbit-services/src/protection.rs";
const ADAPTER_DECISION = /\bfn\s+(?:require_admin|distinct|required_contexts|pattern_taken|team_outside|validate|writer_called|pusher|rules)\b|\bRuleChange::|(?<![.\w])(?:decide_push|merge_rules|may_delete)\s*\(/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkProtectionRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || path.startsWith("crates/rbit-access/")) continue;
    const text = code(source);
    if (OWNED.test(text) || (LEGACY_FILES.includes(path) && LEGACY_OWNED.test(text))) {
      violations.push(`${path}: branch protection rules and decisions are owned by rbit-access; re-export them instead`);
    }
    if (path === ADAPTER && ADAPTER_DECISION.test(text)) {
      violations.push(`${path}: branch protection decision belongs in rbit-access`);
    }
  }
  return violations;
}
