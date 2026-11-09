import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryEditsRetirement as check } from "../rewrite/repository-edits-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("edit refusal texts stay in the application", () => {
  for (const source of [
    'return Err(EditFailure::Invalid(format!("{name} was chosen twice")));\n',
    'Err(EditFailure::Invalid("enter a name for the new branch".to_owned()))\n',
    'format!("a branch called {name} already exists")\n',
    'pub const MIRROR_READ_ONLY: &str = "This repository is a mirror and cannot be edited; change its upstream instead";\n',
  ]) {
    assert.match(check([["crates/rbit-services/src/edits.rs", source]])[0], /policy text belongs/u, source);
    assert.match(check([["crates/rbit-web/src/routes/edits.rs", source]])[0], /policy text belongs/u, source);
    assert.deepEqual(check([["crates/rbit-repositories/src/edits.rs", source]]), []);
  }
  assert.deepEqual(check([["crates/rbit-web/tests/edits.rs", 'assert!(body.contains("was chosen twice"));\n']]), [], "tests assert the text");
});

test("the service does not re-decide access or gate classification", () => {
  for (const source of [
    "if repo.repo.repo.is_archived() { return Ok(Some(reason)); }\n",
    "CommitTarget::NewBranch(_) => EditFailure::Invalid(refusal.reason),\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/edits.rs", source]])[0], /returned to the service/u, source);
  }
  assert.deepEqual(check([["crates/rbit-services/src/timeline.rs", "ctx.repo.repo.repo.is_archived()\n"]]), []);
});

test("the current tree has no retired edit decision", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(check(files), []);
});
