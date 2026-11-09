import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkPresentationRetirement } from "../rewrite/presentation-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkPresentationRetirement([[path, source]]);

test("a second renderer, cut, theme catalog or app document fails", () => {
  const cases = [
    ["crates/rbit-web/src/render/markdown.rs", "fn sanitizer() -> &'static ammonia::Builder<'static> {}\n", /HTML sanitising/u],
    ["crates/rbit-services/src/x.rs", "let clean = ammonia::clean(&html);\n", /HTML sanitising/u],
    ["crates/rbit-web/src/render/markdown.rs", "use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};\n", /rendered to HTML/u],
    ["crates/rbit-mail/src/x.rs", "html::push_html(&mut out, parser);\n", /rendered to HTML/u],
    ["crates/rbit-web/src/urls.rs", "pub enum ResolvedLink { Unchanged }\n", /link types redefined/u],
    ["crates/rbit-web/src/render/markdown_bounds.rs", "pub fn raw_depth(html: &str, open: usize) -> (usize, usize) {}\n", /renderer function/u],
    ["crates/rbit-web/src/render/x.rs", "fn unique_slug(seen: &mut HashMap<String, usize>, text: &str) -> String {}\n", /renderer function/u],
    ["crates/rbit-web/src/routes/repository_documents.rs", "const MAX_DOCUMENTS: usize = 32;\n", /document tab selection/u],
    ["crates/rbit-web/src/routes/repo.rs", "fn readme_priority(name: &str) -> usize { 0 }\n", /document tab selection/u],
    ["crates/rbit-web/src/partial.rs", "pub const TITLE_START: &str = \"<!--partial:title-->\";\n", /marker redefined/u],
    ["crates/rbit-web/src/partial.rs", "pub fn slice(page: &[u8], scope: &[u8]) -> Option<Vec<u8>> {}\n", /cut redefined/u],
    ["crates/rbit-web/src/themes/mod.rs", "pub enum Theme { System }\n", /theme catalog/u],
    ["crates/rbit-web/src/themes/catalog.rs", "pub static THEMES: &[TintedTheme] = &[];\n", /generated into rbit-presentation/u],
    ["crates/rbit-web/src/routes/pwa.rs", "let m = json!({ \"start_url\": \"/\" });\n", /manifest, worker/u],
    ["crates/rbit-web/src/routes/pwa.rs", "String::from_utf8_lossy(&bytes).replace(\"__VERSION__\", version)\n", /manifest, worker/u],
    ["crates/rbit-web/src/routes/pwa.rs", "format!(\".btn-primary{{background-color:{}}}\", c)\n", /manifest, worker/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source).join("\n"), message, source);
});

test("adapters, test literals, comments, Markdown parsing and the owner crate are allowed", () => {
  assert.deepEqual(checkPresentationRetirement([
    ["crates/rbit-web/src/render/mod.rs", "use rbit_presentation::markdown::{self, LinkResolver};\n"],
    ["crates/rbit-web/src/partial.rs", "pub use rbit_presentation::navigation::{MAIN_END, slice};\nlet partial = slice(&bytes, scope.as_bytes());\n"],
    ["crates/rbit-web/src/routes/repository_documents.rs", "documents::sort_and_bound(&mut found);\n"],
    ["crates/rbit-web/src/middleware/security.rs", "assert!(html.contains(\"<!--partial:title-->\"));\n"],
    ["crates/rbit-web/src/routes/pwa.rs", "assert!(!source.contains(\"__VERSION__\"));\n// .replace(\"__VERSION__\", v) moved\n"],
    ["crates/rbit-core/src/references.rs", "use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};\n"],
    ["crates/rbit-web/tests/properties.rs", "/// Decodes the entities ammonia emits in attribute values.\n"],
    ["crates/rbit-presentation/src/markdown.rs", "use pulldown_cmark::{html};\nfn configured_builder() -> ammonia::Builder<'static> {}\n"],
    ["crates/rbit-presentation/src/themes/catalog.rs", "pub static THEMES: &[TintedTheme] = &[];\n"],
  ]), []);
});

test("the current tree has no retired presentation path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkPresentationRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-web/src/urls.rs", root), "utf8"), /impl LinkResolver for DocumentBase/u);
});
