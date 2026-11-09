/** Retired dashboard paths must not return; see docs/design/rewrite/collaboration-dashboards-acceptance.md. */

// The route parses, localizes and renders; view and default-search decisions are
// rbit_collaboration::issues::{DashboardRelation::from_view, DashboardRelation::views, dashboard_query}.
const ROUTE = "crates/rbit-web/src/routes/dashboards.rs";
const ROUTE_POLICY = /"(?:created|assigned|mentioned|review-requested|is:open)"|!=\s*DashboardRelation::ReviewRequested/u;
// Mention persistence is reached through rbit_collaboration::issues::DashboardMentions.
const MENTION_STORE = /\.issue_search\(\)\s*\.(?:record_mentions|mention_sources|mentions_indexed|finish_mentions_index)\(|\.(?:mention_sources|mentions_indexed|finish_mentions_index)\(/u;
const MENTION_OWNERS = ["crates/rbit-storage/", "crates/rbit-collaboration/src/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkDashboardsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || !production(path)) continue;
    const text = code(source);
    if (path === ROUTE && ROUTE_POLICY.test(text)) {
      violations.push(`${path}: dashboard view or default search decided in the route; use rbit_collaboration::issues`);
    }
    if (!MENTION_OWNERS.some(owner => path.startsWith(owner)) && MENTION_STORE.test(text)) {
      violations.push(`${path}: dashboard mention persistence bypasses rbit_collaboration::issues::DashboardMentions`);
    }
  }
  return violations;
}
