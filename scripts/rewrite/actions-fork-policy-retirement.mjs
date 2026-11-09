/** Retired fork-policy paths must not return; see docs/design/rewrite/actions-fork-policy-acceptance.md. */

// Pull request trust, run grants and untrusted isolation are decided in rbit-actions (`trust`).
const OWNER = "crates/rbit-actions/";
const ADAPTER = "crates/rbit-services/src/actions/trust.rs";
const SOURCES = /^crates\/[^/]+\/src\//u;
// Code that reads a stored run's trust flag to decide what it gets: services and server.
const GRANT_READERS = ["crates/rbit-services/src/", "crates/rbit-server/src/"];
const TYPES = /\b(?:enum|struct|type)\s+(?:RunTrust|TriggeringActor|AuthorStanding)\b/u;
const CLASSIFY = /\bfn\s+(?:classify_pull_request|triggering_actor|author_standing)\b/u;
const DECISION = /\bTriggeringActor::Known\b|\bAuthorStanding::(?:Vouched|Suspended|Unknown)\b/u;
// Concurrency and cache namespaces of untrusted runs (`pull/<n>` cache scopes are also
// parsed by the cache API, so only the two untrusted-only prefixes are checked).
const NAMESPACE = /"(?:pr-|untrusted\/)\{/u;
// A stored run's flag goes through RunTrust::from_stored, Grants::of_stored or Isolation::of.
const FLAG = /\brun\.trusted\b/gu;
const VIA = /(?:from_stored|of_stored|Isolation::of)\(\s*$/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsForkPolicyRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !SOURCES.test(path) || path.startsWith(OWNER)) continue;
    const text = code(source);
    if (TYPES.test(text)) violations.push(`${path}: run trust type redefined outside rbit_actions::trust`);
    if (CLASSIFY.test(text) && !(path === ADAPTER && text.includes("PullRequestTrust::classify("))) {
      violations.push(`${path}: pull request trust decided outside rbit_actions::trust::PullRequestTrust`);
    }
    if (DECISION.test(text)) violations.push(`${path}: trust decision inputs built outside rbit_actions::trust`);
    if (NAMESPACE.test(text)) violations.push(`${path}: untrusted isolation scope built outside rbit_actions::trust::Isolation`);
    if (GRANT_READERS.some((root) => path.startsWith(root))) {
      for (const match of text.matchAll(FLAG)) {
        if (!VIA.test(text.slice(Math.max(0, match.index - 40), match.index))) {
          violations.push(`${path}: stored run trust read directly; use RunTrust::from_stored, Grants::of_stored or Isolation::of`);
          break;
        }
      }
    }
  }
  return violations;
}
