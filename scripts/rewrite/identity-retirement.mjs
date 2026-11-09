/** Retired identity paths must not return; see docs/design/rewrite/identity-retirement.md. */
import { checkMfaRetirement } from "./mfa-retirement.mjs";
import { checkOidcRetirement } from "./oidc-retirement.mjs";
import { checkPasskeyRetirement } from "./passkey-retirement.mjs";
import { checkProfileRetirement } from "./profile-retirement.mjs";
import { checkRecoveryRetirement } from "./recovery-retirement.mjs";
import { checkSecondFactorRetirement } from "./second-factor-retirement.mjs";
import { checkTokenRetirement } from "./token-retirement.mjs";

// Session contracts are imported from their defining module, not the legacy service crate.
const SERVICE_ALIAS = /\baccounts::(?:\{[^}]*\b(?:ClientInfo|ActiveSession|SessionUser)\b|(?:ClientInfo|ActiveSession|SessionUser)\b)/u;
const SERVICE_REEXPORT = /\bpub\s+use\s+[^;]*\b(?:ClientInfo|ActiveSession|SessionUser)\b[^;]*;/u;
// Expired-session cleanup belongs to the identity session application.
const DIRECT_PURGE = /\.sessions\(\)\s*\.purge_expired\(|\bSessionStore::purge_expired\b/u;
const PURGE_OWNERS = ["crates/rbit-storage/", "crates/rbit-identity/", "crates/rbit-identity-models/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkIdentityRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    const services = path.startsWith("crates/rbit-services/") || /\brbit_services\b/u.test(text);
    if (services && SERVICE_ALIAS.test(text)) {
      violations.push(`${path}: retired service session alias; import ClientInfo/ActiveSession/SessionUser from rbit_identity_models::sessions`);
    }
    if (path.startsWith("crates/rbit-services/src/") && SERVICE_REEXPORT.test(text)) {
      violations.push(`${path}: services must not re-export identity session contracts`);
    }
    if (production(path) && !PURGE_OWNERS.some((owner) => path.startsWith(owner)) && DIRECT_PURGE.test(text)) {
      violations.push(`${path}: direct session purge bypasses the identity session application`);
    }
  }
  return [...violations, ...checkTokenRetirement(sources), ...checkProfileRetirement(sources), ...checkRecoveryRetirement(sources), ...checkMfaRetirement(sources), ...checkPasskeyRetirement(sources), ...checkOidcRetirement(sources), ...checkSecondFactorRetirement(sources)];
}
