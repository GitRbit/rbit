import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkNotificationsRetirement } from "../rewrite/notifications-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkNotificationsRetirement([[path, source]]);

test("direct store access outside the port adapter fails in any production crate", () => {
  for (const source of [
    "async fn f(db: &Database) { db.notifications().subscription(i, u).await; }\n",
    "async fn f(&self) { let store = self.ctx.database.notifications(); store.list(u, true, 1, 0).await; }\n",
  ]) {
    assert.match(check("crates/rbit-services/src/notifications.rs", source)[0], /bypasses the rbit-notifications ports/u, source);
  }
  assert.match(check("crates/rbit-web/src/x.rs", "fn f(s: &NotificationStore) { NotificationStore::has_unread(s, u); }")[0], /storage adapter's/u);
  const adapter = "let store = NotificationInboxStore::new(self.ctx.database.notifications());\n";
  assert.deepEqual(check("crates/rbit-services/src/notifications/inbox.rs", adapter), []);
});

test("fan-out and retention writes and storage model paths are refused outside the owners", () => {
  assert.match(check("crates/rbit-services/src/n.rs", "store.notify_many(i, r, a, t, &x).await?;")[0], /belong to rbit-notifications/u);
  assert.match(check("crates/rbit-server/src/n.rs", "repo.prune_read(cutoff)")[0], /belong to rbit-notifications/u);
  const models = "pub use rbit_storage::sqlite::notifications::{Notification, NotificationReason};\n";
  assert.match(check("crates/rbit-services/src/notifications.rs", models)[0], /import notification models from rbit_notifications/u);
  const owners = "fn f(s: &NotificationStore) { s.notify_many(); db.notifications().prune_read(t); }";
  assert.deepEqual(check("crates/rbit-storage/src/notification_inbox.rs", owners), []);
  assert.deepEqual(check("crates/rbit-notifications/src/fanout.rs", owners), []);
});

test("comments, integration tests and unit-test modules are fixtures", () => {
  assert.deepEqual(checkNotificationsRetirement([
    ["crates/rbit-services/src/a.rs", "// db.notifications().notify_many()\n/* NotificationStore::find */ fn f() {}\n"],
    ["crates/rbit-services/tests/notifications.rs", "fixture.database.notifications().notify(u, i, r, m, None, t).await;"],
    ["crates/rbit-web/src/middleware/auth.rs", "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn g() { { db.notifications().notify(); } }\n}\n"],
  ]), []);
  const after = "#[cfg(test)]\nmod tests { fn g() {} }\nfn later(db: &Database) { db.notifications().has_unread(u); }\n";
  assert.match(check("crates/rbit-web/src/x.rs", after)[0], /bypasses/u, "only the test module is skipped");
});

test("the current tree has no retired notification path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkNotificationsRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/notifications.rs", root), "utf8"), /fanout::deliver/u);
});
