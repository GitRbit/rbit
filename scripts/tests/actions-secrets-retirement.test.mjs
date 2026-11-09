import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsSecretsRetirement } from "../rewrite/actions-secrets-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsSecretsRetirement([[path, source]]);

test("a second secret or environment rule fails outside rbit-actions", () => {
  const cases = [
    ["crates/rbit-services/src/actions/environments.rs", "pub const MAX_WAIT_TIMER_MINUTES: u32 = 43_200;\n", /limit redefined/u],
    ["crates/rbit-services/src/actions/secrets.rs", "pub const UNREADABLE: &str = \"x\";\n", /limit redefined/u],
    ["crates/rbit-services/src/actions/secrets.rs", "return Err(format!(\"a {kind} needs a value\"));\n", /secret value or scope/u],
    ["crates/rbit-server/src/x.rs", "Err(\"a repository secret is visible to its repository only\")\n", /secret value or scope/u],
    ["crates/rbit-web/src/x.rs", "format!(\"{kind} `{short}` is not allowed to deploy to {name}\")\n", /environment protection/u],
    ["crates/rbit-services/src/actions/environments.rs", "format!(\"{} does not let whoever started a run approve it\", n)\n", /environment protection/u],
    ["crates/rbit-services/src/actions/environments.rs", "format!(\"{bang}refs/heads/{pattern}\")\n", /pattern rewriting/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source).join("\n"), message, source);
});

test("the owner, the API request shape, other crates' words and comments are allowed", () => {
  assert.deepEqual(checkActionsSecretsRetirement([
    ["crates/rbit-actions/src/environments.rs", "pub const MAX_WAIT_TIMER_MINUTES: u32 = 43_200;\n"],
    ["crates/rbit-core/src/actions/records.rs", "pub enum BranchPolicy { All }\n"],
    ["crates/rbit-storage/src/sqlite/actions_environments.rs", "pub const MAX_REVIEWERS: usize = 6;\n"],
    ["crates/rbit-web/src/routes/api/actions.rs", "ApiError::invalid(\"visibility\", \"a repository secret is visible to its repository only\")\n"],
    ["crates/rbit-workflow/src/options.rs", "format!(\"`{flag}` needs a value\")\n"],
    ["crates/rbit-web/src/routes/api/pulls.rs", "ApiError::service(error, \"reviewers\")\n"],
    ["crates/rbit-services/tests/actions_secrets.rs", "assert!(m.contains(\"is not allowed to deploy to\"));\n"],
    ["crates/rbit-services/src/actions/secrets.rs", "// a {kind} needs a value: rbit_actions::secrets::check_value\n"],
  ]), []);
});

test("the current tree has no retired secret or environment rule", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsSecretsRetirement(files), []);
});
