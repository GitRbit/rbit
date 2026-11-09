import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsRunnersRetirement } from "../rewrite/actions-runners-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsRunnersRetirement([[path, source]]);

test("runner registry decisions outside rbit-actions fail", () => {
  const cases = [
    ["crates/rbit-services/src/actions/runners.rs", "self.ctx().database.actions_runners().register(&new).await?\n", /bypasses/u],
    ["crates/rbit-web/src/x.rs", "db.actions_runners().declare(id, &declaration).await?\n", /bypasses/u],
    ["crates/rbit-services/src/actions/runners.rs", "const SEEN_GRANULARITY_SECONDS: i64 = 60;\n", /rule redefined/u],
    ["crates/rbit-services/src/actions/runners.rs", "pub const MAX_REGISTRATION_LIFETIME: Duration = D;\n", /rule redefined/u],
    ["crates/rbit-services/src/actions/runners.rs", "struct Declared { labels: Vec<RunnerLabel> }\n", /decision redefined/u],
    ["crates/rbit-services/src/actions/runners.rs", "fn invalid_registration() -> RunnerError { todo!() }\n", /decision redefined/u],
    ["crates/rbit-services/src/actions/runners.rs", "Err(\"a registration token lasts from a minute up to 30 days\")\n", /refusal text/u],
    ["crates/rbit-services/src/actions/runners.rs", "let long_poll = self.config().long_poll_seconds.clamp(1, 25);\n", /interval/u],
    ["crates/rbit-services/src/actions/runners.rs", "if seen > written && seen - written >= 60 {}\n", /last-seen/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the owner, the port adapter, tests and comments are allowed", () => {
  assert.deepEqual(checkActionsRunnersRetirement([
    ["crates/rbit-actions/src/runners/policy.rs", "pub const SEEN_GRANULARITY_SECONDS: i64 = 60;\n"],
    ["crates/rbit-services/src/actions/runner_ports.rs", "self.service.ctx().database.actions_runners().register(&new).await?\n"],
    ["crates/rbit-services/src/actions/runners.rs", "// fn invalid_registration retired\nfn x() {}\n#[cfg(test)]\nmod tests {\n  struct Declared;\n}\n"],
    ["crates/rbit-services/tests/actions_runners.rs", "assert_eq!(error, \"a registration token lasts from a minute up to 30 days\");\n"],
  ]), []);
});

test("the current tree has no retired runner registry path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsRunnersRetirement(files), []);
});
