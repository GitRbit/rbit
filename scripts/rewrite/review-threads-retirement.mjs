/** Retired review-thread paths must not return; see docs/design/rewrite/collaboration-pulls-progress.md. */

const OWNER = "crates/rbit-collaboration/src/";
// Thread grouping, resolve/dismiss/form rules and request limits are rbit_collaboration::pulls::review_threads.
const ROUTE = "crates/rbit-web/src/routes/pulls.rs";
const ROUTE_POLICY = /replies\.entry\(|ReviewState::Commented\s*&&|can_write\(\)\s*\|\|\s*author\s*==|is_open\(\)\s*\|\|\s*pending_comments/u;
const REVIEWS = "crates/rbit-services/src/pulls/reviews.rs";
const SERVICE_POLICY = /root\.author\s*!=\s*Some\(|in_reply_to\.unwrap_or\(|review\.reviewer\s*==\s*Some\(actor|start_review\s*\|\||find_account_by_name|AccountKind::User|can_triage\s*\|\||saturating_sub\(DIFF_HUNK_LINES/u;
const POLICY_TEXT = /cannot be asked to review this pull request|the author cannot review their own pull request|reviewers can be requested/u;
const LIMIT = /\bconst\s+(?:MAX_REQUESTED_REVIEWERS|DIFF_HUNK_LINES)\b|\bReviewSide\s*\{\s*Left\s*=>/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkReviewThreadsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || !production(path)) continue;
    const text = code(source);
    if (path === ROUTE && ROUTE_POLICY.test(text)) {
      violations.push(`${path}: review thread rules decided in the route; use rbit_collaboration::pulls::review_threads`);
    }
    if (path === REVIEWS && SERVICE_POLICY.test(text)) {
      violations.push(`${path}: review thread rules returned to the service; use rbit_collaboration::pulls::review_threads`);
    }
    if (!path.startsWith(OWNER) && (POLICY_TEXT.test(text) || LIMIT.test(text))) {
      violations.push(`${path}: review request limits and refusals belong to rbit_collaboration::pulls::review_threads`);
    }
  }
  return violations;
}
