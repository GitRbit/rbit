/** Retired external sign-in decisions must not return; see docs/design/rewrite/identity-external-progress.md. */

const OWNER = /^crates\/rbit-identity\/src\/external(?:\.rs|\/)/u;
const SERVICE = /^crates\/rbit-services\/src\/external_auth(?:\.rs|\/)/u;

const RULES = [
  // The decision types and helpers are defined once, by identity.
  { pattern: /\bstruct\s+(?:VerifiedIdentity|CallbackParams|Refusal|ProviderPolicy)\b|\benum\s+(?:Completion|ProviderRegistration)\b|\bfn\s+(?:suggest_username|may_register|linked_entry|truncate)\b/u,
    where: (path) => SERVICE.test(path) || path.startsWith("crates/rbit-auth/src/"), message: "external sign-in decision types and helpers are defined only by rbit_identity::external" },
  // User-facing decision text is identity's.
  { pattern: /already linked to another account here|can't be created with it|couldn't confirm who you are|access was denied|took too long or was started in another browser|an account already uses this email address|by verified email|\(provider MFA\)|\}: account suspended"|\}: no linked account"/u,
    where: (path) => /^crates\/[^/]+\/src\//u.test(path), message: "external sign-in refusal and audit text belongs to rbit_identity::external" },
  // The callback and completion checks are not re-implemented by the service.
  { pattern: /\bstate_matches\s*\(|\.open\(\s*value\s*,|\btrust_mfa\s*&&|\bemail_verified\s*&&|\bProviderRegistration::FollowSite\s*=>\s*registration_open|"code_challenge_method"|\.amr\b|get\("email_verified"\)|\bMAX_PENDING_REGISTRATIONS\b|only way to sign in to your account|\bfn\s+lock_pending\b/u,
    where: (path) => SERVICE.test(path), message: "external sign-in checks are decided by rbit_identity::external" },
];

/** Comments and a trailing inline test module are not checked. */
function code(source) {
  const inlineTests = source.search(/#\[cfg\(test\)\]\s*mod\s+tests\s*\{/u);
  const body = inlineTests < 0 ? source : source.slice(0, inlineTests);
  return body.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Production Rust only: suites assert the text. */
function production(path) {
  return /^crates\/[^/]+\/src\//u.test(path) && path.endsWith(".rs") && !/\/tests(?:\.rs|\/)/u.test(path.slice(path.indexOf("/src/")));
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkExternalSignInRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || OWNER.test(path)) continue;
    const text = code(source);
    for (const rule of RULES) {
      if (rule.where(path) && rule.pattern.test(text)) violations.push(`${path}: ${rule.message}`);
    }
  }
  return violations;
}
