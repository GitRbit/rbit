/** Retired passkey paths must not return; see docs/design/rewrite/identity-passkeys-acceptance.md. */

// The WebAuthn relying party and its error are defined once, by identity.
const DEFINITION = /\b(?:struct|enum|type)\s+(?:RelyingParty|WebauthnError)\b/u;
const DEFINITION_OWNER = "crates/rbit-identity/src/passkeys/";
// Verifying registrations and reading assertions belong to the identity passkey application.
const CEREMONY = /\.verify_registration\(|\bassertion_credential_id\b|\.store\s*\.add_passkey\(/u;
// Passkey limits, challenge lifetimes and registration texts are identity-owned.
const POLICY =
  /\bconst\s+(?:MAX_PASSKEYS|MAX_PASSKEY_NAME|MAX_RESPONSE_LEN|CHALLENGE_TTL|CHALLENGES_PER_ACCOUNT)\b|passkey request expired|couldn't be verified|passkey is already registered/u;
const OWNERS = ["crates/rbit-identity/", "crates/rbit-identity-models/", "crates/rbit-storage/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path) && !/\/tests(?:\/|\.rs$)/u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkPasskeyRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (!path.startsWith(DEFINITION_OWNER) && DEFINITION.test(text)) {
      violations.push(`${path}: the WebAuthn relying party is defined only by rbit_identity::passkeys`);
    }
    if (!production(path) || OWNERS.some((owner) => path.startsWith(owner))) continue;
    if (CEREMONY.test(text)) {
      violations.push(`${path}: passkey registration or assertion bypasses the identity passkey application`);
    }
    if (path.startsWith("crates/rbit-services/src/") && POLICY.test(text)) {
      violations.push(`${path}: passkey limits, challenge lifetimes and rejection text are owned by rbit_identity::passkeys`);
    }
  }
  return violations;
}
