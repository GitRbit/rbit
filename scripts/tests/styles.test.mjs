import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";

const styles = new URL("../../crates/rbit-web/styles/", import.meta.url);
const sources = await Promise.all((await readdir(styles)).filter(name => name.endsWith(".css")).map(async name => [
  name, (await readFile(new URL(name, styles), "utf8")).replace(/\/\*[\s\S]*?\*\//g, ""),
]));

test("no universal :hover rule makes every element restyle as the pointer moves", () => {
  for (const [name, css] of sources) {
    assert.doesNotMatch(css, /(?:^|[\s,{}>+~(])\*?:hover\b/m, `${name} matches :hover on any element`);
  }
});

test("scrollbars still darken while the pointer is over the page", () => {
  const app = sources.find(([name]) => name === "app.css")[1];
  assert.match(app, /html\s*\{\s*scrollbar-color:\s*var\(--line-strong\) transparent;\s*\}/);
  assert.match(app, /html:hover\s*\{\s*scrollbar-color:\s*var\(--ink-faint\) transparent;\s*\}/);
});

test("the file tree's current-file marker moves without a transition", () => {
  // diff.js moves `aria-current` while the page scrolls; a colour transition on the rows
  // would restyle and repaint the sticky tree on every frame of each fade.
  const app = sources.find(([name]) => name === "app.css")[1];
  const rules = [...app.matchAll(/(?:^|\n)([^{}\n]*\.diff-tree-item[^{}]*)\{([^}]*)\}/g)];
  assert.ok(rules.length > 0);
  for (const [, selector, body] of rules) {
    if (selector.includes(":hover")) continue;
    const unconditional = body.replace(/\bhover:[\w-]+/g, "");
    assert.doesNotMatch(unconditional, /\btransition(?!-none\b)/, `${selector.trim()} declares a transition`);
  }
  const base = rules.find(([, selector]) => selector.trim() === ".diff-tree-item")[2];
  // File rows are links: without this they keep the base layer's fade on every `a`.
  assert.match(app, /(?:^|\n)\s*a\s*\{\s*@apply transition-colors/, "the base layer fades links");
  assert.match(base.replace(/\bhover:[\w-]+/g, ""), /\btransition-none\b/, "rows override the links' fade");
  assert.match(base, /\bhover:transition-colors\b/, "hovering still fades in");
});
