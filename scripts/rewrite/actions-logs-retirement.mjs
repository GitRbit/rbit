/** Retired task-log rules must not return; see docs/design/rewrite/actions-logs-acceptance.md. */

// The server's masking pass, append acceptance, line cleaning and the byte budget are
// decided in rbit-actions (`logs`). The runner keeps its own first masking pass (it masks
// base64 forms too, before anything leaves the machine), so rbit-runner is not checked.
const ADAPTERS = ["crates/rbit-services/src/", "crates/rbit-web/src/", "crates/rbit-server/src/"];
const TESTS = /\/(?:tests|batch_tests)\.rs$/u;
const RULES = [
  [/\bAhoCorasick\b|\bstruct\s+Masker\b/u, "server masking pass built outside rbit_actions::logs::Masker"],
  [/" \[line truncated\]"|the log was cut here/u, "log line cleaning or budget decided outside rbit_actions::logs"],
  [/\bconst\s+MAX_LINES_PER_CALL\s*:|lines per call"/u, "append limit decided outside rbit_actions::logs"],
  [/\.split\('\\n'\)[\s\S]{0,200}\.pop\(\)/u, "append body splitting outside rbit_actions::logs::split_body"],
];
// Log files are written, recovered, compressed, removed and read in rbit-actions through
// the TaskLogFiles port; only the disk adapter opens them in the services' logs module.
const LOG_MODULE = "crates/rbit-services/src/actions/logs/";
const DISK_ADAPTER = "crates/rbit-services/src/actions/logs/disk.rs";
const FILE_IO = /\b(?:OpenOptions|GzEncoder|GzDecoder|SeekFrom)\b|\.(?:set_len|sync_data|sync_all)\(|\bfrom_le_bytes\b|\b(?:remove_file|create_dir_all)\s*\(/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkActionsLogsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.endsWith(".rs") || TESTS.test(path) || !ADAPTERS.some((root) => path.startsWith(root))) continue;
    const text = code(source);
    for (const [pattern, message] of RULES) if (pattern.test(text)) violations.push(`${path}: ${message}`);
    if (path.startsWith(LOG_MODULE) && path !== DISK_ADAPTER && FILE_IO.test(text)) {
      violations.push(`${path}: task log file I/O belongs in rbit_actions::logs behind TaskLogFiles`);
    }
  }
  return violations;
}
