/** Retired transfer/old-address paths must not return; see docs/design/rewrite/repositories-transfer-acceptance.md. */

// Old addresses resolve through rbit_repositories::aliases; only storage runs the lookup.
const OLD_ADDRESS = /\bfind_by_old_path\b/u;
// The owner-change transaction is reached through the transfer application's store port.
const DIRECT_TRANSFER = /\.repositories\(\)\s*\.transfer\(|\bRepositoryStore::transfer\b/u;
const STORAGE = "crates/rbit-storage/";
// The service adapter composes effects only: no policy, lookups or storage error mapping.
const SERVICE_ADAPTER = /^crates\/rbit-services\/src\/transfers(?:\.rs|\/)/u;
const ADAPTER_POLICY = /\b(?:require_admin|authorised_owner|memberships_of|is_suspended|find_owner|StorageError)\b/u;
// Transfer policy text is owned by the application.
const POLICY_TEXT = /already belongs to|rename one of them first|you cannot transfer repositories to|there is no user or organization called/u;
const POLICY_OWNER = "crates/rbit-repositories/src/";

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkRepositoryTransferRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (!path.startsWith(STORAGE) && OLD_ADDRESS.test(text)) {
      violations.push(`${path}: old-address lookup bypasses rbit_repositories::aliases::RepositoryAliases`);
    }
    if (!path.startsWith(STORAGE) && DIRECT_TRANSFER.test(text)) {
      violations.push(`${path}: direct owner-change write bypasses rbit_repositories::transfer`);
    }
    if (SERVICE_ADAPTER.test(path) && ADAPTER_POLICY.test(text)) {
      violations.push(`${path}: transfer policy or persistence returned to the service adapter; use RepositoryTransfer and TransferStore`);
    }
    if (production(path) && !path.startsWith(POLICY_OWNER) && POLICY_TEXT.test(text)) {
      violations.push(`${path}: transfer policy text belongs to rbit_repositories::transfer`);
    }
  }
  return violations;
}
