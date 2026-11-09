import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsArtifactsCachesRetirement } from "../rewrite/actions-artifacts-caches-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsArtifactsCachesRetirement([[path, source]]);

test("store decisions that bypass rbit-actions fail", () => {
  const cases = [
    ["crates/rbit-services/src/actions/artifacts.rs", "store.actions_artifacts().record(&new, true).await?;\n", /artifact row write/u],
    ["crates/rbit-web/src/routes/x.rs", "db.actions_artifacts().delete(id).await?;\n", /artifact row write/u],
    ["crates/rbit-services/src/actions/caches.rs", "self.ctx().database.actions_caches().least_used(repo, 256).await?;\n", /cache row access/u],
    ["crates/rbit-services/src/actions/caches.rs", "store.actions_caches().exact(repo, scope, key, version)\n", /cache row access/u],
    ["crates/rbit-services/src/actions/caches.rs", "async fn evict_caches(&self) {}\n", /decision redefined/u],
    ["crates/rbit-services/src/actions/sweeper.rs", "const ORPHAN_AGE: Duration = Duration::from_hours(1);\n", /decision redefined/u],
    ["crates/rbit-services/src/actions/artifacts.rs", "let room = config.max_artifact_storage_bytes.saturating_sub(stored);\n", /quota or retention/u],
    ["crates/rbit-services/src/actions/artifacts.rs", "days.clamp(1, config.artifact_retention_days)\n", /quota or retention/u],
    ["crates/rbit-web/src/routes/actions/mod.rs", "pub fn to_zip<R, W>(source: R, out: W) {}\n", /download conversion/u],
    ["crates/rbit-services/src/actions/caches.rs", "tokio::fs::rename(&temporary, &path).await\n", /file adapter/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the owner, its ports, reads, tests and comments are allowed", () => {
  assert.deepEqual(checkActionsArtifactsCachesRetirement([
    ["crates/rbit-actions/src/caches.rs", "pub async fn evict<P>() {}\nconst ORPHAN_AGE: u8 = 1;\n"],
    ["crates/rbit-services/src/actions/cache_ports.rs", "self.service.ctx().database.actions_caches().touch(entry.id, now).await?\n"],
    ["crates/rbit-services/src/actions/artifact_ports.rs", "database.actions_artifacts().record(&row, overwrite).await?\n"],
    ["crates/rbit-services/src/actions/archive_files.rs", "async fn receive() {}\ntokio::fs::rename(a, b).await\n"],
    ["crates/rbit-services/src/actions/artifacts.rs", "store.actions_artifacts().of_run(run, now).await?\n"],
    ["crates/rbit-services/src/actions/caches.rs", "// fn remove_cache was retired\nfn x() {}\n#[cfg(test)]\nmod tests {\n  fn evict_caches() {}\n}\n"],
    ["crates/rbit-services/tests/actions_sweeper.rs", "db.actions_artifacts().expired(now, 10).await.unwrap();\n"],
    ["crates/rbit-services/src/lfs.rs", "const STALE_UPLOAD_AGE: Duration = Duration::from_hours(24);\n"],
  ]), []);
});

test("the current tree has no retired store path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsArtifactsCachesRetirement(files), []);
});
