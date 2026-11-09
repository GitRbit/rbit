import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkApiRetirement } from "../rewrite/api-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkApiRetirement([[path, source]]);

test("a second scope type or API convention owner fails", () => {
  const cases = [
    ["crates/rbit-core/src/access_token.rs", "pub enum TokenScope { Read, Write, Admin }\n", /token scope semantics/u],
    ["crates/rbit-web/src/x.rs", "impl TokenScope { pub fn limit(self) {} }\n", /token scope semantics/u],
    ["crates/rbit-web/src/routes/api/repos.rs", "(scheme.eq_ignore_ascii_case(\"bearer\")).then_some(token)\n", /credentials or throttling/u],
    ["crates/rbit-web/src/routes/api/hooks.rs", "let who = state.services.tokens.authenticate(token).await?;\n", /credentials or throttling/u],
    ["crates/rbit-web/src/routes/api/issues.rs", "throttle.check(ThrottleScope::ApiRequest, &id)?;\n", /credentials or throttling/u],
    ["crates/rbit-web/src/routes/api/auth.rs", "pub const TASK_ROUTES: &[(ApiMethod, &str, TaskNeed)] = &[];\n", /task-token allowlist/u],
    ["crates/rbit-web/src/routes/api/actions.rs", "headers.insert(HeaderName::from_static(\"x-total-count\"), v);\n", /pagination headers/u],
    ["crates/rbit-web/src/routes/api/actions.rs", "headers.insert(header::LINK, link);\n", /pagination headers/u],
    ["crates/rbit-web/src/routes/api/repos.rs", "headers.insert(header::WWW_AUTHENTICATE, v);\n", /error challenge/u],
    ["crates/rbit-web/src/routes/api/mod.rs", "const OPENAPI: &str = include_str!(\"openapi.json\");\n", /OpenAPI document/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source).join("\n"), message, source);
});

test("owners, reads in tests, comments and other routes are allowed", () => {
  assert.deepEqual(checkApiRetirement([
    ["crates/rbit-access/src/token_scope.rs", "pub enum TokenScope { Read }\nimpl TokenScope {}\n"],
    ["crates/rbit-web/src/routes/api/auth.rs", "throttle.check(ThrottleScope::ApiRequest, &id)?;\nstate.services.tokens.authenticate(token)\n"],
    ["crates/rbit-web/src/routes/api/task_routes.rs", "pub const TASK_ROUTES: &[(ApiMethod, &str, TaskNeed)] = &[];\n"],
    ["crates/rbit-web/src/routes/api/pagination.rs", "headers.insert(header::LINK, link);\n"],
    ["crates/rbit-web/src/routes/api/actions.rs", "assert_eq!(response.headers()[\"x-total-count\"], \"3\");\nlet link = response.headers()[header::LINK].to_str();\n"],
    ["crates/rbit-web/src/routes/api/error.rs", "headers.insert(header::WWW_AUTHENTICATE, v);\n"],
    ["crates/rbit-web/src/routes/api/openapi_document.rs", "const OPENAPI: &str = include_str!(\"openapi.json\");\n"],
    ["crates/rbit-web/src/routes/git_http.rs", "let user = state.services.tokens.authenticate(password).await?;\n"],
    ["crates/rbit-web/src/routes/api/repos.rs", "// headers.insert(header::LINK, link) moved to pagination\n"],
  ]), []);
});

test("the current tree keeps one owner per API convention", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkApiRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-core/src/access_token.rs", root), "utf8"), /pub use rbit_access::TokenScope;/u);
});
