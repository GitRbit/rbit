/** Retired external sign-in primitives must not return; see docs/design/rewrite/identity-external-progress.md. */

// Flow sealing, PKCE and ID-token validation are defined once, by identity.
const DEFINITION = /\b(?:struct|enum|type)\s+(?:FlowKey|Pkce|JwkSet|IdTokenCheck|OidcError)\b|\bfn\s+verify_id_token\b/u;
const DEFINITION_OWNER = /^crates\/rbit-identity\/src\/oidc(?:\.rs|\/)/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkOidcRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || DEFINITION_OWNER.test(path)) continue;
    if (DEFINITION.test(code(source))) {
      violations.push(`${path}: external sign-in flow, PKCE and ID-token checks are defined only by rbit_identity::oidc`);
    }
  }
  return violations;
}
