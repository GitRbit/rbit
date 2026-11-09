/** Retired administration decisions must not return; see docs/design/rewrite/operations-admin-acceptance.md. */

const SERVICE = "crates/rbit-services/src/admin.rs";
const STORE = "crates/rbit-storage/src/sqlite/admin.rs";
// Listing and moderation types are owned by rbit-access; the legacy files re-export them.
const OWNED = /\b(?:struct|enum)\s+(?:Moderation|Listing|ListFilter|ListQuery|Paged)\b|\bconst\s+(?:PAGE_SIZE|MAX_SEARCH_LEN)\b/u;
// Decisions moved out of the service adapter: guards, confirmations, refusals, descriptions.
const DECISION = /\bfn\s+(?:refuse_self|check_moderation|explain)\b|\bModeration::|confirmation\.trim\(\)|\bold\s*!=\s*new\b|\bis_admin\s*\{\s*Ok/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkAdministrationRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (path !== SERVICE && path !== STORE) continue;
    const text = code(source);
    if (OWNED.test(text)) violations.push(`${path}: administration listing and moderation types are owned by rbit-access`);
    if (path === SERVICE && DECISION.test(text)) violations.push(`${path}: administration decision belongs in rbit_access::administration`);
  }
  return violations;
}
