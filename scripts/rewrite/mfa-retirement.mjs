/** Retired authenticator and recovery-code paths must not return; see docs/design/rewrite/identity-mfa-acceptance.md. */

// The TOTP algorithm and the recovery-code format are defined once, by identity.
const DEFINITION = /\b(?:struct|enum|type)\s+(?:TotpSecret|RecoveryCode)\b/u;
const DEFINITION_OWNER = "crates/rbit-identity/src/two_factor/";
// Generating, restoring or parsing secrets and codes belongs to the identity application.
const SECRET_USE = /\b(?:TotpSecret|RecoveryCode)::(?:generate|from_bytes|parse)\b/u;
// Enrolment and recovery-code writes go through the identity application.
const STORE_WRITE = /\.store\s*\.(?:begin_enrolment|cancel_enrolment|confirm|replace_recovery_codes)\(/u;
// The code count and the enrolment rejection texts are identity-owned.
const POLICY = /\bconst\s+RECOVERY_CODE_COUNT\b|authenticator app is already set up|authenticator setup (?:in progress|changed)|two-factor authentication is not enabled/u;
const OWNERS = ["crates/rbit-identity/", "crates/rbit-identity-models/", "crates/rbit-storage/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path) && !/\/tests(?:\/|\.rs$)/u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkMfaRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (!path.startsWith(DEFINITION_OWNER) && DEFINITION.test(text)) {
      violations.push(`${path}: TOTP secrets and recovery codes are defined only by rbit_identity::two_factor`);
    }
    if (!production(path) || OWNERS.some((owner) => path.startsWith(owner))) continue;
    if (SECRET_USE.test(text)) {
      violations.push(`${path}: authenticator secrets and recovery codes are handled only by rbit_identity::two_factor`);
    }
    if (STORE_WRITE.test(text)) {
      violations.push(`${path}: enrolment or recovery-code write bypasses the identity two-factor application`);
    }
    if (path.startsWith("crates/rbit-services/src/") && POLICY.test(text)) {
      violations.push(`${path}: recovery-code count and enrolment rejection text are owned by rbit_identity::two_factor`);
    }
  }
  return violations;
}
