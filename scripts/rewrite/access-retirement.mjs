/** Retired organisation/team paths must not return; see docs/design/rewrite/access-organizations-acceptance.md. */

// Organisation and team rule types are owned by rbit-access; other crates re-export them.
const OWNED_TYPES = /\b(?:enum|struct|type)\s+(?:OrgRole|OrgCreationPolicy|MembershipChange|TeamChange|OrgProfileUpdate|CreateOrganization|UpdateOrganization|TeamInput|NewOrganization)\b(?!\s*=\s*rbit_access::)/u;
// Membership, profile, team and grant writes go through the rbit-access applications.
const STORE_WRITE = /\.(?:orgs|teams)\(\)\s*\.\s*(?:create|add_member|set_role|remove_member|update_profile|set_base_permission|delete_if_empty|update|delete|set_repository|remove_repository)\s*\(/u;
// The service adapters reach storage only through the access store adapters.
const ADAPTER_STORE = /\.(?:orgs|teams|accounts|repositories)\(\)|\b(?:OrgStore|TeamStore)\b/u;
// Decisions moved out of the service adapters.
const ADAPTER_DECISION = /\bfn\s+(?:require_manager|find_user|find_repository|validate|explain|optional_text|website|access)\b|\b(?:MembershipChange|TeamChange|OrgCreationPolicy)::/u;
const ADAPTERS = ["crates/rbit-services/src/orgs.rs", "crates/rbit-services/src/teams.rs"];
const WRITE_OWNERS = ["crates/rbit-storage/", "crates/rbit-access/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Unit-test modules conventionally close a production file; fixtures may seed stores. */
function production(source) {
  const tests = source.search(/#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s/u);
  return tests < 0 ? source : source.slice(0, tests);
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkAccessRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (!path.startsWith("crates/rbit-access/") && OWNED_TYPES.test(text)) {
      violations.push(`${path}: organisation/team rule types are owned by rbit-access; re-export them instead`);
    }
    if (!/^crates\/[^/]+\/src\//u.test(path) || WRITE_OWNERS.some((owner) => path.startsWith(owner))) continue;
    const live = production(text);
    if (STORE_WRITE.test(live)) {
      violations.push(`${path}: organisation/team store write bypasses the rbit-access rules`);
    }
    if (ADAPTERS.includes(path) && ADAPTER_STORE.test(live)) {
      violations.push(`${path}: service adapter must use the access store adapters, not raw stores`);
    }
    if (ADAPTERS.includes(path) && ADAPTER_DECISION.test(live)) {
      violations.push(`${path}: organisation/team decision belongs in rbit-access`);
    }
  }
  return violations;
}
