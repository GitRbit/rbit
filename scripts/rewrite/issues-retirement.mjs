/** Retired timeline and issue paths must not return; see docs/design/rewrite/collaboration-issues-acceptance.md. */

const OWNER = "crates/rbit-collaboration/src/";
// Thread and comment permission tables are defined once, in rbit_collaboration::threads.
const PERMISSION_TABLES = /\b(?:struct|enum)\s+(?:IssuePermissions|CommentPermissions)\b|\bfn\s+(?:issue_permissions|comment_permissions)\b/u;
// The archived-repository rule belongs to rbit_collaboration::threads::thread_permissions.
const TIMELINE = /^crates\/rbit-services\/src\/timeline(?:\.rs|\/)/u;
const TIMELINE_POLICY = /IssuePermissions::default\(\)/u;
// IssueService composes: participant lookups and subscriber reads live in its port adapter.
const SERVICE = /^crates\/rbit-services\/src\/issues(?:\.rs|\/)/u;
// Port adapters (participants/activity, listing) may name the stores they wrap.
const ADAPTERS = new Set(["crates/rbit-services/src/issues/ports.rs", "crates/rbit-services/src/issues/reader.rs"]);
const SERVICE_POLICY = /\b(?:accounts_named|find_label|find_active_user|viewer_grants|is_organization)\b|\.assignable\(\s*[^,)]+,|\.comment_thread\(\s*[^,)]+,|ReviewState::Pending|\.reviews\(\)|\.(?:record_mentions|record_cross_references)\(|\.ids\(\)\s*\.filter/u;
// Participant refusal text is owned by the application.
const POLICY_TEXT = /cannot be assigned: only people who can push|"unknown label"/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkIssuesRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs") || !production(path)) continue;
    const text = code(source);
    if (!path.startsWith(OWNER) && PERMISSION_TABLES.test(text)) {
      violations.push(`${path}: thread permission tables belong to rbit_collaboration::threads`);
    }
    if (TIMELINE.test(path) && TIMELINE_POLICY.test(text)) {
      violations.push(`${path}: thread permission policy returned to the timeline adapter; use thread_permissions`);
    }
    if (SERVICE.test(path) && !ADAPTERS.has(path) && SERVICE_POLICY.test(text)) {
      violations.push(`${path}: participant or subscriber policy returned to IssueService; use IssueParticipants or ThreadActivity`);
    }
    if (!path.startsWith(OWNER) && POLICY_TEXT.test(text)) {
      violations.push(`${path}: participant refusal text belongs to rbit_collaboration::issues`);
    }
  }
  return violations;
}
