// The page chrome (top bar, repository tabs, footer) mirrors in right-to-left documents
// through logical properties and the inherited `dir`, never through per-direction copies.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const web = new URL("../../crates/rbit-web/", import.meta.url);
const read = async (path) => (await readFile(new URL(path, web), "utf8")).replace(/\r\n/g, "\n");
const css = async (path) => (await read(`styles/${path}`)).replace(/\/\*[\s\S]*?\*\//g, "");

const CHROME_TEMPLATES = [
  "templates/base.html", "templates/render_probe/shell_open.html", "templates/render_probe/shell_close.html",
  "templates/repo/nav_context.html", "templates/repo/subnav.html", "templates/repo/star_button.html",
];
// Tailwind utilities (with any variant prefix) tied to a physical side.
const PHYSICAL_UTILITY = /(?:^|[\s"':])-?(?:m[lr]|p[lr]|left|right|rounded-[lr]|rounded-[tb][lr]|border-[lr]|scroll-[mp][lr]|text-left|text-right|float-left|float-right|space-x)(?:-|(?=[\s"']))/;
const PHYSICAL_PROPERTY = /(?:^|[;{\s])(?:margin|padding|border|inset|scroll-margin|scroll-padding)-(?:left|right)\b|(?:^|[;{\s])(?:left|right)\s*:|text-align:\s*(?:left|right)|float:\s*(?:left|right)|row-reverse|\bto (?:left|right)\b/;

function classAttributes(source) {
  return [...source.matchAll(/\bclass="([^"]*)"/g)].map((match) => match[1]);
}

test("chrome templates use only logical sides", async () => {
  for (const path of CHROME_TEMPLATES) {
    const source = await read(path);
    for (const value of classAttributes(source)) {
      assert.doesNotMatch(` ${value} `, PHYSICAL_UTILITY, `${path}: class="${value}"`);
    }
    assert.doesNotMatch(source, /row-reverse/, `${path} reverses a row instead of following dir`);
  }
});

test("the header, tabs and footer inherit the document direction", async () => {
  const base = await read("templates/base.html");
  assert.match(base, /<html lang="\{\{ page\.locale\.tag\(\) \}\}" dir="\{\{ page\.locale\.direction\(\)\.as_str\(\) \}\}"/);
  for (const [name, open] of [["header", /<header\b[^>]*>/], ["tabs", /<div class="site-subnav"[^>]*>/], ["footer", /<footer\b[^>]*>/]]) {
    const tag = base.match(open)?.[0];
    assert.ok(tag, `${name} is in base.html`);
    assert.doesNotMatch(tag, /\bdir=/, `${name} must not pin its own direction: ${tag}`);
  }
  for (const path of CHROME_TEMPLATES.slice(3)) {
    assert.doesNotMatch(await read(path), /\bdir="(?:ltr|rtl)"/, `${path} pins a direction`);
  }
});

test("chrome component rules use only logical sides", async () => {
  const app = await css("app.css");
  const chrome = /^\s*(?:\.(?:menu|menu-item|menu-icon|menu-label|menu-divider|tab|scroll-tabs|repo-subnav|repo-title|repo-actions|repo-tabs|site-header|site-subnav|notification-dot|btn[\w-]*)\b|#page-progress|details > summary)/;
  const rules = [...app.matchAll(/(?:^|\n)([^{}\n@]+)\{([^{}]*)\}/g)].filter(([, selector]) => chrome.test(selector));
  assert.ok(rules.length >= 10, "found the chrome rules");
  for (const [, selector, body] of rules) {
    assert.doesNotMatch(` ${body}`, PHYSICAL_UTILITY, `${selector.trim()} applies a physical-side utility`);
    assert.doesNotMatch(body, PHYSICAL_PROPERTY, `${selector.trim()} declares a physical side`);
  }
  const progress = app.match(/@keyframes page-progress\s*\{([\s\S]*?)\n\}/)[1];
  for (const step of progress.match(/translateX\([^;]*\)/g)) {
    assert.match(step, /var\(--inline-sign\)/, "the progress bar runs along the reading direction");
  }
});

test("direction tokens follow every explicit dir, so LTR islands stay LTR", async () => {
  const direction = await css("direction.css");
  assert.match(await read("styles/app.css"), /@import "\.\/direction\.css";/);
  assert.match(direction, /:root,\s*\[dir="ltr"\]\s*\{\s*--inline-sign:\s*1;\s*--inline-end:\s*right;\s*--inline-start:\s*left;\s*\}/);
  assert.match(direction, /\[dir="rtl"\]\s*\{\s*--inline-sign:\s*-1;\s*--inline-end:\s*left;\s*--inline-start:\s*right;\s*\}/);
  assert.match(direction, /\.dir-icon\s*\{\s*scale:\s*var\(--inline-sign\) 1;\s*\}/);
  assert.match(direction, /@utility rotate-inline-90\s*\{\s*rotate:\s*calc\(var\(--inline-sign\) \* 90deg\);\s*\}/);
});

test("directional icons, and only those, are marked to mirror", async () => {
  const macros = await read("templates/macros.html");
  const marker = macros.match(/\{% macro icon\(name, class\) %\}\n<svg class="\{\{ class \}\}(.*?)" viewBox=/)?.[1];
  assert.ok(marker, "the icon macro marks its svg");
  const flipped = [...marker.matchAll(/name == "([a-z-]+)"/g)].map((match) => match[1]).sort();
  assert.deepEqual(flipped, ["chevron-left", "chevron-right", "log-out"]);
  assert.match(marker, /\bdir-icon\b/);
});

test("the run's commit and ref wrap together as one group", async () => {
  const header = await read("templates/actions/run/header.html");
  const commit = header.match(/<dt[^>]*>\{\{ text\.commit \}\}<\/dt>\s*<dd([^>]*)>([\s\S]*?)<\/dd>/);
  assert.ok(commit, "the commit cell is a dt/dd pair");
  const [, attributes, body] = commit;
  assert.match(attributes, /\bflex\b/, "the sha and ref share one flex line");
  assert.match(attributes, /\bflex-wrap\b/, "and wrap as a group on narrow screens");
  assert.doesNotMatch(attributes, PHYSICAL_UTILITY);
  const ref = body.match(/<bdi\b[^>]*>\{\{ run\.reference \}\}<\/bdi>/)?.[0];
  assert.ok(ref, "the ref is isolated");
  // A block-level `bdi` resolves its own (LTR) direction and aligns to the left edge on
  // its own line inside a right-to-left cell.
  assert.doesNotMatch(ref, /\bblock\b/);
  assert.match(body, /<bdi dir="ltr">\{\{ run\.short_sha \}\}<\/bdi>/, "the sha stays an LTR identifier");
});
