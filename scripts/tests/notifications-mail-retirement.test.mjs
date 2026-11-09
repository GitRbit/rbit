import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkNotificationsMailRetirement } from "../rewrite/notifications-mail-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkNotificationsMailRetirement([[path, source]]);
const OUTBOX = "crates/rbit-services/src/notification_email/outbox.rs";

test("digest and claim stores are reached only through the mail adapter", () => {
  const digests = "async fn f(&self) { self.ctx.database.email_digests().claim(a, p, n).await; }\n";
  assert.match(check("crates/rbit-services/src/notification_email/digests.rs", digests)[0], /digest store access/u);
  assert.deepEqual(check(OUTBOX, digests), []);
  const claims = "async fn f(db: &Database) { db.notification_email().claim_many(&ids, i, k, n).await; }\n";
  assert.match(check("crates/rbit-services/src/notification_email.rs", claims).join("\n"), /email claims and recipient reads/u);
  assert.match(check("crates/rbit-web/src/x.rs", claims).join("\n"), /belongs to the mail adapter or settings/u);
  const settings = "async fn f(&self) { self.ctx.database.notification_email().preference(u).await; }\n";
  assert.deepEqual(check("crates/rbit-services/src/notification_email.rs", settings), []);
});

test("moved mail and webhook policy cannot be redefined beside its adapters", () => {
  for (const source of [
    "fn verify_link(key: &[u8], token: &str) -> Option<AccountId> { None }\n",
    "type LinkMac = Hmac<Sha256>;\n",
    "const RETRY_DELAYS_SECONDS: [u64; 5] = [10, 60, 600, 3_600, 21_600];\n",
    "pub const MAX_HOOKS_PER_OWNER: i64 = 20;\n",
    "pub fn signature(secret: &str, body: &[u8]) -> String { String::new() }\n",
  ]) {
    assert.match(check("crates/rbit-services/src/webhooks/delivery.rs", source)[0], /belongs to rbit-notifications/u, source);
    assert.match(check("crates/rbit-services/src/notification_email.rs", source)[0], /belongs to rbit-notifications/u, source);
  }
  assert.deepEqual(check("crates/rbit-services/src/lfs/tokens.rs", "fn mac() -> Hmac<Sha256> { todo!() }\n"), [], "other HMAC users");
  assert.deepEqual(check("crates/rbit-notifications/src/mail/unsubscribe.rs", "type LinkMac = Hmac<Sha256>;\n"), []);
  assert.deepEqual(check("crates/rbit-services/src/webhooks.rs", "pub use rbit_notifications::webhooks::policy::MAX_HOOKS_PER_OWNER;\n"), []);
});

test("webhook queueing, attempts and envelopes stay behind the feature ports", () => {
  for (const source of [
    "async fn f(&self) { self.inner.store.enqueue(deliveries).await; }\n",
    "async fn f(&self) { self.inner.store.claim_due(now, 1).await; }\n",
    "fn f() -> AttemptOutcome { AttemptOutcome::Failed }\n",
  ]) {
    assert.match(check("crates/rbit-services/src/webhooks.rs", source)[0], /webhook ports/u, source);
    assert.deepEqual(check("crates/rbit-services/src/webhooks/ports.rs", source), []);
  }
  for (const source of ["const ZEN: &[&str] = &[];\n", "pub const NULL_SHA: &str = \"0\";\n", "hooks.retain(|hook| *hook <= newest);\n"]) {
    assert.match(check("crates/rbit-services/src/webhooks/payloads.rs", source)[0], /belongs to rbit-notifications/u, source);
  }
});

test("comments, tests and unit-test modules are not production paths", () => {
  assert.deepEqual(checkNotificationsMailRetirement([
    ["crates/rbit-services/src/notification_email.rs", "// fn verify_link was moved\n/* db.email_digests() */ fn f() {}\n"],
    ["crates/rbit-services/tests/email_digests.rs", "fixture.database.email_digests().claim(a, p, n).await;"],
    ["crates/rbit-services/src/webhooks/delivery.rs", "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn signature() {}\n}\n"],
  ]), []);
});

test("the current tree has no retired mail or webhook policy path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(checkNotificationsMailRetirement(files), []);
  assert.match(readFileSync(new URL(OUTBOX, root), "utf8"), /impl MailOutbox for ThreadOutbox/u);
});
