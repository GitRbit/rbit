/** Retired CLI paths in the legacy services must not return; see docs/design/rewrite/operations-cli-acceptance.md. */

// `rbit user create` provisions in rbit-server (`src/provision.rs`) through the identity
// registration application; the services crate keeps no CLI entry point of its own.
const PROVISION = "crates/rbit-server/src/provision.rs";
// [pattern, message, applies to path]
const RULES = [
  [/\bcreate_user_offline\b(?!_original)/u, "retired services CLI provisioning; use rbit-server provision", () => true],
  [/\brbit_services::accounts::(?:create_user|registration::)|\baccounts::create_user_offline/u,
    "the server CLI provisions through rbit-server provision, not services", (path) => path.startsWith("crates/rbit-server/")],
  [/\bAccountRegistration\b/u, "server account registration belongs in rbit-server provision",
    (path) => path.startsWith("crates/rbit-server/src/") && path !== PROVISION],
  [/\bfn\s+create_user\b/u, "CLI provisioning moved out of serve", (path) => path === "crates/rbit-server/src/serve.rs"],
];

/** Comments are not code; strip line and block comments, keeping `scheme://` addresses. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/(?<!:)\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkOperationsCliRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !path.startsWith("crates/")) continue;
    const text = code(source);
    for (const [pattern, message, applies] of RULES) {
      if (applies(path) && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
