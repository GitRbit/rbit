// Every rbit-web page mirrors in right-to-left documents through logical properties and the
// inherited `dir`, never through per-direction copies. Code, diffs, logs and machine
// identifiers stay left-to-right inside explicit `dir="ltr"` islands.
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const web = new URL("../../crates/rbit-web/", import.meta.url);
const root = fileURLToPath(web);
const read = async (path) => (await readFile(new URL(path, web), "utf8")).replace(/\r\n/g, "\n");
const css = async (path) => (await read(`styles/${path}`)).replace(/\/\*[\s\S]*?\*\//g, "");
async function files(directory, extension) {
  const entries = await readdir(new URL(directory, web), { recursive: true, withFileTypes: true });
  return entries.filter((entry) => entry.isFile() && entry.name.endsWith(extension))
    .map((entry) => join(entry.parentPath, entry.name).slice(root.length).replace(/\\/g, "/")).sort();
}

const CHROME_TEMPLATES = [
  "templates/base.html", "templates/render_probe/shell_open.html", "templates/render_probe/shell_close.html",
  "templates/repo/nav_context.html", "templates/repo/subnav.html", "templates/repo/star_button.html",
];
// Tailwind utilities (with any variant prefix) tied to a physical side. `space-x-*` is not
// here: Tailwind 4 emits it as margin-inline.
const PHYSICAL_UTILITY = /(?:^|[\s"':])-?(?:m[lr]|p[lr]|left|right|rounded-[lr]|rounded-[tb][lr]|border-[lr]|scroll-[mp][lr]|text-left|text-right|float-left|float-right|origin-(?:top-|bottom-)?(?:left|right)|bg-(?:linear|gradient)-to-[lr]|flex-row-reverse)(?:-|(?=[\s"']))/;
const PHYSICAL_PROPERTY = /(?:^|[;{\s])(?:margin|padding|border|inset|scroll-margin|scroll-padding)-(?:left|right)\b|(?:^|[;{\s])(?:left|right)\s*:|text-align:\s*(?:left|right)|float:\s*(?:left|right)|row-reverse|\bto (?:left|right)\b/;
// A four-value margin or padding shorthand whose two inline sides differ.
const ASYMMETRIC_SHORTHAND = /(?:^|[;{\s])(?:margin|padding):\s*([^\s;}]+)\s+([^\s;}]+)\s+([^\s;}]+)\s+([^\s;}]+)/g;

// Reviewed places where a machine value set in `font-mono` carries no `dir` of its own.
const UNISOLATED_MONO = [
  ["templates/repo/commit.html", '<div class="ms-auto flex items-center gap-3 font-mono',
    "the commit's parent/sha line mixes localized labels with ids; each id carries its own dir"],
  ["templates/repo/file_diffs.html", '<p class="diff-placeholder font-mono text-xs">',
    "a localized sentence; the two submodule ids are one <bdi dir=\"ltr\">"],
  ["templates/pulls/files.html", '<p class="diff-placeholder font-mono text-xs">',
    "a localized sentence; the two submodule ids are one <bdi dir=\"ltr\">"],
  ["templates/base.html", '<span class="hidden font-mono text-[11px] sm:inline"',
    "the render time is a localized phrase around a number"],
  ["templates/render_probe/shell_close.html", '<span class="hidden font-mono text-[11px] sm:inline"',
    "the render time is a localized phrase around a number"],
  ["templates/actions/run/header.html", '<dd class="mt-1 flex min-w-0 flex-wrap',
    "the commit cell follows the cell's direction; its sha is a <bdi dir=\"ltr\">"],
  ["templates/actions/job/header.html", '<dd class="mt-1 break-words font-mono text-ink-muted"><bdi>',
    "the runner labels are isolated by the inner <bdi>"],
  ["templates/admin/dashboard.html", '<dd class="min-w-0 truncate text-end {% if row.mono %}font-mono',
    "an end-aligned value; when it is monospace an inner <bdi dir=\"ltr\"> isolates it"],
];

function classValues(source) {
  const attributes = [...source.matchAll(/\bclass="((?:[^"{]|\{[%{#][\s\S]*?[%}#]\})*)"/g)].map((match) => match[1]);
  // Classes passed to the icon and avatar macros.
  const macroArguments = [...source.matchAll(/\bcall ui::(?:icon|avatar)\([^)]*?"([^"]*)"\)/g)].map((match) => match[1]);
  return [...attributes, ...macroArguments];
}

const OPEN_TAG = /<([a-z][a-z0-9]*)\b((?:[^<>"{]|"[^"]*"|\{[%{#][\s\S]*?[%}#]\})*)>/g;

test("every template uses only logical sides", async () => {
  const templates = await files("templates/", ".html");
  assert.ok(templates.length > 150, "found the templates");
  for (const path of templates) {
    const source = await read(path);
    for (const value of classValues(source)) {
      assert.doesNotMatch(` ${value} `, PHYSICAL_UTILITY, `${path}: class="${value}"`);
    }
    assert.doesNotMatch(source, /row-reverse/, `${path} reverses a row instead of following dir`);
    // Mirroring comes from logical properties and the `dir-icon` tokens; a per-direction
    // variant would mirror a second time.
    assert.doesNotMatch(source, /[\s"'](?:rtl|ltr):/, `${path} styles one direction by hand`);
  }
});

test("every stylesheet uses only logical sides", async () => {
  const sheets = await files("styles/", ".css");
  assert.ok(sheets.includes("styles/app.css") && sheets.length >= 5, "found the stylesheets");
  for (const path of sheets) {
    const source = await css(path.slice("styles/".length));
    for (const [, selector, body] of source.matchAll(/(?:^|\n)([^{}\n@]*)\{([^{}]*)\}/g)) {
      assert.doesNotMatch(` ${body}`, PHYSICAL_UTILITY, `${path}: ${selector.trim()} applies a physical-side utility`);
      assert.doesNotMatch(body, PHYSICAL_PROPERTY, `${path}: ${selector.trim()} declares a physical side`);
      for (const [shorthand, , right, , left] of body.matchAll(ASYMMETRIC_SHORTHAND)) {
        assert.equal(left, right, `${path}: ${selector.trim()} sets different sides with "${shorthand.trim()}"`);
      }
    }
  }
  const progress = (await css("app.css")).match(/@keyframes page-progress\s*\{([\s\S]*?)\n\}/)[1];
  for (const step of progress.match(/translateX\([^;]*\)/g)) {
    assert.match(step, /var\(--inline-sign\)/, "the progress bar runs along the reading direction");
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
    const source = await read(path);
    assert.doesNotMatch(source, /\bdir="rtl"/, `${path} pins a direction`);
    // Only machine values (clone URLs, the mirror's source) are left-to-right islands.
    for (const [tag] of source.matchAll(/<[a-z]+\b[^>]*\bdir="ltr"[^>]*>(?:\s*<input\b[^>]*>)?/g)) {
      assert.match(tag, /\bfont-mono\b/, `${path} pins a direction on chrome: ${tag}`);
    }
  }
});

test("machine values set in monospace are left-to-right islands", async () => {
  const unused = new Set(UNISOLATED_MONO.map(([path, snippet]) => `${path}\n${snippet}`));
  for (const path of await files("templates/", ".html")) {
    const source = await read(path);
    let previous = "";
    for (const match of source.matchAll(OPEN_TAG)) {
      const [tag, name, attributes] = match;
      const before = previous;
      previous = tag;
      const value = attributes.match(/\bclass="((?:[^"{]|\{[%{][\s\S]*?[%}]\})*)"/)?.[1] ?? "";
      if (!/(?:^|\s)font-mono(?=\s|$)/.test(value.replace(/\{[%{][\s\S]*?[%}]\}/g, " "))) continue;
      if (/\bdir="(?:ltr|auto)"/.test(attributes) || name === "bdi") continue;
      // A value in a box wider than itself keeps the box in the page direction, so the value
      // sits next to its icon or label, and isolates its text in an inner LTR <bdi>.
      if (/^\s*<bdi dir="ltr">/.test(source.slice(match.index + tag.length))) continue;
      // A select or search field keeps the page direction so its padding stays on the side
      // of its chevron or magnifier; one value in it is a single isolated run anyway.
      if (name === "select" || /\btype="search"/.test(attributes)) continue;
      // A value joined to its button (copy, browse files): the whole group is the island,
      // so the squared corners meet whatever the page direction.
      if (/\bdir="ltr"/.test(before)) continue;
      const allowed = UNISOLATED_MONO.find(([file, snippet]) => file === path && source.slice(source.indexOf(tag)).startsWith(snippet));
      assert.ok(allowed, `${path}: ${tag.slice(0, 120)} needs dir="ltr" (or dir="auto" for prose)`);
      unused.delete(`${allowed[0]}\n${allowed[1]}`);
    }
  }
  assert.deepEqual([...unused], [], "every allow-list entry still matches");
  assert.ok(UNISOLATED_MONO.every(([, , reason]) => reason.length > 20), "each entry gives its reason");
});

test("code, diffs and their line numbers stay left-to-right as a whole", async () => {
  const islands = [
    ["templates/repo/blob.html", /<div class="overflow-x-auto" dir="ltr">\s*<div class="code-lines /],
    ["templates/repo/blame.html", /<div class="blame-scroll overflow-x-auto" dir="ltr">/],
    ["templates/repo/file_diffs.html", /<div class="diff-scroll" dir="ltr"><div class="diff-view /],
    ["templates/pulls/files.html", /<div class="diff-scroll" dir="ltr"><div class="diff-view /],
    ["templates/pulls/review_comment.html", /<div class="diff-scroll border-b border-line" dir="ltr"><div class="diff-view diff-excerpt">/],
  ];
  for (const [path, island] of islands) assert.match(await read(path), island, `${path} isolates its code`);
  for (const path of await files("templates/", ".html")) {
    const source = await read(path);
    for (const [tag] of source.matchAll(/<div class="(?:diff-scroll|blame-scroll)\b[^"]*"[^>]*>/g)) {
      assert.match(tag, /\bdir="ltr"/, `${path}: ${tag}`);
    }
  }
  // Review threads and the line comment form sit inside the diff's island but are prose
  // and controls in the page's language: they take the page's direction back.
  const files_ = await read("templates/pulls/files.html");
  const page = 'dir="{{ page.content_locale.direction().as_str() }}"';
  assert.equal(files_.split(page).length - 1, 2, "both review boxes follow the page direction");
  // The single machine-log boundary of the Actions step body is unchanged.
  const step = await read("templates/actions/step_body.html");
  assert.equal(step.split('<div class="code-lines {{ log.columns() }}" dir="ltr">').length - 1, 1);
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
  // Rotation applies after the mirror: a mirrored chevron turned by a fixed quarter would
  // point up in a right-to-left document.
  for (const path of await files("templates/", ".html")) {
    for (const [call] of (await read(path)).matchAll(/icon\("chevron-(?:left|right)", "[^"]*"\)/g)) {
      assert.doesNotMatch(call, /(?:^|[\s:"])-?rotate-(?:90|180|270)\b/, `${path}: ${call}`);
    }
  }
  assert.match(await css("app.css"), /\.diff-tree-chevron\s*\{\s*@apply rotate-inline-90\b/);
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
