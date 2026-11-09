import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkPullsRetirement as check } from "../rewrite/pulls-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("branch and head selection are defined only in collaboration", () => {
  const source = "fn branch_ref(branch: &str) -> Result<String, ServiceError> { todo!() }\n";
  assert.match(check([["crates/rbit-services/src/pulls.rs", source]])[0], /belong to rbit_collaboration::pulls/u);
  assert.deepEqual(check([["crates/rbit-collaboration/src/pulls/branches.rs", source]]), []);
  assert.deepEqual(check([["crates/rbit-services/src/pulls.rs", "let full = branch_ref(branch).ok_or_else(|| todo!())?;\n"]]), []);
});

test("opening policy text and the insert outcome stay with the application and adapter", () => {
  for (const text of [
    "\"pull requests cannot be opened against a mirror\"",
    "format!(\"{base_branch} already contains every commit of {head_branch}\")",
    "\"choose existing branches to compare\"",
  ]) {
    assert.match(check([["crates/rbit-services/src/pulls.rs", `${text}\n`]])[0], /policy text belongs/u, text);
    assert.deepEqual(check([["crates/rbit-collaboration/src/pulls/creation.rs", `${text}\n`]]), [], text);
    assert.deepEqual(check([["crates/rbit-services/tests/pulls.rs", `${text}\n`]]), [], "tests assert the text");
  }
  const insert = "match store.create(&new).await? { CreatedPull::Created(pair) => *pair, CreatedPull::Duplicate(n) => todo!() }\n";
  assert.match(check([["crates/rbit-services/src/pulls.rs", insert]])[0], /outside the PullCreation adapter/u);
  assert.deepEqual(check([["crates/rbit-services/src/pulls/creation.rs", insert]]), []);
  assert.deepEqual(check([["crates/rbit-storage/src/sqlite/pulls.rs", insert]]), []);
});

test("the current tree has no retired pull creation path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/pulls.rs", root), "utf8"), /PullCreation::new/u);
});
