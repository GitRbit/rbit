/** Retired REST API paths must not return; see docs/design/rewrite/integrations-api-acceptance.md. */

// Token scope semantics are decided in rbit-access. Inside the REST API, each convention has
// one owner module: credentials, throttling and scopes in `auth.rs`, the task allowlist in
// `task_routes.rs`, pages and their headers in `pagination.rs`, error bodies and challenges in
// `error.rs`, and the served OpenAPI document in `openapi_document.rs`.
const API = "crates/rbit-web/src/routes/api/";
const ACCESS = "crates/rbit-access/";
// [pattern, message, owner file below API ("" for rules over every crate)]
const RULES = [
  [/\benum\s+TokenScope\b|\bimpl\s+TokenScope\b/u, "token scope semantics outside rbit_access::TokenScope", ""],
  [/\beq_ignore_ascii_case\(\s*"bearer"\s*\)|\bservices\.tokens\.authenticate\s*\(|\bThrottleScope::Api(?:Auth|Request)\b/u,
    "API credentials or throttling outside api/auth.rs (Caller)", "auth.rs"],
  [/\b(?:const|static)\s+TASK_ROUTES\b/u, "task-token allowlist outside api/task_routes.rs", "task_routes.rs"],
  [/HeaderName::from_static\(\s*"x-total-count"\s*\)|\.insert\(\s*header::LINK\b/u, "pagination headers outside api/pagination.rs", "pagination.rs"],
  [/\.insert\(\s*header::WWW_AUTHENTICATE\b|\.insert\(\s*header::RETRY_AFTER\b/u, "API error challenge or retry header outside api/error.rs", "error.rs"],
  [/include_str!\(\s*"openapi[./]/u, "OpenAPI document assembled outside api/openapi_document.rs", "openapi_document.rs"],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkApiRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || !path.startsWith("crates/")) continue;
    const text = code(source);
    for (const [pattern, message, owner] of RULES) {
      const applies = owner === "" ? !path.startsWith(ACCESS) : path.startsWith(API) && path !== `${API}${owner}`;
      if (applies && pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
