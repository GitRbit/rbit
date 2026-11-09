/** Retired collaborator management must not return; see docs/design/rewrite/access-policy-collaborators.md. */

const ADAPTER = "crates/rbit-services/src/collaborators.rs";
// The service adapter composes the rbit-access rules and their store adapter.
const DECISION = /\bfn\s+(?:require_admin|find_user)\b|\bStorageError::Conflict\b|\.collaborators\(\)|\bCollaboratorStore\b|\bis_suspended\s*\(/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkCollaboratorsRetirement(sources) {
  return sources.filter(([path, source]) => path === ADAPTER && DECISION.test(code(source)))
    .map(([path]) => `${path}: collaborator management decision or raw store belongs in rbit-access and its store adapter`);
}
