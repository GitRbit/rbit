import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkCollaboratorsRetirement } from "../rewrite/collaborators-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const ADAPTER = "crates/rbit-services/src/collaborators.rs";

test("the collaborator service adapter holds no decisions or raw stores", () => {
  for (const source of [
    "fn require_admin(repo: &RepoContext) {}",
    "async fn find_user(&self, username: &str) {}",
    "Err(StorageError::Conflict(_)) => {}",
    "self.ctx.database.collaborators().add(repo, user, permission, actor)",
    "if self.ctx.database.accounts().is_suspended(user.id).await? {}",
  ]) {
    assert.match(checkCollaboratorsRetirement([[ADAPTER, source]])[0], /belongs in rbit-access/u, source);
  }
  assert.deepEqual(checkCollaboratorsRetirement([[ADAPTER, "// fn require_admin retired\nRules::require_admin(scope(repo))"]]), []);
  assert.deepEqual(checkCollaboratorsRetirement([["crates/rbit-storage/src/access_collaborators.rs", "self.accounts.is_suspended(user)"]]), []);
});

test("the current tree has no retired collaborator path", () => {
  const { files } = authoredSources(fileURLToPath(new URL("../../", import.meta.url)));
  assert.deepEqual(checkCollaboratorsRetirement(files), []);
  assert.match(new Map(files).get(ADAPTER), /Collaborators::new/u);
});
