import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkMirrorsRetirement } from "../rewrite/mirrors-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkMirrorsRetirement([[path, source]]);

test("mirror claims and outcomes are written only by the port adapters", () => {
  const record = "async fn f(&self) { self.store().record_sync(repo, now, None).await; }\n";
  assert.match(check("crates/rbit-services/src/mirrors.rs", record)[0], /mirror ports/u);
  assert.deepEqual(check("crates/rbit-services/src/mirrors/port.rs", record), []);
  const claim = "async fn f(&self) { self.store().claim(id, next).await; }\n";
  assert.match(check("crates/rbit-services/src/push_mirrors.rs", claim)[0], /mirror ports/u);
  assert.deepEqual(check("crates/rbit-services/src/push_mirrors/port.rs", claim), []);
  assert.deepEqual(check("crates/rbit-services/src/webhooks/ports.rs", "self.inner.store.record_attempt(id, r, o)"), [], "other stores");
});

test("moved mirror policy cannot be redefined beside its adapters", () => {
  for (const source of [
    "const BATCH: u32 = 16;\n",
    "pub enum MirrorPhase { Connecting }\n",
    "struct ActiveProgress<'a> { repo: RepoId }\n",
    "Err(format!(\"the interval must be between {min} minutes and a year\"))\n",
    "Err(\"the remote moved to another host; enter the password or token again for it\")\n",
  ]) {
    assert.match(check("crates/rbit-services/src/push_mirrors.rs", source).join(), /belongs to rbit-repositories/u, source);
    assert.match(check("crates/rbit-services/src/mirrors.rs", source).join(), /belongs to rbit-repositories/u, source);
  }
  assert.deepEqual(check("crates/rbit-repositories/src/mirrors.rs", "pub const SCHEDULER_TICK: Duration = Duration::from_secs(30);\n"), []);
  assert.deepEqual(check("crates/rbit-services/src/mirrors.rs", "// const BATCH: u32 = 16;\n"), [], "comments");
});

test("the current tree has no retired mirror path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(checkMirrorsRetirement(files), []);
});
