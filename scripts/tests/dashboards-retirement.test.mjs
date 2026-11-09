import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkDashboardsRetirement as check } from "../rewrite/dashboards-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const route = "crates/rbit-web/src/routes/dashboards.rs";

test("the route does not decide views or the default search", () => {
  for (const source of [
    "let relation = match params.view.as_deref().unwrap_or(\"created\") { _ => todo!() };\n",
    "\"review-requested\" if kind == IssueKind::Pull => DashboardRelation::ReviewRequested,\n",
    "let typed = q.unwrap_or(\"is:open\").to_owned();\n",
    "if kind == IssueKind::Pull || view != DashboardRelation::ReviewRequested {}\n",
  ]) {
    assert.match(check([[route, source]])[0], /decided in the route/u, source);
  }
  assert.deepEqual(check([[route, "DashboardRelation::ReviewRequested => Involvement::ReviewRequested,\n"]]), [], "labels stay in the route");
  assert.deepEqual(check([["crates/rbit-web/src/routes/pulls.rs", "IssueQuery::parse(\"is:open is:pr\")\n"]]), [], "other pages keep their searches");
  assert.deepEqual(check([["crates/rbit-collaboration/src/issues/dashboard.rs", "pub const DEFAULT_DASHBOARD_QUERY: &str = \"is:open\";\n"]]), []);
});

test("mention persistence goes through the application", () => {
  for (const source of [
    "self.ctx.database.issue_search().record_mentions(issue, &ids).await?;\n",
    "if store.mentions_indexed().await? { return Ok(()); }\n",
    "let rows = store.mention_sources(source, after).await?;\n",
    "store.finish_mentions_index().await?;\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/issues.rs", source]])[0], /bypasses rbit_collaboration::issues::DashboardMentions/u, source);
    assert.deepEqual(check([["crates/rbit-storage/src/collaboration_mentions.rs", source]]), [], source);
  }
  assert.deepEqual(check([["crates/rbit-services/src/issues.rs", "self.record_mentions(*issue, body).await?;\n"]]), [], "the subscriber delegates");
  assert.deepEqual(check([["crates/rbit-services/tests/access.rs", "fixture.database.issue_search().record_mentions(i, &ids).await\n"]]), [], "tests seed directly");
});

test("comments are allowed", () => {
  assert.deepEqual(check([[route, "// Absent views are \"created\"; blank searches are \"is:open\".\n"]]), []);
});

test("the current tree has no retired dashboard path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  assert.match(readFileSync(new URL(route, root), "utf8"), /DashboardRelation::from_view/u);
});
