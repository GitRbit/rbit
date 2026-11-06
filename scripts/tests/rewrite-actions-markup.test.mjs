import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { context, render } from "../../docs/design/prototypes/actions/render.mjs";
import { cases, markupDigest } from "./fixtures/actions-markup.mjs";

test("readable prototype helpers preserve reviewed tags, attributes and text across fixture states", async () => {
  // Recorded from the original source before refactoring. Formatting whitespace
  // may change outside code; markup, attributes, escaped text and code stay exact.
  // The only deliberate correction is the earlier-window ol start to 49841.
  const expected = JSON.parse(await readFile(new URL("./fixtures/actions-markup.json", import.meta.url)));
  assert.equal(expected.length, cases.length);
  for (const [index, { screen, ...query }] of cases.entries()) {
    const html = render(context(`/${screen}`, new URLSearchParams(query)));
    assert.equal(markupDigest(html), expected[index], `${screen}: ${JSON.stringify(query)}`);
  }
});

test("expanded semantic styles preserve original cascade and declaration tokens", async () => {
  const root = new URL("../../docs/design/prototypes/actions/", import.meta.url);
  const names = ["base", "overview", "run", "job", "responsive"];
  const sources = await Promise.all(names.map((name) => readFile(new URL(`styles/${name}.css`, root), "utf8")));
  const canonical = sources.join("").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, "");
  const expected = (await readFile(new URL("./fixtures/actions-css.sha256", import.meta.url), "utf8")).trim();
  assert.equal(createHash("sha256").update(canonical).digest("hex"), expected);
  const imports = await readFile(new URL("prototype.css", root), "utf8");
  assert.deepEqual([...imports.matchAll(/\/styles\/([a-z]+)\.css/g)].map((match) => match[1]), names);
});

test("ordered log-list numbering matches each window's first anchored line", () => {
  for (const [earlier, first] of [["", 49921], ["1", 49841]]) {
    const html = render(context("/job", new URLSearchParams({ earlier })));
    assert.match(html, new RegExp(`class="log-lines" start="${first}"`));
    assert.match(html, new RegExp(`<li id="L${first}"`));
  }
});
