import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkIssuesRetirement as check } from "../rewrite/issues-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("permission tables are defined only in collaboration", () => {
  for (const source of [
    "pub struct IssuePermissions { pub can_comment: bool }\n",
    "pub fn issue_permissions(viewer: Option<&User>, access: RepoAccess, issue: &Issue) -> IssuePermissions { todo!() }\n",
    "pub fn comment_permissions(v: Option<&User>, p: IssuePermissions, c: &Comment) -> CommentPermissions { todo!() }\n",
  ]) {
    assert.match(check([["crates/rbit-core/src/issue.rs", source]])[0], /belong to rbit_collaboration::threads/u, source);
    assert.deepEqual(check([["crates/rbit-collaboration/src/threads/permissions.rs", source]]), [], source);
  }
  assert.deepEqual(check([["crates/rbit-core/src/issue.rs", "pub use rbit_collaboration::threads::{IssuePermissions, issue_permissions};\n"]]), []);
});

test("the timeline adapter does not decide archived permissions", () => {
  const legacy = "if archived { IssuePermissions::default() } else { issue_permissions(viewer, access, issue) }\n";
  assert.match(check([["crates/rbit-services/src/timeline.rs", legacy]])[0], /returned to the timeline adapter/u);
  assert.deepEqual(check([["crates/rbit-web/src/views/timeline.rs", "IssuePermissions { can_comment: true, ..IssuePermissions::default() }\n"]]), []);
});

test("IssueService routes to the applications", () => {
  for (const source of [
    "let accounts = self.ctx.database.issue_meta().accounts_named(names).await?;\n",
    "store.find_label(repo, *id).await?\n",
    "self.ctx.database.accounts().find_active_user(*id).await?\n",
    "let grants = viewer_grants(&repositories, Some(user), repo).await?;\n",
    "self.ctx.database.issue_meta().assignable(repo, prefix.trim(), MAX_ASSIGNABLE).await?\n",
    "if row.state != rbit_core::issue::ReviewState::Pending {}\n",
    "self.record_mentions(*issue, body).await?;\n",
    "self.references.record_cross_references(&source, *actor, &body).await\n",
    "snapshot.ids().filter(|tip| *tip != after)\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/issues.rs", source]])[0], /returned to IssueService/u, source);
    assert.match(check([["crates/rbit-services/src/issues/creation.rs", source]])[0], /IssueService/u, source);
    assert.deepEqual(check([["crates/rbit-services/src/issues/ports.rs", source]]), [], source);
  }
  assert.deepEqual(check([["crates/rbit-services/src/issues.rs", "self.participants(&view).assignable(repo.access.can_read(), prefix)\n"]]), []);
  assert.deepEqual(check([["crates/rbit-services/src/pulls/reviews.rs", "self.ctx.database.reviews().find(pull, id)\n"]]), [], "pull reviews keep their owner");
});

test("participant refusal text stays in the application", () => {
  const text = "invalid(format!(\"{} cannot be assigned: only people who can push to the repository can\", user.name()))\n";
  assert.match(check([["crates/rbit-services/src/pulls.rs", text]])[0], /refusal text belongs/u);
  assert.deepEqual(check([["crates/rbit-collaboration/src/issues/participants.rs", text]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/issues.rs", text]]), [], "tests assert the text");
});

test("the current tree has no retired timeline or issue path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/issues.rs", root), "utf8"), /ThreadActivity::new/u);
});
