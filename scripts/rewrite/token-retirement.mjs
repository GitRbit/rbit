/** Retired access token paths must not return; see docs/design/rewrite/identity-tokens-acceptance.md. */

// The secret format is defined once, by the identity token module.
const SECRET_DEFINITION = /\b(?:struct|enum|type)\s+AccessTokenSecret\b/u;
const SECRET_OWNER = "crates/rbit-identity/src/token/";
// Token policy and credential resolution belong to the identity token application.
const POLICY_LITERAL = /\b(?:EXPIRY_CHOICES|MAX_TOKENS_PER_\w+)\s*(?::[^=;]*)?=\s*(?:&\s*\[|\d)/u;
const STORE_CALL = /\.(?:create_with_limit|find_by_digest)\(|\bAccessTokenStore::(?:create_with_limit|find_by_digest|touch|delete)\b/u;
const OWNERS = ["crates/rbit-identity/", "crates/rbit-identity-models/", "crates/rbit-storage/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkTokenRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (!path.startsWith(SECRET_OWNER) && SECRET_DEFINITION.test(text)) {
      violations.push(`${path}: access token secrets are defined only by rbit_identity::token`);
    }
    if (!production(path) || OWNERS.some((owner) => path.startsWith(owner))) continue;
    if (path.startsWith("crates/rbit-services/src/") && /\bAccessTokenSecret\b/u.test(text)) {
      violations.push(`${path}: services handle access token secrets only through the identity token application`);
    }
    if (POLICY_LITERAL.test(text)) {
      violations.push(`${path}: access token limits and expiry choices are owned by rbit_identity::access_tokens`);
    }
    if (STORE_CALL.test(text)) {
      violations.push(`${path}: direct access token store call bypasses the identity token application`);
    }
  }
  return violations;
}
