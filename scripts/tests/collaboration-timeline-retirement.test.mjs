import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkCollaborationTimelineRetirement } from "../rewrite/collaboration-timeline-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const OPTIONS = "Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES | Options::ENABLE_GFM";
const scanner = ["crates/rbit-collaboration/src/references/markdown.rs", `pub fn markdown_options() -> Options { ${OPTIONS} }\n`];
const renderer = (options) => ["crates/rbit-web/src/render/markdown.rs", `fn options() -> Options { ${options} }\n`];
const check = (...files) => checkCollaborationTimelineRetirement([scanner, renderer(OPTIONS), ...files]);

test("a second bridge, its old path or Markdown parsing in core fails", () => {
  const cases = [
    ["crates/rbit-core/src/references.rs", "pub fn scan_markdown(source: &str) -> Scan { todo!() }\n", /scanning redefined/u],
    ["crates/rbit-services/src/x.rs", "fn markdown_options() -> Options { Options::empty() }\n", /scanning redefined/u],
    ["crates/rbit-services/src/references.rs", "use rbit_core::{AccountId, references::scan_markdown};\n", /retired rbit_core/u],
    ["crates/rbit-services/src/issues.rs", "close(pull, rbit_core::references::scan_markdown)\n", /retired rbit_core/u],
    ["crates/rbit-core/src/lib.rs", "use pulldown_cmark::Parser;\n", /rbit-core parses Markdown/u],
  ];
  for (const [path, source, message] of cases) assert.match(check([path, source]).join("\n"), message, source);
});

test("the scan's options must match safe rendering's, and the scan must exist", () => {
  assert.deepEqual(check(), []);
  const drift = checkCollaborationTimelineRetirement([scanner, renderer("Options::ENABLE_TABLES | Options::ENABLE_GFM")]);
  assert.match(drift.join("\n"), /differ from crates\/rbit-web/u);
  const moved = checkCollaborationTimelineRetirement([scanner, ["crates/rbit-presentation/src/markdown.rs", `fn options() -> Options { ${OPTIONS} }\n`]]);
  assert.deepEqual(moved, [], "the renderer may live in rbit-presentation");
  assert.match(checkCollaborationTimelineRetirement([renderer(OPTIONS)]).join("\n"), /scan is missing/u);
});

test("callers, parsing elsewhere and comments are allowed", () => {
  assert.deepEqual(check(
    ["crates/rbit-services/src/references.rs", "use rbit_collaboration::references::scan_markdown;\n"],
    ["crates/rbit-web/src/render/markdown.rs", `use pulldown_cmark::Parser;\nfn options() -> Options { ${OPTIONS} }\n`],
    ["crates/rbit-core/src/references.rs", "//! Markdown scanning is `rbit_collaboration::references::scan_markdown`.\n"],
  ), []);
});

test("the current tree has the scan in collaboration only", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkCollaborationTimelineRetirement(files), []);
  assert.doesNotMatch(readFileSync(new URL("crates/rbit-core/Cargo.toml", root), "utf8"), /pulldown-cmark/u);
});
