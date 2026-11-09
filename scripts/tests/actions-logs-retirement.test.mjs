import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsLogsRetirement } from "../rewrite/actions-logs-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsLogsRetirement([[path, source]]);

test("a second masking pass, line rule, budget or append limit fails in the adapters", () => {
  const cases = [
    ["crates/rbit-services/src/actions/logs/masking.rs", "pub struct Masker { automaton: Option<AhoCorasick> }\n", /masking pass/u],
    ["crates/rbit-web/src/x.rs", "let a = AhoCorasick::new(p);\n", /masking pass/u],
    ["crates/rbit-services/src/actions/logs/append.rs", "const MARKER: &str = \" [line truncated]\";\n", /line cleaning/u],
    ["crates/rbit-server/src/x.rs", "format!(\"rbit: the log was cut here: {max}\")\n", /budget/u],
    ["crates/rbit-services/src/actions/logs/types.rs", "pub const MAX_LINES_PER_CALL: usize = 10_000;\n", /append limit/u],
    ["crates/rbit-services/src/actions/logs/append.rs", "let mut lines: Vec<&str> = text.split('\\n').collect();\nif text.ends_with('\\n') {\n    lines.pop();\n}\n", /body splitting/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source).join("\n"), message, source);
});

test("the runner's own pass, test modules, re-exports and comments are allowed", () => {
  assert.deepEqual(checkActionsLogsRetirement([
    ["crates/rbit-runner/src/mask.rs", "pub struct Masker { automaton: AhoCorasick }\n"],
    ["crates/rbit-services/src/actions/logs/tests.rs", "assert!(line.ends_with(\" [line truncated]\"));\n"],
    ["crates/rbit-services/src/actions/logs/masking.rs", "pub use rbit_actions::logs::{MASK, MIN_MASKED_CHARS, Masker};\n"],
    ["crates/rbit-services/src/actions/logs/types.rs", "pub use rbit_actions::logs::MAX_LINES_PER_CALL;\n"],
    ["crates/rbit-services/src/actions/logs/append.rs", "// once used AhoCorasick and \" [line truncated]\" here\n"],
  ]), []);
});

test("task log file I/O stays behind the TaskLogFiles port", () => {
  for (const source of [
    "let log = OpenOptions::new().create(true).append(true).open(&files.log)?;",
    "log.set_len(progress.bytes)?;",
    "index.sync_data()?;",
    "let mut encoder = flate2::write::GzEncoder::new(file, level);",
    "let end = u64::from_le_bytes(entry);",
    "tokio::fs::remove_file(&path).await",
  ]) {
    assert.match(check("crates/rbit-services/src/actions/logs/archive.rs", source).join("\n"), /TaskLogFiles/u, source);
  }
  assert.deepEqual(check("crates/rbit-services/src/actions/logs/disk.rs", "OpenOptions::new().create(true).append(true).open(path)"), []);
  assert.deepEqual(check("crates/rbit-services/src/actions/logs/batch_tests.rs", "std::fs::remove_file(&files.log).unwrap();"), []);
});

test("the current tree has no retired log rule", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsLogsRetirement(files), []);
});
