/** The retired second sign-in step must not return; see docs/design/rewrite/identity-mfa-acceptance.md. */

// The pending owner, the proof types and the step are defined once, by identity.
const DEFINITION = /\b(?:struct|enum|type)\s+(?:PendingSignIn|PendingSignIns|SecondFactorProof|SecondFactorStep)\b/u;
const DEFINITION_OWNER = /^crates\/rbit-identity\/src\/second_factor(?:\.rs|\/)/u;
// One-use claim writes are reached only through the identity step's claim port adapter.
const CLAIM_WRITE = /\.(?:claim_step|spend_recovery_code|use_passkey)(?:_at_epoch)?\(/u;
const CLAIM_ADAPTER = "crates/rbit-services/src/two_factor/verification.rs";
// Pending lifetimes and attempt limits are identity-owned.
const POLICY = /\bconst\s+(?:PENDING_SIGN_IN_TTL|PENDING_SIGN_IN_ATTEMPTS|MAX_PENDING_SIGN_INS)\b/u;
const OWNERS = ["crates/rbit-identity/", "crates/rbit-identity-models/", "crates/rbit-storage/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path) && !/\/tests(?:\/|\.rs$)/u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkSecondFactorRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (!DEFINITION_OWNER.test(path) && DEFINITION.test(text)) {
      violations.push(`${path}: the pending second step is defined only by rbit_identity::second_factor`);
    }
    if (!production(path) || OWNERS.some((owner) => path.startsWith(owner))) continue;
    if (path !== CLAIM_ADAPTER && CLAIM_WRITE.test(text)) {
      violations.push(`${path}: one-use proof claims bypass the identity second-factor step`);
    }
    if (path.startsWith("crates/rbit-services/src/") && POLICY.test(text)) {
      violations.push(`${path}: pending lifetimes and attempt limits are owned by rbit_identity::second_factor`);
    }
  }
  return violations;
}
