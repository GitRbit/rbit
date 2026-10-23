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
