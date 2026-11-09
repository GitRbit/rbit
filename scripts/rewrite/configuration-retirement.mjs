/** Retired configuration and runtime paths must not return; see docs/design/rewrite/operations-configuration-acceptance.md. */

const EDITOR = "crates/rbit-services/src/admin/configuration.rs";
const DOCUMENT = "crates/rbit-core/src/config/document.rs";
// The document rules are shared from rbit-core; the service editor only loads, seals and audits.
const EDITOR_RULE = /\bfn\s+(?:redact|restore_secrets|validate_configuration|open_stored|is_secret)\b|\bREDACTED\b|128\s*\*\s*1024/u;
const SECRET_RULE = /\bfn\s+is_secret_field\b/u;
// Administration pages reach monitoring and restarts through the runtime port.
const RUNTIME_BYPASS = /\bstate\.restart\b|\.monitoring\.snapshot\(\)/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkConfigurationRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (path === EDITOR && EDITOR_RULE.test(text)) violations.push(`${path}: configuration document rules belong in rbit_core::config::document`);
    if (path !== DOCUMENT && SECRET_RULE.test(text)) violations.push(`${path}: the secret-field rule is owned by rbit_core::config::document`);
    if (path.startsWith("crates/rbit-web/src/routes/") && RUNTIME_BYPASS.test(text)) {
      violations.push(`${path}: use the RuntimeControl port for monitoring and restarts`);
    }
  }
  return violations;
}
