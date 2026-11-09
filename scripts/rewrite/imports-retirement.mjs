/** Retired import admission and lifecycle policy must not return; see docs/design/rewrite/repositories-imports-acceptance.md. */
import { code, withoutTestModules } from "./notifications-retirement.mjs";

// Admission and lifecycle rules that moved to rbit-repositories must not be redefined
// beside their adapters in the services import modules.
const POLICY = new RegExp([
  String.raw`\bconst\s+(?:MAX_TOKEN_BYTES|MAX_DESCRIPTION_CHARS|INTERRUPTED|STOPPED)\b`,
  "the token is not valid", "importing is disabled on this instance",
  "the repository is no longer available to the importing user",
  String.raw`\btruncate_chars\(`,
].join("|"), "u");
const ADAPTERS = /^crates\/rbit-services\/src\/imports(?:\.rs|\/(?:ports|startup)\.rs)$/u;

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkImportsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (ADAPTERS.test(path) && POLICY.test(withoutTestModules(code(source)))) {
      violations.push(`${path}: import admission and lifecycle policy belongs to rbit-repositories`);
    }
  }
  return violations;
}
