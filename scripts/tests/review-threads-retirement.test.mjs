import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkReviewThreadsRetirement as check } from "../rewrite/review-threads-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the route does not group threads or decide who may act", () => {
  for (const source of [
    "replies.entry(comment.reply_to).or_default().push(comment);\n",
    "if threads.is_empty() && entry.review.state == ReviewState::Commented && entry.review.body.trim().is_empty() {}\n",
    "viewer.is_some_and(|user| ctx.repo().access.can_write() || author == Some(user.id()))\n",
    "(can_comment && (ctx.issue().is_open() || pending_comments > 0)).then(|| todo!())\n",
  ]) {
    assert.match(check([["crates/rbit-web/src/routes/pulls.rs", source]])[0], /decided in the route/u, source);
    assert.deepEqual(check([["crates/rbit-web/src/routes/issues.rs", source]]), [], source);
  }
});

test("the review service delegates thread rules and request limits", () => {
  for (const source of [
    "if !ctx.repo().access.can_write() && root.author != Some(actor.id()) {}\n",
    "in_reply_to: Some(parent.in_reply_to.unwrap_or(parent.id)),\n",
    "Some(review) => review.state != ReviewState::Pending || review.reviewer == Some(actor.id()),\n",
    "if input.start_review || store.pending(pull, actor).await?.is_some() {}\n",
    "let account = self.ctx().database.accounts().find_account_by_name(name).await?;\n",
    "Some(account) if account.kind == rbit_core::AccountKind::User => {}\n",
    "if !(permissions.can_triage || (is_author && permissions.can_comment)) {}\n",
    "let start = (end + 1).saturating_sub(DIFF_HUNK_LINES);\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/pulls/reviews.rs", source]])[0], /returned to the service/u, source);
  }
  const text = "format!(\"at most {MAX_REQUESTED_REVIEWERS} reviewers can be requested\")\n";
  assert.match(check([["crates/rbit-services/src/pulls/reviews.rs", text]]).at(-1), /belong to rbit_collaboration/u);
  assert.match(check([["crates/rbit-core/src/pull.rs", "pub const MAX_REQUESTED_REVIEWERS: usize = 15;\n"]])[0], /belong to/u);
  assert.match(check([["crates/rbit-services/src/pulls/reviews.rs", "pub const DIFF_HUNK_LINES: usize = 4;\n"]]).at(-1), /belong to/u);
  assert.match(check([["crates/rbit-core/src/pull.rs", "stored_enum!(ReviewSide { Left => \"left\", Right => \"right\" });\n"]])[0], /belong to/u);
  assert.deepEqual(check([["crates/rbit-collaboration/src/pulls/review_threads.rs", text]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/pulls.rs", text]]), [], "tests assert the text");
});

test("the current tree has no retired review-thread path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
});
