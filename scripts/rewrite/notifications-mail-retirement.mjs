/** Retired notification mail and webhook policy must not return; see docs/design/rewrite/notifications-mail-acceptance.md. */
import { code, withoutTestModules } from "./notifications-retirement.mjs";

const OUTBOX = "crates/rbit-services/src/notification_email/outbox.rs";
const SETTINGS = "crates/rbit-services/src/notification_email.rs";
const OWNERS = ["crates/rbit-storage/", "crates/rbit-notifications/"];
// Digest and claim stores are reached only by the port adapter; settings may read preferences.
const DIGEST_STORE = /\.email_digests\(\)/u;
const MAIL_STORE = /\.notification_email\(\)/u;
const CLAIM_OR_RECIPIENTS = /\.(?:claim_many|recipients)\(/u;
// Decisions that moved into rbit-notifications must not be redefined beside their adapters.
const POLICY_FN = /\bfn\s+(?:sign_link|verify_link|link_mac|wants|reason_clause|message_id_host|excerpt_of|one_line_excerpt|capitalised|prune_cutoff|signature|retry_delay|request_headers|check_secret|next_step)\b/u;
const POLICY_TOKEN = /\bHmac\s*<|\b(?:RETRY_DELAYS_SECONDS|LINK_PURPOSE|MIN_PENDING_RETENTION|FIRST_MONDAY)\b/u;
const POLICY_CONST = /\bconst\s+(?:MAX_SECRET_LEN|MAX_HOOKS_PER_OWNER|MAX_RESPONSE_BODY|MAX_RESPONSE_HEADERS|USER_AGENT|MAX_EXCERPT_CHARS)\b/u;
// Webhook queueing and worker bookkeeping reach the store only through the port adapter.
const WEBHOOK_PORTS = "crates/rbit-services/src/webhooks/ports.rs";
const WEBHOOK_STORE_CALL = /\.store\.(?:subscribed|enqueue|claim_due|record_attempt)\(|\bAttemptOutcome::/u;
const WEBHOOK_POLICY = /\bconst\s+(?:ZEN|NULL_SHA|MAX_PUSH_COMMITS)\b|\.retain\(\|hook\|/u;
const FEATURE_ADAPTERS = /^crates\/rbit-services\/src\/(?:notification_email|webhooks)(?:\.rs|\/)/u;

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkNotificationsMailRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || !path.endsWith(".rs") || OWNERS.some((owner) => path.startsWith(owner))) continue;
    const text = withoutTestModules(code(source));
    if (path !== OUTBOX && DIGEST_STORE.test(text)) {
      violations.push(`${path}: digest store access bypasses the rbit-notifications mail ports`);
    }
    if (path !== OUTBOX && path !== SETTINGS && MAIL_STORE.test(text)) {
      violations.push(`${path}: notification email store access belongs to the mail adapter or settings`);
    }
    if (path !== OUTBOX && MAIL_STORE.test(text) && CLAIM_OR_RECIPIENTS.test(text)) {
      violations.push(`${path}: email claims and recipient reads go through the rbit-notifications mail ports`);
    }
    if (path !== WEBHOOK_PORTS && WEBHOOK_STORE_CALL.test(text)) {
      violations.push(`${path}: webhook queueing and attempts go through the rbit-notifications webhook ports`);
    }
    if (FEATURE_ADAPTERS.test(path) && (POLICY_FN.test(text) || POLICY_TOKEN.test(text) || POLICY_CONST.test(text) || WEBHOOK_POLICY.test(text))) {
      violations.push(`${path}: mail and webhook delivery policy belongs to rbit-notifications`);
    }
  }
  return violations;
}
