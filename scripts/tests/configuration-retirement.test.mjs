import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkConfigurationRetirement } from "../rewrite/configuration-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const EDITOR = "crates/rbit-services/src/admin/configuration.rs";

test("the service editor holds no document rules", () => {
  for (const source of ["fn redact(value: &mut Value) {}", "fn restore_secrets() {}", "const REDACTED: &str = \"<unchanged>\";", "if document.len() > 128 * 1024 {}"]) {
    assert.match(checkConfigurationRetirement([[EDITOR, source]])[0], /rbit_core::config::document/u, source);
  }
  assert.deepEqual(checkConfigurationRetirement([[EDITOR, "pub use rbit_core::config::document::validate_configuration;"]]), []);
});

test("the secret-field rule has one owner", () => {
  const rule = "pub fn is_secret_field(name: &str) -> bool { name.ends_with(\"secret\") }";
  assert.match(checkConfigurationRetirement([["crates/rbit-storage/src/sealing.rs", rule]])[0], /owned by rbit_core/u);
  assert.deepEqual(checkConfigurationRetirement([["crates/rbit-core/src/config/document.rs", rule]]), []);
});

test("administration routes use the runtime port", () => {
  for (const source of ["state.restart.receiver_count() > 0", "&state.monitoring.snapshot()"]) {
    assert.match(checkConfigurationRetirement([["crates/rbit-web/src/routes/admin.rs", source]])[0], /RuntimeControl/u, source);
  }
  assert.deepEqual(checkConfigurationRetirement([["crates/rbit-web/src/routes/admin.rs", "state.runtime.restart_supported()"]]), []);
  assert.deepEqual(checkConfigurationRetirement([["crates/rbit-web/tests/admin_configuration.rs", "state.restart.subscribe()"]]), []);
});

test("the current tree has no retired configuration path", () => {
  const { files } = authoredSources(fileURLToPath(new URL("../../", import.meta.url)));
  assert.deepEqual(checkConfigurationRetirement(files), []);
});
