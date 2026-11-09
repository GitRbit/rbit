/** Retired poll-loop and stale-task paths must not return; see docs/design/rewrite/actions-scheduling-acceptance.md. */

// The long poll (admission, one poll per runner, the claim-or-wait loop) and the sweep's
// zombie, endless and abandoned passes are decided in rbit_actions::{polling, stale_tasks}.
// Their adapters are the only places that reach the queue and the stale-task SQL.
const OWNER = "crates/rbit-actions/";
const ADAPTERS = ["crates/rbit-services/src/actions/scheduler/poll_queue.rs", "crates/rbit-services/src/actions/stale_task_port.rs"];
const SERVER = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/"];
const RULES = [
  [/\bstruct\s+PollSlot\b|\bby_runner\s*:\s*Mutex\b/u, "poll registry redefined; use rbit_actions::polling::PollSlots"],
  [/Idempotency-Key must be 1 to 64|holds as many polls as it may|is disabled and takes no new jobs/u,
    "poll refusal text outside rbit_actions::polling"],
  [/\bversion\s*!=\s*[\w.]*idle_version\b|\bidle_version\s*\.\s*load\([^)]*\)\s*!=/u, "poll claim-or-wait decision outside rbit_actions::polling::LongPoll"],
  [/\.\s*(?:silent_tasks|long_running_tasks|cancel_abandoned)\s*\(/u, "stale-task selection outside the stale-task port"],
  [/sent no word for|ran longer than \{|\bfn\s+time_out\b/u, "stale-task pass redefined; use rbit_actions::stale_tasks::StaleTasks"],
];

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** An inline unit-test module conventionally closes a production file; `mod x;` declarations do not. */
function production(source) {
  const tests = source.search(/#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/u);
  return tests < 0 ? source : source.slice(0, tests);
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsPollSweepRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || path.startsWith(OWNER) || ADAPTERS.includes(path) || !SERVER.some((root) => path.startsWith(root))) continue;
    const text = production(code(source));
    for (const [pattern, message] of RULES) {
      if (pattern.test(text)) violations.push(`${path}: ${message}`);
    }
  }
  return violations;
}
