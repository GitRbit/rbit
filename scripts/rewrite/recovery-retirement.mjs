/** Retired recovery paths must not return; see docs/design/rewrite/identity-recovery-acceptance.md. */

// Issuing, finding and redeeming one-use links belongs to the identity recovery application.
const LINK_STORE = /\bNewEmailToken\s*\{|\.(?:find_valid|mark_verified|find_account_by_email)\(|\.consume\([^)]*TokenPurpose/u;
const OWNERS = ["crates/rbit-identity/", "crates/rbit-identity-models/", "crates/rbit-storage/"];
// The service recovery adapter neither admits password work nor hashes.
const PASSWORD_WORK = /\bpasswords\s*\.begin\(|\.hash_if_different\(|\battempt\s*\.hash\(/u;
// Link lifetimes and recovery rejection text are identity-owned.
const RECOVERY_POLICY =
  /\bconst\s+(?:RESET|VERIFY)_LIFETIME_SECONDS\b|enter a valid email address|from your initial password|lift the suspension first|email address is already verified/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path) && !/\/tests(?:\/|\.rs$)/u.test(path);
const recoveryAdapter = (path) => /^crates\/rbit-services\/src\/recovery(?:\.rs|\/)/u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRecoveryRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !production(path) || OWNERS.some((owner) => path.startsWith(owner))) continue;
    const text = code(source);
    if (LINK_STORE.test(text)) {
      violations.push(`${path}: recovery link issue or redemption bypasses the identity recovery application`);
    }
    if (recoveryAdapter(path) && PASSWORD_WORK.test(text)) {
      violations.push(`${path}: recovery password checks and hashing run only in rbit_identity::recovery`);
    }
    if (path.startsWith("crates/rbit-services/src/") && RECOVERY_POLICY.test(text)) {
      violations.push(`${path}: recovery lifetimes and rejection text are owned by rbit_identity::recovery`);
    }
  }
  return violations;
}
