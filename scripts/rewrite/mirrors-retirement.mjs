/** Retired mirror policy must not return; see docs/design/rewrite/repositories-mirrors-acceptance.md. */
import { code, withoutTestModules } from "./notifications-retirement.mjs";

const PORTS = ["crates/rbit-services/src/mirrors/port.rs", "crates/rbit-services/src/push_mirrors/port.rs"];
const OWNERS = ["crates/rbit-storage/", "crates/rbit-repositories/"];
// Outcomes and claims are recorded only by the sync and push port adapters.
const OUTCOME_WRITE = /\.store\(\)\s*\.(?:record_sync|record_attempt|claim)\(/u;
// Policy that moved to rbit-repositories must not be redefined beside its adapters.
const POLICY = new RegExp([
  String.raw`\bconst\s+(?:PUSH_REFUSED|SCHEDULER_TICK|MAX_CONCURRENT_FETCHES|MAX_CONCURRENT_PUSHES|BATCH|DEFAULT_USERNAME)\b`,
  String.raw`\benum\s+MirrorPhase\b`, String.raw`\bstruct\s+ActiveProgress\b`,
  "the interval must be between", "the remote moved to another host", "can push to at most",
].join("|"), "u");
const ADAPTERS = /^crates\/rbit-services\/src\/(?:mirrors|push_mirrors)(?:\.rs|\/)/u;

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkMirrorsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || !path.endsWith(".rs") || OWNERS.some((owner) => path.startsWith(owner))) continue;
    const text = withoutTestModules(code(source));
    if (!PORTS.includes(path) && OUTCOME_WRITE.test(text)) {
      violations.push(`${path}: mirror claims and outcomes go through the rbit-repositories mirror ports`);
    }
    if (ADAPTERS.test(path) && POLICY.test(text)) {
      violations.push(`${path}: mirror policy belongs to rbit-repositories`);
    }
  }
  return violations;
}
