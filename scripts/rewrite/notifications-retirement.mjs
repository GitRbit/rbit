/** Retired notification inbox paths must not return; see docs/design/rewrite/notifications-inbox-acceptance.md. */

// Production reaches the notification store only through the feature port adapter.
const STORE_ACCESS = /\.notifications\(\)/gu;
const ADAPTER = /NotificationInboxStore::new\(\s*[\w.&]*$/u;
const STORE_PATH = /\bNotificationStore\s*::/u;
// Fan-out and retention writes are feature decisions, never direct port calls.
const DELIVERY_CALL = /\.(?:participate|notify_many|known_participants|prune_read)\(/u;
// Services expose feature models from their defining crate, not the storage adapter.
const STORAGE_MODELS = /\brbit_storage::sqlite::notifications::\{?[^;]*\b(?:Notification|NotificationReason|Subscription|NotificationRecipient)\b/u;
const OWNERS = ["crates/rbit-storage/", "crates/rbit-notifications/"];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Unit-test modules inside sources are fixtures; integration tests are excluded by path. */
function withoutTestModules(text) {
  let result = text;
  for (let at = result.search(/#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{/u); at >= 0;
    at = result.search(/#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{/u)) {
    let depth = 0, end = result.indexOf("{", at);
    for (; end < result.length; end += 1) {
      if (result[end] === "{") depth += 1;
      else if (result[end] === "}" && --depth === 0) break;
    }
    result = result.slice(0, at) + result.slice(end + 1);
  }
  return result;
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkNotificationsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path) || !path.endsWith(".rs") || OWNERS.some((owner) => path.startsWith(owner))) continue;
    const text = withoutTestModules(code(source));
    for (const match of text.matchAll(STORE_ACCESS)) {
      if (!ADAPTER.test(text.slice(Math.max(0, match.index - 120), match.index))) {
        violations.push(`${path}: direct notification store access bypasses the rbit-notifications ports`);
        break;
      }
    }
    if (STORE_PATH.test(text)) violations.push(`${path}: NotificationStore is the storage adapter's; use the rbit-notifications ports`);
    if (DELIVERY_CALL.test(text)) violations.push(`${path}: fan-out and retention writes belong to rbit-notifications applications`);
    if (path.startsWith("crates/rbit-services/") && STORAGE_MODELS.test(text)) {
      violations.push(`${path}: import notification models from rbit_notifications, not the storage adapter`);
    }
  }
  return violations;
}
