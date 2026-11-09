import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("retired service session aliases fail in any crate, tests included", () => {
  for (const source of [
    "use rbit_services::accounts::ClientInfo;\n",
    "use rbit_services::{Services, accounts::{ActiveSession, SessionUser}};\n",
    "fn f(c: &rbit_services::accounts::ClientInfo) {}\n",
  ]) {
    assert.match(checkIdentityRetirement([["crates/rbit-web/tests/x.rs", source]])[0], /retired service session alias/u, source);
  }
  assert.match(checkIdentityRetirement([["crates/rbit-services/src/two_factor.rs", "use crate::accounts::ClientInfo;\n"]])[0], /alias/u);
  const storage = [["crates/rbit-storage/src/sqlite/mod.rs", "pub use self::accounts::{AccountStore, SessionUser};\n"]];
  assert.deepEqual(checkIdentityRetirement(storage), [], "the store adapter owns its own accounts module");
  const reexport = [["crates/rbit-services/src/accounts.rs", "pub use rbit_identity::sessions::{ActiveSession, ClientInfo};\n"]];
  assert.match(checkIdentityRetirement(reexport)[0], /must not re-export/u);
});

test("direct purge is refused outside the identity application and its store", () => {
  const call = "async fn run(kv: KvStore) { kv.sessions().purge_expired().await; }\n";
  assert.match(checkIdentityRetirement([["crates/rbit-server/src/serve/session_purger.rs", call]])[0], /bypasses the identity/u);
  assert.match(checkIdentityRetirement([["crates/rbit-web/src/x.rs", "SessionStore::purge_expired(&store)"]])[0], /bypasses/u);
  assert.deepEqual(checkIdentityRetirement([["crates/rbit-storage/src/kv/sessions.rs", call]]), []);
  assert.deepEqual(checkIdentityRetirement([["crates/rbit-services/tests/sessions.rs", call]]), [], "parity tests compare against the store");
});

test("comments, identity-model imports and unrelated names are allowed", () => {
  assert.deepEqual(checkIdentityRetirement([
    ["crates/rbit-web/src/a.rs", "// accounts::ClientInfo was retired\nuse rbit_identity_models::sessions::ClientInfo;\n"],
    ["crates/rbit-services/src/b.rs", "/* kv.sessions().purge_expired() */ pub use accounts::{AccountService, SignUp};\n"],
    ["docs/x.md", "accounts::ClientInfo"],
  ]), []);
});

test("the current tree has no retired identity path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkIdentityRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-server/src/serve/session_purger.rs", root), "utf8"), /purge_expired_sessions/u);
});
