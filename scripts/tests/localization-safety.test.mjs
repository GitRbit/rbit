// Localization safety (docs/design/rewrite/security-contracts.md, group 6): catalog text is
// plain text. Templates never mark it trusted HTML, translations never carry markup or the
// private-use slot markers of the Actions rich messages, Markdown and Mermaid never receive
// it as their input, and the browser's catalog identities match the server's locales.
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { catalogKey } from "../../crates/rbit-web/assets/locale-key.js";

const repository = fileURLToPath(new URL("../../", import.meta.url));
const read = async (path) => (await readFile(join(repository, path), "utf8")).replace(/\r\n/g, "\n");

async function files(directory, extension) {
  const entries = await readdir(join(repository, directory), { recursive: true, withFileTypes: true });
  return entries.filter((entry) => entry.isFile() && entry.name.endsWith(extension))
    .map((entry) => relative(repository, join(entry.parentPath, entry.name)).replace(/\\/g, "/")).sort();
}

// Every `|safe` in a template, reviewed: [template, expression, what makes it trusted HTML].
// None is catalog text: catalog output is escaped by ordinary interpolation.
const SAFE = [
  ["actions/run/results.html", "summary.html", "a job summary rendered by render_markdown (sanitised)"],
  ["actions/step_body.html", "log.html", "log rows (views/actions/log_lines.rs): every line escaped before its ANSI colour spans are added"],
  ["admin/dashboard.html", "monitoring", "an Askama template (MonitoringPanel) that escapes its own fields"],
  ["base.html", "page.layout_foot()", "the layout's footer: an Askama template (layout/foot.html) that escapes its own values"],
  ["base.html", "page.layout_head()", "the layout's head: an Askama template (layout/head.html) that escapes its own values"],
  ["base.html", "page.layout_menus()", "the layout's top-bar menus: an Askama template (layout/menus.html) that escapes its own values"],
  ["base.html", "page.layout_top()", "the layout's top bar: an Askama template (layout/top.html) that escapes its own values"],
  ["issues/milestone.html", "description_html", "a milestone description rendered by render_markdown"],
  ["layout/head.html", "crate::assets::inline_css(\"app.css\")", "the embedded stylesheet, allowed by its CSP hash"],
  ["layout/head.html", "crate::assets::inline_css(path)", "the theme's embedded stylesheet, allowed by its CSP hash"],
  ["macros.html", "crate::icons::svg(name, class)", "an icon (src/icons.rs): fixed SVG markup, the class escaped as Askama escapes it"],
  ["pulls/review.html", "review.body_html", "a review body rendered by the issue Markdown renderer"],
  ["pulls/review_comment.html", "comment.body_html", "a line comment rendered by the issue Markdown renderer"],
  ["releases/card.html", "release.notes_html", "release notes rendered by render_markdown"],
  ["render_probe/shell_close.html", "page.layout_foot()", "the probe's copy of base.html"],
  ["render_probe/shell_open.html", "page.layout_head()", "the probe's copy of base.html"],
  ["render_probe/shell_open.html", "page.layout_menus()", "the probe's copy of base.html"],
  ["render_probe/shell_open.html", "page.layout_top()", "the probe's copy of base.html"],
  ["repo/blame.html", "line.html", "a highlighted line: escaped source inside highlighter spans"],
  ["repo/blob.html", "line.html", "a highlighted line: escaped source inside highlighter spans"],
  ["repo/blob.html", "html", "a repository document rendered by render_markdown, or its escaped source"],
  ["repo/documents.html", "readme.html", "a repository document rendered by render_markdown, or its escaped source"],
  ["settings/security.html", "setup.qr_svg", "the generated QR code of an authenticator secret"],
  ["timeline/comment.html", "comment.body_html", "a comment rendered by the issue Markdown renderer"],
  ["wiki/edit.html", "html", "a wiki preview rendered by render_markdown"],
  ["wiki/page.html", "html", "a wiki page rendered by render_markdown"],
];

// Words that would put catalog text into a trusted-HTML or Markdown position.
const CATALOG = /localized|Localized|Catalog|catalog|rbit_localization|Message\b|Message::|Label::|\.text\(|page\.text|\brich\b|UiText|\bt\.(?:get|rich|english)\b/;

const TEMPLATE_ROOTS = ["crates/rbit-web/templates/", "crates/rbit-mail/templates/"];

test("every |safe in a template is reviewed, and none is catalog text", async () => {
  const found = [];
  for (const root of TEMPLATE_ROOTS) {
    for (const path of await files(root, ".html")) {
      const source = await read(path);
      for (const [, expression] of source.matchAll(/\{\{-?\s*([^{}]*?)\s*\|\s*safe\b[^{}]*?-?\}\}/g)) {
        found.push([path.slice(root.length), expression.trim()]);
      }
      assert.doesNotMatch(source, /\{%-?\s*filter\b[^%]*\bsafe\b/, `${path}: a filter block marks content safe`);
      assert.doesNotMatch(source, /\|\s*(?:escape|e)\(\s*"none"\s*\)/, `${path}: escaping switched off`);
    }
  }
  const reviewed = SAFE.map(([template, expression]) => `${template} ${expression}`);
  for (const [template, expression] of found) {
    assert.ok(reviewed.includes(`${template} ${expression}`), `${template}: unreviewed |safe on ${expression}; review it and add it to SAFE`);
    assert.doesNotMatch(expression, CATALOG, `${template}: catalog text marked safe: ${expression}`);
  }
  for (const entry of reviewed) {
    assert.ok(found.some(([template, expression]) => `${template} ${expression}` === entry), `${entry} no longer occurs; drop it from SAFE`);
  }
});

test("no Rust code switches escaping off or declares a type HTML-safe", async () => {
  const sources = [...await files("crates/", ".rs"), ...await files("tools/", ".rs")].filter((path) => !path.includes("/target/"));
  assert.ok(sources.length > 500, "found the sources");
  const texts = await Promise.all(sources.map(read));
  for (const [index, path] of sources.entries()) {
    const source = texts[index];
    if (!/escape|HtmlSafe|MaybeSafe|template/.test(source)) continue;
    assert.doesNotMatch(source, /escape\s*=\s*"none"/, `${path}: a template without escaping`);
    assert.doesNotMatch(source, /impl\b[^{;]*\bHtmlSafe\s+for\b/, `${path}: a type that Askama would not escape`);
    assert.doesNotMatch(source, /\b(?:MaybeSafe|HtmlSafeOutput)\b/, `${path}: a filter output Askama would not escape`);
    assert.doesNotMatch(source, /#\[template\([^\]]*\|\s*safe\b/s, `${path}: an inline template marks content safe`);
  }
});

// Reviewed placeholders shown as text that look like a tag: [fragment, why].
const PLACEHOLDERS = [["Bearer <token>", "the shape of an authorization header, written as text"]];

const decode = (literal) => literal
  .replace(/\\u\{([0-9a-fA-F]+)\}/g, (_, hex) => String.fromCodePoint(Number.parseInt(hex, 16)))
  .replace(/\\(["\\nt'])/g, (_, character) => ({ n: "\n", t: "\t" })[character] ?? character);

// What a catalog literal must never contain, and why.
const FORBIDDEN = [
  [/<\/?[A-Za-z!][^<>]*>?/, "HTML: catalog text is escaped, so a tag would show as text"],
  [/&(?:#[0-9]+|#x[0-9a-f]+|[a-z]+);/i, "an HTML entity: write the character itself"],
  [/\{\{|\{%|%\}|\}\}/, "a template delimiter: catalog text is never evaluated"],
  [/[\uE000-\uF8FF]/u, "a private-use character: the Actions rich messages read U+E000–U+E003 as slots"],
  [/[\u202A-\u202E\u2066-\u2069\u200E\u200F\u061C]/u, "a bidirectional control: arguments are isolated by the ar-XB diagnostic only"],
  [/[\u0000-\u0008\u000B-\u001F\u007F]/u, "a control character"],
];

test("catalog wording and translations carry no markup, entities, slot markers or bidi controls", async () => {
  const catalog = (await files("crates/rbit-localization/src/catalog/", ".rs"))
    .filter((path) => !/\/(?:slot_)?tests\.rs$/.test(path));
  assert.ok(catalog.some((path) => path.includes("/translations/de/")), "found the translation tables");
  let literals = 0;
  for (const path of catalog) {
    const source = (await read(path)).split("#[cfg(test)]")[0];
    for (const [, literal] of source.matchAll(/"((?:[^"\\\n]|\\.)*)"/g)) {
      literals += 1;
      let text = decode(literal);
      for (const [fragment] of PLACEHOLDERS) text = text.replaceAll(fragment, "");
      for (const [pattern, why] of FORBIDDEN) assert.doesNotMatch(text, pattern, `${path}: ${JSON.stringify(literal)}: ${why}`);
    }
  }
  assert.ok(literals > 20000, `scanned ${literals} literals`);
  for (const [fragment, why] of PLACEHOLDERS) {
    const used = await Promise.all(catalog.map(async (path) => (await read(path)).includes(fragment)));
    assert.ok(used.some(Boolean), `${fragment} (${why}) no longer occurs; drop it from PLACEHOLDERS`);
  }
});

const MARKDOWN_CALLS = /\b(render_markdown|render_markdown_rewriting|render_issue_markdown|document_jobs?\.document|\.document)\(/g;

/** The argument list of the call whose `(` is at `open`. */
function argumentsAt(source, open) {
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === "(") depth += 1;
    if (source[index] === ")" && --depth === 0) return source.slice(open + 1, index);
  }
  return source.slice(open + 1);
}

test("Markdown is rendered from user content only, never from catalog text", async () => {
  const sources = (await files("crates/rbit-web/src/", ".rs")).filter((path) => !/(?:^|\/)tests?(?:\.rs|\/)/.test(path));
  let calls = 0;
  for (const path of sources) {
    const source = (await read(path)).split("#[cfg(test)]")[0];
    for (const match of source.matchAll(MARKDOWN_CALLS)) {
      const before = source.slice(Math.max(0, match.index - 8), match.index);
      if (/fn\s+$/.test(before)) continue;
      const argumentsText = argumentsAt(source, match.index + match[0].length - 1);
      calls += 1;
      assert.doesNotMatch(argumentsText, CATALOG, `${path}: Markdown rendered from catalog text: ${match[1]}(${argumentsText})`);
    }
  }
  assert.ok(calls >= 7, `found the Markdown call sites (${calls})`);
  const mermaid = await read("crates/rbit-web/assets/markdown.js");
  assert.match(mermaid, /const selector = 'pre > code\.language-mermaid';/);
  const posted = [...mermaid.matchAll(/postMessage\(([^;]*)\)/g)].map(([, message]) => message);
  assert.deepEqual(posted, ["{ type: 'rbit:mermaid', source: code.textContent }, '*'"], "the diagram frame receives the fence's own text");
  assert.doesNotMatch(mermaid, /innerHTML|insertAdjacentHTML|outerHTML/, "Mermaid's page script writes text, never HTML");
});

test("templates add no inline scripts, handlers or style attributes", async () => {
  for (const path of await files("crates/rbit-web/templates/", ".html")) {
    const source = await read(path);
    for (const [tag] of source.matchAll(/<script\b[^>]*>/g)) assert.match(tag, /\bsrc=/, `${path}: inline script ${tag}`);
    assert.doesNotMatch(source, /<[a-z][^<>]*\son[a-z]+\s*=/i, `${path}: an inline event handler`);
    assert.doesNotMatch(source, /<[a-z][^<>]*\sstyle\s*=/i, `${path}: a style attribute`);
  }
});

test("the browser accepts exactly the catalog identities the server sends", async () => {
  const locale = await read("crates/rbit-localization/src/locale.rs");
  const body = locale.slice(locale.indexOf("pub const fn catalog_key"), locale.indexOf("pub const fn tag"));
  const keys = [...body.matchAll(/=> "([^"]+)"/g)].map(([, key]) => key);
  const variants = [...locale.slice(locale.indexOf("pub enum Locale"), locale.indexOf("impl Locale")).matchAll(/^\s{4}([A-Z][A-Za-z]+),$/gm)];
  assert.equal(keys.length, variants.length, "every locale has a catalog key");
  assert.equal(new Set(keys).size, keys.length, "catalog keys are distinct");
  // An unknown key makes the browser skip its catalog-mismatch reload and its navigation
  // cache, so a fragment of another language could be swapped into the page.
  for (const key of keys) assert.equal(catalogKey(key), key, `${key} is rejected by locale-key.js`);
});
