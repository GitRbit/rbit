import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(process.env.RBIT_DIFF_ASSET ?? new URL("../../crates/rbit-web/assets/diff.js", import.meta.url), "utf8");

// A deterministic DOM harness counts work, not elapsed time. Geometry is supplied so
// the tests can exercise scrolled files, htmx replacements and restored snapshots.
function browser(count = 150, { restored = false, mobile = false } = {}) {
  const events = new Map();
  const frames = new Map();
  const observed = new Set();
  const stats = { queries: 0, geometry: 0, classes: 0 };
  let nextFrame = 0;
  class Element {
    constructor(kind) {
      this.kind = kind;
      this.dataset = {};
      this.attributes = new Map();
      this.isConnected = true;
      this.top = 0;
      this.offsetHeight = 64;
      this.classList = { toggle() { stats.classes++; } };
    }
    setAttribute(key, value) { this.attributes.set(key, value); }
    removeAttribute(key) { this.attributes.delete(key); }
    getAttribute(key) { return this.attributes.get(key) ?? null; }
    hasAttribute(key) { return key === "data-ready" ? "ready" in this.dataset : this.attributes.has(key); }
    matches(selector) { return selector === ".diff-file" && this.kind === "file"; }
    closest(selector) { return selector === "[data-diff-layout]" ? this.layout ?? (this.kind === "layout" ? this : null) : null; }
    contains(other) { return this === other || other.layout === this; }
    getBoundingClientRect() { stats.geometry++; return { top: this.top, bottom: this.top + 20 }; }
    querySelectorAll(selector) {
      stats.queries++;
      if (selector === ".diff-file") return this.files ?? [];
      if (selector === "[data-diff-target]") return this.links ?? [];
      if (selector === "[data-when]") return this.parts ?? [];
      return [];
    }
    querySelector(selector) {
      stats.queries++;
      if (selector === "[data-diff-tree]") return this.tree;
      if (selector === "[data-diff-collapse-all]") return this.button;
      if (selector === '[aria-current="true"]') return this.links?.find(link => link.hasAttribute("aria-current")) ?? null;
      if (selector.startsWith('[data-diff-target="')) return this.links?.find(link => selector.includes(`"${link.dataset.diffTarget}"`)) ?? null;
      return null;
    }
  }
  class Details extends Element {
    constructor(id) { super("file"); this.id = id; this.open = true; }
  }
  const layout = new Element("layout");
  if (restored) layout.dataset.ready = "";
  layout.tree = new Element("tree");
  layout.tree.scrollTop = 0;
  layout.tree.links = [];
  layout.button = new Element("button");
  layout.button.parts = ["expanded", "collapsed"].map(when => {
    const part = new Element("part"); part.dataset.when = when; return part;
  });
  layout.files = Array.from({ length: count }, (_, i) => {
    const file = new Details(`diff-${i}`);
    file.layout = layout;
    file.top = i * 100;
    const link = new Element("link"); link.dataset.diffTarget = file.id;
    layout.tree.links.push(link);
    return file;
  });
  const page = { layouts: count ? [layout] : [], header: new Element("header") };
  const listen = (name, fn) => events.set(name, [...(events.get(name) ?? []), fn]);
  const document = {
    documentElement: { style: { setProperty(key, value) { page[key] = value; } } },
    querySelector(selector) { stats.queries++; return selector === "body > header" ? page.header : null; },
    querySelectorAll(selector) {
      stats.queries++;
      return selector.includes("[data-diff-layout]") ? page.layouts.filter(l => !selector.includes(":not") || !("ready" in l.dataset)) : [];
    },
    getElementById() { return null; },
    addEventListener: listen,
  };
  vm.runInNewContext(source, {
    document, Element, HTMLDetailsElement: Details, HTMLInputElement: class {},
    ResizeObserver: class { observe(element) { observed.add(element); } disconnect() { observed.clear(); } },
    matchMedia: () => ({ matches: !mobile, addEventListener() {} }),
    getComputedStyle: () => ({ getPropertyValue: () => "0" }),
    requestAnimationFrame: fn => { frames.set(++nextFrame, fn); return nextFrame; },
    cancelAnimationFrame: id => frames.delete(id),
    addEventListener: listen, localStorage: { getItem() { return null; } },
    location: { hash: "" }, CSS: { escape: value => value }, scrollBy() {},
  });
  const fire = (name, detail = {}, target = document) => events.get(name)?.forEach(fn => fn({ detail, target }));
  const flush = () => { const jobs = [...frames.values()]; frames.clear(); jobs.forEach(fn => fn()); };
  flush();
  Object.keys(stats).forEach(key => stats[key] = 0);
  return { page, layout, frames, observed, stats, fire, flush, Element, Details };
}

test("ordinary pages schedule no diff work on scroll", () => {
  const page = browser(0);
  for (let i = 0; i < 60; i++) { page.fire("scroll"); page.flush(); }
  assert.equal(page.stats.queries, 0);
  assert.equal(page.frames.size, 0);
});

test("diff scrolling uses its index and still tracks the current file", () => {
  const page = browser();
  page.layout.files.forEach(file => file.top -= 500);
  for (let i = 0; i < 60; i++) { page.fire("scroll"); page.flush(); }
  assert.equal(page.stats.queries, 0, "no selector traversal during scrolling");
  assert.equal(page.layout.tree.links[5].getAttribute("aria-current"), "true");
  assert.equal(page.layout.tree.links[0].hasAttribute("aria-current"), false);
});

test("bulk native toggles update the toolbar once per animation frame", () => {
  const page = browser();
  for (const file of page.layout.files) {
    file.open = false;
    page.fire("toggle", {}, file);
  }
  assert.equal(page.frames.size, 1);
  page.flush();
  assert.equal(page.layout.button.dataset.state, "collapsed");
  assert.equal(page.stats.classes, 4, "two classes on each of the two labels");
  assert.equal(page.stats.queries, 0);
});

test("fragment swaps replace cached file and sidebar references", () => {
  const page = browser(2);
  const replacement = new page.Details("diff-new");
  replacement.layout = page.layout;
  const link = new page.Element("link"); link.dataset.diffTarget = replacement.id;
  page.layout.files = [replacement];
  page.layout.tree.links = [link];
  page.fire("htmx:afterSwap");
  page.fire("htmx:afterSettle");
  page.flush();
  assert.equal(link.getAttribute("aria-current"), "true");
  replacement.open = false;
  page.fire("toggle", {}, replacement);
  page.flush();
  assert.equal(page.layout.button.dataset.state, "collapsed");
});

test("restored data-ready markup receives a fresh index", () => {
  const page = browser(2, { restored: true });
  page.fire("htmx:historyRestore");
  page.flush();
  assert.equal(page.layout.tree.links[0].getAttribute("aria-current"), "true");
});

test("hidden desktop and closed mobile trees do not measure file geometry", () => {
  for (const mobile of [false, true]) {
    const page = browser(2, { mobile });
    if (!mobile) page.layout.dataset.sidebar = "hidden";
    page.fire("scroll"); page.flush();
    assert.equal(page.stats.geometry, 0);
    if (mobile) page.layout.dataset.sidebarMobile = "open";
    else delete page.layout.dataset.sidebar;
    page.fire("resize"); page.flush();
    assert.ok(page.stats.geometry > 0);
  }
});

test("navigation releases pending work and follows the replacement header", () => {
  const page = browser(2);
  page.fire("scroll");
  page.fire("htmx:beforeCleanupElement", { elt: page.layout });
  assert.equal(page.frames.size, 0);
  page.fire("htmx:beforeCleanupElement", { elt: page.page.header });
  assert.equal(page.observed.size, 0);
  page.page.layouts = [];
  page.page.header = new page.Element("header");
  page.page.header.offsetHeight = 96;
  page.fire("htmx:afterSettle");
  assert.equal(page.page["--header-height"], "96px");
  assert.ok(page.observed.has(page.page.header));
  assert.equal(page.frames.size, 0);
});

test("pull-request file toggles outside a diff layout are ignored", () => {
  const page = browser(0);
  page.fire("toggle", {}, new page.Details("pull-file"));
  assert.equal(page.frames.size, 0);
  assert.equal(page.stats.queries, 0);
});
