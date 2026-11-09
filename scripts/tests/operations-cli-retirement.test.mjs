import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkOperationsCliRetirement } from "../rewrite/operations-cli-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkOperationsCliRetirement([[path, source]]);

test("a services CLI provisioning path fails wherever it returns", () => {
  const cases = [
    ["crates/rbit-services/src/accounts/registration.rs", "pub fn create_user_offline<'a>() {}\n", /retired services CLI provisioning/u],
    ["crates/rbit-services/src/accounts.rs", "pub use registration::{create_user_offline, create_user_offline_original};\n", /retired services CLI provisioning/u],
    ["crates/rbit-server/src/serve.rs", "accounts::create_user(&config, &store, input, Some(admin)).await?;\nuse rbit_services::accounts::create_user;\n", /not services/u],
    ["crates/rbit-server/src/main.rs", "let app = rbit_identity::accounts::AccountRegistration::new(&s, &p, 8, None);\n", /belongs in rbit-server provision/u],
    ["crates/rbit-server/src/serve.rs", "pub async fn create_user(config: Config) {}\n", /moved out of serve/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the server provisioning, the inactive original path and comments are allowed", () => {
  assert.deepEqual(checkOperationsCliRetirement([
    ["crates/rbit-server/src/provision.rs", "let application = AccountRegistration::new(accounts, &passwords, 8, None);\npub async fn create_user() {}\n"],
    ["crates/rbit-services/src/accounts/registration.rs", "pub fn create_user_offline_original<'a>() {}\n"],
    ["crates/rbit-services/tests/password_admission/creation_proof.rs", "rbit_services::accounts::create_user_offline_original(&config, &store, input(\"alice\"), None)\n"],
    ["crates/rbit-services/src/admin/accounts.rs", "pub fn create_user<'a>(&'a self) {}\n"],
    ["crates/rbit-server/src/serve.rs", "// create_user_offline was retired; see provision.rs\n"],
  ]), []);
});

test("the current tree has no retired CLI provisioning path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkOperationsCliRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-server/src/provision.rs", root), "utf8"), /AccountRegistration::new/u);
});
