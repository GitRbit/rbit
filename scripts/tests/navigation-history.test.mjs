// nav.js decides what htmx may keep as a history snapshot before htmx stores it.
import assert from "node:assert/strict";
import test from "node:test";
import vm from "node:vm";
import { navigationModules } from "./fixtures/navigation-modules.mjs";

const installNavigation = await navigationModules();

// A policy-free parsed document: tags with quoted attributes, everything else text.
function snapshotParser() {
  const parses = [];
  class DOMParser {
    parseFromString(text, type) {
      parses.push({ text, type });
      const elements = [];
      const nodes = text.replace(/^<body>/, "").split(/(<[a-z]+(?:\s+[a-z-]+="[^"]*")*\s*>)/).map(part => {
        const tag = /^<([a-z]+)((?:\s+[a-z-]+="[^"]*")*)\s*>$/.exec(part);
        if (!tag) return part;
        const attributes = new Map([...tag[2].matchAll(/([a-z-]+)="([^"]*)"/g)].map(([, name, value]) => [name, value]));
        elements.push({ attributes, removeAttribute(name) { attributes.delete(name); } });
        return () => `<${tag[1]}${[...attributes].map(([name, value]) => ` ${name}="${value}"`).join("")}>`;
      });
      return { body: {
        querySelectorAll(selector) {
          assert.equal(selector, "[style]");
          return elements.filter(element => element.attributes.has("style"));
        },
        get innerHTML() { return nodes.map(node => typeof node === "function" ? node() : node).join(""); },
      } };
    }
  }
  return { DOMParser, parses };
}

function page() {
  const listeners = new Map();
  const { DOMParser, parses } = snapshotParser();
  const document = {
    body: { dataset: {} },
    documentElement: { classList: { add() {}, remove() {} }, getAttribute: () => null },
    querySelector: () => null,
    querySelectorAll: () => [],
    getElementById: () => null,
    getElementsByTagName: () => [],
    addEventListener(name, callback) { listeners.set(name, [...listeners.get(name) ?? [], callback]); },
  };
  const context = vm.createContext({ URL, DOMParser, document, location: new URL("https://git.example.com/settings/language"),
    addEventListener() {}, setTimeout, clearTimeout });
  context.window = context;
  installNavigation(context);
  return {
    parses,
    created(item) {
      for (const callback of listeners.get("htmx:historyItemCreated") ?? []) callback({ detail: { item } });
      return item;
    },
  };
}

test("history snapshots never carry style attributes the page's style-src refuses on restore", () => {
  const app = page();
  // Screenshot caret hiding leaves `style=""` on inputs; scripts and extensions set real styles.
  const content = '<form action="/themes"><input type="hidden" name="next" value="/settings/language" style="">'
    + '</form><div class="row" style="--depth: 2">code: style="kept" &amp; text</div>';
  const item = app.created({ url: "/settings/language?saved=1", content });
  assert.deepEqual(app.parses, [{ text: `<body>${content}`, type: "text/html" }], "parsed once, in a document without the page's policy");
  assert.equal(item.content, '<form action="/themes"><input type="hidden" name="next" value="/settings/language">'
    + '</form><div class="row">code: style="kept" &amp; text</div>', "only the attributes go; text and other attributes stay");
  assert.equal(item.url, "/settings/language?saved=1");
});

test("history snapshots without style attributes keep their exact content and are never parsed", () => {
  const app = page();
  const content = '<main id="main"><a href="/alice/repo">repo</a></main>';
  assert.equal(app.created({ url: "/explore", content }).content, content);
  assert.deepEqual(app.parses, []);
});

test("history snapshots over 64k characters are dropped before htmx stores them", () => {
  const app = page();
  const kept = app.created({ url: "/alice/repo/issues", content: "x".repeat(64_000) });
  const dropped = app.created({ url: "/alice/repo/blob/main/big.rs", content: ` style="${"x".repeat(64_000)}"` });
  assert.equal(kept.url, "/alice/repo/issues");
  assert.equal(kept.content.length, 64_000);
  assert.equal(dropped.content, "");
  assert.equal(dropped.url, "\u0000too-large", "the placeholder can never match a real path");
  assert.deepEqual(app.parses, [], "an oversized snapshot is dropped without being parsed");
});
