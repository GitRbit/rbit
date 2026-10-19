import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/repo.js", import.meta.url), "utf8");

function browser(fetch, DOMParser) {
  const listeners = new Map();
  const elements = [];
  class Element {
    constructor(tag) {
      this.tag = tag;
      this.dataset = {};
      this.children = [];
      this.listeners = new Map();
      this.isConnected = true;
    }
    setAttribute(key, value) { (this.attributes ??= {})[key] = value; }
    removeAttribute(key) { delete this.attributes?.[key]; }
    querySelectorAll() { return []; }
    replaceChildren(...children) { this.children = children; }
    append(...children) { this.children.push(...children); }
    addEventListener(name, listener) { this.listeners.set(name, listener); }
    showModal() { this.open = true; }
    close() { this.open = false; this.listeners.get("close")?.(); }
    remove() { this.isConnected = false; }
    focus() {}
    closest(selector) { return selector === "[data-language-files]" && this === trigger ? this : null; }
  }
  const trigger = new Element("button");
  trigger.dataset.languageFilesUrl = "/owner/repo/language-files?revision=abc&language=Rust";
  const document = {
    body: new Element("body"),
    addEventListener(name, handler, capture) { listeners.set(name, { handler, capture }); },
    querySelector() { return elements.findLast(element => element.tag === "dialog" && element.isConnected) ?? null; },
    querySelectorAll() { return []; },
    createElement(tag) {
      const element = new Element(tag);
      elements.push(element);
      return element;
    },
  };
  vm.runInNewContext(source, { document, Element, AbortController, fetch, DOMParser, window: {}, addEventListener() {} });
  return { trigger, listeners, elements, document, Element };
}

test("language click opens a modal and cannot reach htmx navigation", () => {
  const requests = [];
  const page = browser(url => {
    requests.push(url);
    return new Promise(() => {});
  });
  let prevented = false;
  let stopped = false;
  const click = page.listeners.get("click");
  assert.equal(click.capture, true);
  click.handler({
    target: page.trigger,
    preventDefault() { prevented = true; },
    stopPropagation() { stopped = true; },
  });
  assert.equal(prevented, true);
  assert.equal(stopped, true, "htmx must never receive the language control click");
  assert.equal(page.document.querySelector().open, true, "dialog opens before the request finishes");
  assert.deepEqual(requests, [page.trigger.dataset.languageFilesUrl]);
});

test("file preview stays beside the tree and superseded requests cannot overwrite selection", async () => {
  const requests = [];
  let content;
  class DOMParser {
    parseFromString() { return { querySelector: () => content }; }
  }
  const page = browser((url, options) => new Promise(resolve => requests.push({ url, options, resolve })), DOMParser);
  const preview = new page.Element('section');
  const selected = [];
  const browserContent = {
    querySelector: () => preview,
    querySelectorAll: () => selected,
  };
  const makeLink = path => {
    const link = new page.Element('a');
    link.href = `/owner/repo/blob/abc/${path}`;
    link.title = path;
    link.closest = selector => selector === '[data-language-dialog] [data-outline-file]' ? link : selector === '[data-language-content]' ? browserContent : null;
    return link;
  };
  const first = makeLink('src/first.rs');
  const second = makeLink('src/second.rs');
  selected.push(first, second);
  let stopped = 0;
  const click = link => page.listeners.get('click').handler({ target: link, preventDefault() {}, stopPropagation() { stopped++; } });
  click(first);
  click(second);
  assert.equal(requests[0].options.signal.aborted, true);
  assert.equal(first.attributes['aria-current'], undefined);
  assert.equal(second.attributes['aria-current'], 'true');
  assert.equal(preview.attributes['aria-busy'], 'true');
  content = new page.Element('section');
  requests[1].resolve({ ok: true, text: async () => '<section>second file</section>' });
  await new Promise(resolve => setImmediate(resolve));
  const secondContent = content;
  assert.equal(preview.children[1], secondContent);
  content = new page.Element('section');
  requests[0].resolve({ ok: true, text: async () => '<section>stale first file</section>' });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(preview.children[1], secondContent, 'late responses cannot overwrite the current file');
  assert.equal(preview.attributes['aria-busy'], 'false');
  assert.equal(stopped, 2, 'preview clicks never reach boosted navigation');
});

test("failed file loading offers an in-dialog retry without a page link", async () => {
  let attempts = 0;
  const page = browser(async () => { attempts += 1; throw new Error("Unavailable"); });
  page.listeners.get("click").handler({ target: page.trigger, preventDefault() {}, stopPropagation() {} });
  await new Promise(resolve => setImmediate(resolve));
  const retry = page.elements.find(element => element.textContent === "Try again");
  assert.equal(retry.tag, "button");
  assert.equal(page.elements.some(element => element.tag === "a"), false);
  retry.listeners.get("click")();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(attempts, 2);
  assert.equal(page.document.querySelector().open, true);
});

test("language links restored from old page history also stay in a dialog", () => {
  let requested;
  const page = browser(url => { requested = url; return new Promise(() => {}); });
  page.trigger.href = page.trigger.dataset.languageFilesUrl;
  delete page.trigger.dataset.languageFilesUrl;
  let stopped = false;
  page.listeners.get("click").handler({
    target: page.trigger,
    preventDefault() {},
    stopPropagation() { stopped = true; },
  });
  assert.equal(stopped, true);
  assert.equal(requested, page.trigger.href);
  assert.equal(page.document.querySelector().open, true);
});

test("listing pages inside the dialog load in the dialog, not the page", () => {
  const requests = [];
  const page = browser(url => { requests.push(url); return new Promise(() => {}); });
  page.listeners.get("click").handler({ target: page.trigger, preventDefault() {}, stopPropagation() {} });
  const dialog = page.document.querySelector();
  const next = new page.Element("a");
  next.href = "/owner/repo/language-files?q=&page=2&revision=abc&language=Rust";
  next.closest = selector => {
    if (selector === "[data-language-dialog] [data-listing-page]") return next;
    if (selector === "[data-language-dialog]") return dialog;
    return null;
  };
  let prevented = false;
  let stopped = false;
  page.listeners.get("click").handler({
    target: next,
    preventDefault() { prevented = true; },
    stopPropagation() { stopped = true; },
  });
  assert.equal(prevented && stopped, true, "htmx must not navigate the page");
  assert.deepEqual(requests, [page.trigger.dataset.languageFilesUrl, next.href]);
  assert.equal(page.document.querySelector().open, true);
});
