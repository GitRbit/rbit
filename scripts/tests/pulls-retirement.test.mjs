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
    "\"you cannot approve or request changes on your own pull request\"",
    "\"write a comment or leave line comments first\"",
    "\"say why the review is dismissed\"",
    "\"only approvals and change requests can be dismissed\"",
    "format!(\"The base branch changed from {previous} to {branch}.\")",
    "ServiceError::Invalid(\"choose an existing branch\".to_owned())",
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

test("pull models and merge gating stay in collaboration", () => {
  for (const source of [
    "pub struct PullRequest { pub issue_id: IssueId }\n",
    "pub fn evaluate(facts: &MergeFacts<'_>, rules: &MergeRules) -> MergeEvaluation { todo!() }\n",
    "pub fn tally(reviews: &[Review]) -> ReviewTally { todo!() }\n",
  ]) {
    assert.match(check([["crates/rbit-core/src/pull.rs", source]])[0], /belong to rbit_collaboration::pulls/u, source);
    assert.deepEqual(check([["crates/rbit-collaboration/src/pulls/merge.rs", source]]), [], source);
  }
  for (const source of [
    "let stale = base_tip.is_none() || ctx.pull.mergeable_base_sha != base_tip;\n",
    "if !(input.bypass && evaluation.can_merge_with_bypass()) {}\n",
    "let title = if input.title.trim().is_empty() { title } else { input.title.trim() };\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/pulls/merging.rs", source]])[0], /merge gating returned/u, source);
  }
  assert.deepEqual(check([["crates/rbit-core/src/pull.rs", "pub use rbit_collaboration::pulls::{PullRequest, evaluate};\n"]]), []);
});

test("the current tree has no retired pull creation path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/pulls.rs", root), "utf8"), /PullCreation::new/u);
});
