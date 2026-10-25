import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(process.env.RBIT_DIFF_ASSET ?? new URL("../../crates/rbit-web/assets/diff.js", import.meta.url), "utf8");

// A small element tree with the selectors diff.js's line comments use: classes, `[attr]`
// (with an optional value) and comma lists. It counts the elements single-element
// searches visit, so the tests can show that finding a long diff's first line does not
// walk the rest of it.
function dom() {
  // Elements visited by single-element searches, by selector.
  const stats = { visited: new Map() };
  let active = null;
  class Element {
    constructor(classes = "", { id = "", dataset = {} } = {}) {
      this.classes = new Set(classes.split(" ").filter(Boolean));
      this.id = id;
      this.dataset = { ...dataset };
      this.attributes = new Map();
      this.children = [];
      this.parentElement = null;
      this.classList = { contains: (name) => this.classes.has(name) };
    }
    append(...children) {
      for (const child of children) {
        child.parentElement = this;
        this.children.push(child);
      }
      return this;
    }
    get firstElementChild() { return this.children[0] ?? null; }
    get lastElementChild() { return this.children.at(-1) ?? null; }
    sibling(offset) {
      const siblings = this.parentElement?.children ?? [];
      return siblings[siblings.indexOf(this) + offset] ?? null;
    }
    get nextElementSibling() { return this.sibling(1); }
    get previousElementSibling() { return this.sibling(-1); }
    setAttribute(name, value) { this.attributes.set(name, String(value)); }
    removeAttribute(name) { this.attributes.delete(name); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    hasAttribute(name) { return this.matches(`[${name}]`); }
    contains(other) { for (let node = other; node; node = node.parentElement) if (node === this) return true; return false; }
    focus() { active = this; }
    matches(selector) {
      return selector.split(",").some((part) =>
        (part.trim().match(/\.[\w-]+|\[[^\]]+\]/g) ?? []).every((token) => {
          if (token.startsWith(".")) return this.classes.has(token.slice(1));
          const [name, quoted] = token.slice(1, -1).split("=");
          const value = quoted?.slice(1, -1);
          let actual;
          if (name === "id") actual = this.id || undefined;
          else if (name.startsWith("data-")) actual = this.dataset[name.slice(5).replace(/-(\w)/g, (_, c) => c.toUpperCase())];
          else actual = this.attributes.get(name);
          return actual !== undefined && (value === undefined || actual === value);
        }),
      );
    }
    closest(selector) {
      for (let node = this; node; node = node.parentElement) if (node.matches(selector)) return node;
      return null;
    }
    *walk() {
      for (const child of this.children) {
        yield child;
        yield* child.walk();
      }
    }
    querySelectorAll(selector) { return [...this.walk()].filter((node) => node.matches(selector)); }
    querySelector(selector) {
      for (const node of this.walk()) {
        stats.visited.set(selector, (stats.visited.get(selector) ?? 0) + 1);
        if (node.matches(selector)) return node;
      }
      return null;
    }
  }
  const row = (kind, id, old, now, text) =>
    new Element(`diff-${kind}`, { id }).append(new Element("num"), new Element("num"), Object.assign(new Element(), { text, old, now }));
  const body = new Element();
  const events = new Map();
  const document = {
    querySelectorAll: (selector) => body.querySelectorAll(selector),
    querySelector: (selector) => body.querySelector(selector),
    getElementById: () => null,
    addEventListener: (name, fn) => events.set(name, [...(events.get(name) ?? []), fn]),
  };
  const requests = [];
  const assigned = [];
  const context = {
    document, Element, HTMLDetailsElement: class {}, HTMLInputElement: class {},
    matchMedia: () => ({ matches: true, addEventListener() {} }),
    requestAnimationFrame: () => 1, cancelAnimationFrame() {}, addEventListener() {},
    localStorage: { getItem() { return null; } }, getComputedStyle: () => ({ scrollMarginTop: "0px" }),
    location: { hash: "", assign: (url) => assigned.push(url) }, scrollBy() {},
    htmx: { ajax: (verb, url, options) => requests.push({ verb, url, ...options }) },
  };
  const fire = (name, event) => {
    let prevented = false;
    events.get(name)?.forEach((fn) => fn({ ...event, preventDefault() { prevented = true; } }));
    return prevented;
  };
  return { Element, row, body, context, fire, requests, assigned, stats, active: () => active, run: () => vm.runInNewContext(source, context) };
}

/** A pull request file: a hunk and two lines in a block, a thread, then a block of one
    more line and an end-of-file note. */
function page({ comment = true } = {}) {
  const page = dom();
  const { Element, row, body } = page;
  const view = new Element("diff-view");
  const first = new Element("vblock").append(
    row("hunk", "", "", "", "@@ -1,2 +1,3 @@"),
    row("del", "diff-ab-L1", "1", "", "a"),
    row("add", "diff-ab-R1", "", "1", "b"),
  );
  const thread = new Element("diff-comment");
  const second = new Element("vblock").append(row("ctx", "diff-ab-R2", "2", "2", "c"), new Element("diff-eof"));
  view.append(first, thread, second);
  const file = new Element("diff-file", { id: "diff-ab", dataset: comment ? { comment: "/o/r/pull/3/files" } : {} }).append(view);
  body.append(new Element("", { dataset: { diffLayout: "" } }).append(file));
  page.run();
  return { ...page, view, rows: [first.children[1], first.children[2], second.children[0]], hunk: first.children[0] };
}

const cell = (row) => row.firstElementChild;
const focusable = (view) => view.querySelectorAll("[tabindex]");

test("each commentable diff gets one tab stop, on its first line, labelled with its line", () => {
  const { view, rows } = page();
  assert.deepEqual(focusable(view), [cell(rows[0])]);
  assert.equal(cell(rows[0]).getAttribute("role"), "button");
  assert.equal(cell(rows[0]).getAttribute("aria-label"), "Comment on line 1");
  assert.equal(page({ comment: false }).view.querySelectorAll("[tabindex]").length, 0, "read-only diffs stay inert");
});

test("finding the first line does not walk the rest of a long diff", () => {
  const long = dom();
  const view = new long.Element("diff-view");
  for (let block = 0; block < 100; block++) {
    const rows = Array.from({ length: 128 }, (_, i) => long.row("add", `diff-ab-R${block * 128 + i + 1}`, "", "", "x"));
    view.append(new long.Element("vblock").append(...rows));
  }
  const file = new long.Element("diff-file", { id: "diff-ab", dataset: { comment: "/o/r/pull/3/files" } }).append(view);
  long.body.append(new long.Element("", { dataset: { diffLayout: "" } }).append(file));
  long.run();
  const visited = long.stats.visited.get(".diff-add[id], .diff-del[id], .diff-ctx[id]");
  assert.ok(visited < 20, `visited ${visited} of 12,900 elements`);
  assert.equal(view.querySelectorAll("[tabindex]").length, 1);
});

test("clicking a number opens the form for that line in place, once", () => {
  const { fire, rows, requests, hunk } = page();
  assert.equal(fire("click", { target: rows[1].children[1] }), true);
  assert.deepEqual(requests, [{
    verb: "GET", url: "/o/r/pull/3/files?comment=diff-ab-R1#diff-ab-R1", source: rows[1],
    target: "#diff-ab", select: "#diff-ab", swap: "outerHTML",
  }]);
  // Code text, hunk headers and read-only diffs do nothing.
  assert.equal(fire("click", { target: rows[1].children[2] }), false);
  assert.equal(fire("click", { target: hunk.children[0] }), false);
  const readOnly = page({ comment: false });
  assert.equal(readOnly.fire("click", { target: cell(readOnly.rows[0]) }), false);
  assert.equal(readOnly.requests.length, 0);
});

test("without htmx the number navigates to the form", () => {
  const { fire, rows, assigned, context } = page();
  delete context.htmx;
  fire("click", { target: cell(rows[2]) });
  assert.deepEqual(assigned, ["/o/r/pull/3/files?comment=diff-ab-R2#diff-ab-R2"]);
});

test("arrow keys move the one tab stop across blocks, threads and hunks; Enter opens the form", () => {
  const { fire, view, rows, requests, active } = page();
  const key = (target, key) => fire("keydown", { target, key });
  assert.equal(key(cell(rows[0]), "ArrowDown"), true);
  assert.equal(active(), cell(rows[1]));
  assert.equal(key(cell(rows[1]), "ArrowDown"), true, "over the thread, into the next block");
  assert.equal(active(), cell(rows[2]));
  assert.deepEqual(focusable(view), [cell(rows[2])], "the previous tab stop is released");
  assert.equal(cell(rows[2]).getAttribute("aria-label"), "Comment on line 2");
  assert.equal(key(cell(rows[2]), "ArrowDown"), false, "the last line stays put");
  key(cell(rows[2]), "ArrowUp");
  key(cell(rows[1]), "ArrowUp");
  assert.equal(active(), cell(rows[0]));
  assert.equal(key(cell(rows[0]), "ArrowUp"), false, "the hunk header is not a stop");
  assert.equal(key(cell(rows[0]), "Enter"), true);
  assert.equal(key(cell(rows[0]), " "), true);
  assert.equal(requests.length, 2);
  assert.equal(fire("keydown", { target: cell(rows[0]), key: "ArrowDown", altKey: true }), false);
});
