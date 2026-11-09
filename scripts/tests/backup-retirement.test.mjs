import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkBackupRetirement } from "../rewrite/backup-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkBackupRetirement([[path, source]]);

test("a returning inventory, SQL, snapshot or format definition fails", () => {
  const cases = [
    ["crates/rbit-server/src/backup/inventory.rs", "pub struct Inventory;\n", /inventory and pruning/u],
    ["crates/rbit-server/src/backup/prune/mod.rs", "pub fn prune() {}\n", /inventory and pruning/u],
    ["crates/rbit-server/src/backup/restore.rs", "use sqlx::Connection;\n", /database work returned/u],
    ["crates/rbit-server/src/backup/mod.rs", "let options = rbit_storage::sqlite::writer_compatibility::register(o);\n", /database work returned/u],
    ["crates/rbit-server/src/backup/copy.rs", "query(\"DELETE FROM lfs_objects WHERE oid = ?\")\n", /database work returned/u],
    ["crates/rbit-server/src/backup/restore.rs", "query(\"UPDATE wiki_repositories SET generation = NULL\")\n", /database work returned/u],
    ["crates/rbit-services/src/admin/backup.rs", "conn.execute(\"VACUUM  INTO ?\")\n", /database snapshot outside/u],
    ["crates/rbit-server/src/backup/mod.rs", "pub const FORMAT_VERSION: u32 = 7;\n", /format redefined/u],
    ["crates/rbit-server/src/backup/wikis.rs", "pub const SINCE_FORMAT: u32 = 6;\n", /format redefined/u],
    ["crates/rbit-server/src/backup/copy.rs", "const fn since_format(self) -> u32 { 2 }\n", /format redefined/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("storage, tests, comments, re-exports and file copies are allowed", () => {
  assert.deepEqual(checkBackupRetirement([
    ["crates/rbit-storage/src/sqlite/backup/mod.rs", "sqlx::query(\"VACUUM INTO ?\")\npub const FORMAT_VERSION: u32 = 6;\n"],
    ["crates/rbit-storage/src/identity_upgrade/active/tests/x.rs", "sqlx::query(\"VACUUM INTO ?\")\n"],
    ["crates/rbit-server/tests/backup.rs", "sqlx::query(\"SELECT COUNT(*) FROM repositories\")\n"],
    ["crates/rbit-server/src/backup/mod.rs", "//! written with `VACUUM INTO`\npub use rbit_storage::sqlite::backup::FORMAT_VERSION;\n"],
    ["crates/rbit-server/src/backup/restore.rs", "/* sqlx::query(\"SELECT 1\") */\nif manifest.format > FORMAT_VERSION {}\n"],
    ["crates/rbit-server/src/telemetry.rs", "const DEFAULT_FILTER: &str = \"info,sqlx=warn\";\n"],
    ["crates/rbit-server/src/backup/copy.rs", "tokio::fs::copy(from, to).await?;\n"],
  ]), []);
});

test("the current tree has no retired backup database path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkBackupRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-storage/src/sqlite/backup/mod.rs", root), "utf8"), /pub async fn snapshot_into/u);
});
