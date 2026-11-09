/** Retired profile paths must not return; see docs/design/rewrite/identity-profile-acceptance.md. */

// Credential compare-and-set writes belong to the identity profile application.
const CAS_CALL = /\.(?:set_email_if_password|rename_if_password|set_password_hash_if_current)\(/u;
const OWNERS = ["crates/rbit-identity/", "crates/rbit-identity-models/", "crates/rbit-storage/"];
// The service adapter supplies throttle and effects; it never verifies or hashes itself.
const PASSWORD_WORK = /\bpassword_work\(\)\s*\.begin\(|\.verify_and_hash\(|\.verify\(/u;
// Profile policy constants and user-facing rejection text are identity-owned.
const PROFILE_POLICY = /\bconst\s+MAX_(?:DISPLAY_NAME|BIO|WEBSITE|LOCATION)_LEN\b|from your current password|must be a URL starting with http/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);
const profileAdapter = (path) => /^crates\/rbit-services\/src\/profiles(?:\.rs|\/)/u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkProfileRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !production(path) || OWNERS.some((owner) => path.startsWith(owner))) continue;
    const text = code(source);
    if (CAS_CALL.test(text)) {
      violations.push(`${path}: credential compare-and-set write bypasses the identity profile application`);
    }
    if (profileAdapter(path) && PASSWORD_WORK.test(text)) {
      violations.push(`${path}: profile password checks run only in rbit_identity::profiles`);
    }
    if (path.startsWith("crates/rbit-services/src/") && PROFILE_POLICY.test(text)) {
      violations.push(`${path}: profile limits and rejection text are owned by rbit_identity::profiles`);
    }
  }
  return violations;
}
