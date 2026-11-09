import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsDisabledRetirement } from "../rewrite/actions-disabled-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsDisabledRetirement([[path, source]]);

test("reading the Actions switch outside the gate fails", () => {
  const cases = [
    ["crates/rbit-services/src/actions/mod.rs", "if !ctx.config.actions.enabled { return None; }\n", /outside rbit_actions::availability::Availability::of/u],
    ["crates/rbit-web/src/routes/api/mod.rs", "if Availability::of(config.actions.enabled).is_enabled() && state.config.actions.enabled {}\n", /outside/u],
    ["crates/rbit-server/src/x.rs", "let on = config.actions.enabled;\n", /outside/u],
    ["crates/rbit-services/src/webhooks.rs", "format!(\"\\\"{name}\\\" needs Actions, which this instance does not have\")\n", /refusal text/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the gate, assignments, tests and other crates are allowed", () => {
  assert.deepEqual(checkActionsDisabledRetirement([
    ["crates/rbit-services/src/actions/mod.rs", "Availability::of(ctx.config.actions.enabled).provide(|| build())\n"],
    ["crates/rbit-web/src/routes/api/mod.rs", "let document = Availability::of(state.config.actions.enabled).choose(a, b);\n"],
    ["crates/rbit-server/src/config.rs", "config.actions.enabled = true;\n"],
    ["crates/rbit-web/src/routes/api/mod.rs", "fn x() {}\n#[cfg(test)]\nmod tests {\n  fn t() { if config.actions.enabled {} }\n}\n"],
    ["crates/rbit-core/src/config/actions.rs", "if self.actions.enabled {}\n"],
    ["crates/rbit-actions/src/availability.rs", "\"needs Actions, which this instance does not have\"\n"],
  ]), []);
});

test("the current tree reads the Actions switch only through the gate", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsDisabledRetirement(files), []);
});
