import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryLifecycleRetirement as check } from "../rewrite/repository-lifecycle-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("listing statements run only behind RepositoryListings", () => {
  for (const source of [
    "self.store.list_by_owner(owner.id, viewer, admin).await?\n",
    "database.repositories().list_for_member(user, 100).await?\n",
    "let total = self.store.count_for_member(user).await?;\n",
    "self.store.page_for_member(user, limit, offset).await?\n",
    "self.store.list_recent_public(8).await?\n",
    "const DASHBOARD_LIMIT: u32 = 100;\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/repositories.rs", source]])[0], /bypasses rbit_repositories::discovery/u, source);
  }
  assert.deepEqual(check([["crates/rbit-services/src/repositories.rs", "pub async fn page_for_member(&self) {}\n"]]), []);
  assert.deepEqual(check([["crates/rbit-storage/src/repository_discovery.rs", "self.list_by_owner(owner, viewer, admin)\n"]]), []);
  assert.deepEqual(check([["crates/rbit-repositories/src/discovery/listings.rs", "const DASHBOARD_LIMIT: u32 = 100;\n"]]), []);
});

test("explore parsing, administrator deletion and its text stay in their owners", () => {
  const parse = "let sort = params.sort.as_deref().and_then(RepoSort::parse).unwrap_or_default();\n";
  assert.match(check([["crates/rbit-web/src/routes/home/explore.rs", parse]])[0], /RepoSearch::from_query/u);
  const deletion = "if !self.store.delete_repository(id).await? { return Err(ServiceError::NotFound); }\n";
  assert.match(check([["crates/rbit-services/src/admin.rs", deletion]])[0], /bypasses AdminRepositoryDeletion/u);
  assert.deepEqual(check([["crates/rbit-services/src/admin/repository_deletion.rs", deletion]]), []);
  const text = "ServiceError::Invalid(\"type the repository's full name exactly to confirm\".to_owned())\n";
  assert.match(check([["crates/rbit-services/src/admin.rs", text]])[0], /deletion policy text/u);
  assert.deepEqual(check([["crates/rbit-web/tests/admin.rs", text]]), [], "tests may assert the text");
});

test("the current tree has no retired lifecycle or listing path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(check(files), []);
});
