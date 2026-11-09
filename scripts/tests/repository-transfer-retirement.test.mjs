import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryTransferRetirement as check } from "../rewrite/repository-transfer-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("old-address and owner-change SQL are reachable only from storage", () => {
  const fallback = "let repo = match store.find_by_path(o, n).await? { Some(r) => r, None => store.find_by_old_path(o, n).await?.unwrap() };\n";
  assert.match(check([["crates/rbit-services/src/lfs.rs", fallback]])[0], /bypasses rbit_repositories::aliases/u);
  assert.match(check([["crates/rbit-web/tests/x.rs", fallback]])[0], /bypasses/u, "tests are covered too");
  const write = "db.repositories().transfer(repo, owner).await?;\n";
  assert.match(check([["crates/rbit-services/src/admin.rs", write]])[0], /owner-change write bypasses/u);
  assert.match(check([["crates/rbit-web/src/x.rs", "RepositoryStore::transfer(&store, r, o)"]])[0], /bypasses/u);
  assert.deepEqual(check([["crates/rbit-storage/src/repository_transfer.rs", fallback + write]]), []);
  assert.deepEqual(check([["crates/rbit-storage/tests/redirects.rs", fallback]]), []);
});

test("the service adapter holds no transfer policy or persistence", () => {
  for (const source of [
    "fn require_admin(repo: &RepoContext) {}\n",
    "authorised_owner(&store, actor, name).await\n",
    "self.ctx.database.orgs().memberships_of(actor.id()).await?\n",
    "self.ctx.database.accounts().is_suspended(id).await?\n",
    "Err(StorageError::Conflict(_)) => {}\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/transfers.rs", source]])[0], /returned to the service adapter/u, source);
    assert.match(check([["crates/rbit-services/src/transfers/bridge.rs", source]])[0], /service adapter/u, source);
  }
  assert.deepEqual(check([["crates/rbit-services/src/orgs.rs", "self.ctx.database.orgs().memberships_of(id).await?\n"]]), []);
});

test("transfer policy text stays in the application", () => {
  const text = "return Err(ServiceError::Invalid(format!(\"{name} already belongs to {owner}\")));\n";
  assert.match(check([["crates/rbit-services/src/repositories.rs", text]])[0], /policy text belongs/u);
  assert.deepEqual(check([["crates/rbit-repositories/src/transfer.rs", text]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/transfers.rs", text]]), [], "tests assert the text");
});

test("comments and non-Rust files are allowed", () => {
  assert.deepEqual(check([
    ["crates/rbit-services/src/a.rs", "// store.find_by_old_path was retired\n/* db.repositories().transfer(r, o) */\n"],
    ["crates/rbit-services/src/transfers.rs", "//! No StorageError mapping or require_admin here.\n"],
    ["docs/x.md", "find_by_old_path"],
  ]), []);
});

test("the current tree has no retired transfer path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/transfers.rs", root), "utf8"), /RepositoryTransfer::suggested_owners/u);
});
