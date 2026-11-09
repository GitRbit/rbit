import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkStatusesRetirement as check } from "../rewrite/statuses-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("status records and decisions are defined only in collaboration", () => {
  for (const source of [
    "pub struct CommitStatus { pub id: StatusId }\n",
    "pub enum StatusState { Pending }\n",
    "pub struct CombinedStatus { pub sha: String }\n",
    "pub fn combined_state(latest: &[CommitStatus]) -> StatusState { todo!() }\n",
    "pub fn parse_sha(value: &str) -> Result<String, StatusError> { todo!() }\n",
    "pub fn required_checks<S>(required: &[S]) {}\n",
  ]) {
    assert.match(check([["crates/rbit-core/src/status.rs", source]])[0], /belong to rbit_collaboration::statuses/u, source);
    assert.deepEqual(check([["crates/rbit-collaboration/src/statuses/models.rs", source]]), [], source);
  }
  assert.deepEqual(check([["crates/rbit-core/src/status.rs", "pub use rbit_collaboration::statuses::{CommitStatus, combined_state};\n"]]), []);
  assert.deepEqual(check([["crates/rbit-core/src/pull.rs", "fn required_checks_block_merges() {}\n"]]), [], "longer names are not the decision");
});

test("status writes go through the application", () => {
  for (const write of [
    "self.ctx.database.commit_statuses().create(repo, &sha, &report, actor, 1000).await?\n",
    "database.commit_statuses()\n    .create_as_system(repo, sha, report, 1000).await?\n",
    "CommitStatusStore::create(&store, repo, sha, report, actor, 1000)\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/actions/statuses.rs", write]])[0], /bypasses rbit_collaboration::statuses::StatusApplication/u, write);
  }
  assert.deepEqual(check([["crates/rbit-storage/src/collaboration_statuses.rs", "CommitStatusStore::create(self, repo, sha, report, c, cap)\n"]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/statuses.rs", "fixture.database.commit_statuses().create_as_system(r, s, &p, 1000)\n"]]), [], "tests seed directly");
  assert.deepEqual(check([["crates/rbit-services/src/pulls/merging.rs", "let statuses = self.ctx().database.commit_statuses();\nstatuses.latest(r, s)\n"]]), []);
});

test("the service adapter holds no status policy or persistence", () => {
  for (const source of [
    "let statuses = self.ctx.database.commit_statuses().latest(id, sha).await?;\n",
    "let sha = parse_sha(sha).map_err(invalid)?;\n",
    "store.create(id, &sha, &report, actor.id(), MAX_STATUSES_PER_CONTEXT)\n",
    "repo.ensure_writable()?;\n",
    "fn at_cap() -> ServiceError { todo!() }\n",
    "Self { state: combined_state(&statuses), sha, statuses }\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/statuses.rs", source]])[0], /returned to the service adapter/u, source);
    assert.match(check([["crates/rbit-services/src/statuses/hooks.rs", source]])[0], /service adapter/u, source);
  }
  assert.deepEqual(check([["crates/rbit-services/src/statuses.rs", "StatusApplication::new(self.ctx.database.commit_statuses(), access)\n"]]), []);
});

test("submission policy text stays in the application", () => {
  const text = "ServiceError::Invalid(format!(\"no commit found for SHA {sha}\"))\n";
  assert.match(check([["crates/rbit-web/src/routes/api/statuses.rs", text]])[0], /policy text belongs/u);
  assert.deepEqual(check([["crates/rbit-collaboration/src/statuses/application.rs", text]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/statuses.rs", text]]), [], "tests assert the text");
});

test("comments and non-Rust files are allowed", () => {
  assert.deepEqual(check([
    ["crates/rbit-services/src/statuses.rs", "//! [`rbit_core::status::combined_state`] over the latest status\n/* repo.ensure_writable() */\n"],
    ["docs/x.md", "pub fn parse_sha"],
  ]), []);
});

test("the current tree has no retired status path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/statuses.rs", root), "utf8"), /StatusApplication::new/u);
});
