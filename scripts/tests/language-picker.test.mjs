// The header's language button opens a dialog that saves through the language page's own form
// and reloads once; without scripts the button is a link to that page.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { DIALOG_URL, installLanguagePicker, saveLanguage } from "../../crates/rbit-web/assets/language-picker.js";

const read = (path) => readFileSync(new URL(`../../crates/rbit-web/${path}`, import.meta.url), "utf8").replace(/\r\n/g, "\n");

class FakeElement {
  constructor(attributes = {}, children = []) {
    this.attributes = { ...attributes };
    this.children = children;
    this.listeners = new Map();
    this.isConnected = true;
    this.disabled = false;
    for (const child of children) child.parent = this;
  }
  get value() { return this.attributes.value; }
  get name() { return this.attributes.name; }
  getAttribute(name) { return this.attributes[name] ?? null; }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  removeAttribute(name) { delete this.attributes[name]; }
  matches(selector) {
    const attribute = selector.match(/^\[([a-z-]+)(?:="([^"]*)")?\]$/);
    if (attribute) return attribute[2] === undefined ? attribute[1] in this.attributes : this.attributes[attribute[1]] === attribute[2];
    if (selector === "details[data-menu]") return this.attributes.tag === "details" && "data-menu" in this.attributes;
    if (selector === "summary") return this.attributes.tag === "summary";
    return false;
  }
  *descendants() {
    for (const child of this.children) {
      yield child;
      yield* child.descendants();
    }
  }
  querySelectorAll(selector) {
    const [first, second] = selector.split(/(?<=\])(?=\[)/);
    return [...this.descendants()].filter((element) => element.matches(first) && (!second || element.matches(second)));
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0] ?? null; }
  closest(selector) {
    for (let element = this; element; element = element.parent) if (element.matches?.(selector)) return element;
    return null;
  }
  addEventListener(name, listener) { this.listeners.set(name, listener); }
  focus() { focused.element = this; }
}
const focused = { element: null };

function dialogFixture({ current = "en" } = {}) {
  const rows = ["en", "de", "ja"].map((tag) => new FakeElement({ name: "locale", value: tag, "data-lpk-option": "", ...(tag === current ? { "aria-current": "true" } : {}) }));
  const form = new FakeElement({ action: "/settings/language", "data-language-form": "" }, rows);
  const close = new FakeElement({ "data-lpk-close": "" });
  const dialog = new FakeElement({ "data-language-dialog": "" }, [close, form]);
  dialog.isConnected = false;
  dialog.showModal = () => { dialog.open = true; };
  dialog.close = () => { dialog.open = false; dialog.listeners.get("close")?.(); };
  return { dialog, form, rows, close };
}

function page({ fetchResult = async () => ({ type: "opaqueredirect", ok: false, status: 0 }), dialogHtml = true, current } = {}) {
  const parts = dialogFixture({ current });
  const listeners = new Map();
  const requests = [];
  const navigation = { reloads: 0, assigned: [] };
  const summary = new FakeElement({ tag: "summary" });
  const menuLink = new FakeElement({ href: "/settings/language", "data-language-dialog-open": "" });
  const menu = new FakeElement({ tag: "details", "data-menu": "", open: "" }, [summary, menuLink]);
  const link = new FakeElement({ href: "/settings/language", "data-language-dialog-open": "" });
  const document = {
    body: { append: (element) => { element.isConnected = true; } },
    activeElement: null,
    addEventListener: (name, listener) => listeners.set(name, listener),
    createElement: () => ({ set innerHTML(_) {}, content: { querySelector: () => (dialogHtml ? parts.dialog : null) } }),
  };
  const window = {
    fetch: async (url, options = {}) => {
      requests.push({ url, ...options });
      if (url === DIALOG_URL) return { ok: true, text: async () => "<dialog data-language-dialog></dialog>" };
      return fetchResult();
    },
    location: { reload: () => navigation.reloads++, assign: (url) => navigation.assigned.push(url) },
  };
  installLanguagePicker(document, window);
  const click = async (target) => {
    let prevented = false;
    listeners.get("click")({ target, button: 0, preventDefault: () => { prevented = true; } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    return prevented;
  };
  const submit = async (button) => {
    let prevented = false;
    parts.form.listeners.get("submit")({ submitter: button, preventDefault: () => { prevented = true; } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    return prevented;
  };
  return { ...parts, link, menu, menuLink, summary, document, requests, navigation, click, submit };
}

test("the save posts only the locale, keeps the redirect unfollowed and reports refusals", async () => {
  const calls = [];
  const form = { getAttribute: (name) => (name === "action" ? "/settings/language" : null) };
  const saved = await saveLanguage(form, "pt-BR", async (url, options) => { calls.push({ url, options }); return { type: "opaqueredirect", ok: false }; });
  assert.equal(saved, true);
  assert.equal(calls[0].url, "/settings/language");
  assert.equal(calls[0].options.method, "POST");
  assert.equal(calls[0].options.redirect, "manual", "the language page itself is not fetched");
  assert.equal(calls[0].options.body.toString(), "locale=pt-BR");
  assert.equal(await saveLanguage(form, "xx", async () => ({ type: "basic", ok: false, status: 422 })), false);
});

test("the button opens the fetched dialog on the current language and saving reloads once", async () => {
  const fixture = page({ current: "de" });
  assert.equal(await fixture.click(fixture.link), true, "the link's navigation is replaced by the dialog");
  assert.equal(fixture.requests[0].url, DIALOG_URL);
  assert.equal(fixture.dialog.open, true);
  assert.equal(focused.element, fixture.rows[1], "focus starts on the current language");

  assert.equal(await fixture.submit(fixture.rows[2]), true);
  const post = fixture.requests.at(-1);
  assert.equal(post.method, "POST");
  assert.equal(post.body.toString(), "locale=ja");
  assert.equal(fixture.navigation.reloads, 1, "one reload renders the new language");
  assert.ok(fixture.rows.every((row) => row.disabled), "rows wait while the choice is saved");
  await fixture.submit(fixture.rows[0]);
  assert.equal(fixture.navigation.reloads, 1, "a second press while saving does nothing");
});

test("choosing the current language only closes, and focus returns to the button", async () => {
  const fixture = page({ current: "en" });
  await fixture.click(fixture.link);
  await fixture.submit(fixture.rows[0]);
  assert.equal(fixture.dialog.open, false);
  assert.equal(fixture.requests.length, 1, "nothing is saved");
  assert.equal(fixture.navigation.reloads, 0);
  assert.equal(focused.element, fixture.link);
});

test("the backdrop and close button close it; a phone menu closes and takes focus back", async () => {
  const fixture = page();
  await fixture.click(fixture.menuLink);
  assert.equal(fixture.menu.getAttribute("open"), null, "the menu behind the dialog closes");
  fixture.dialog.listeners.get("click")({ target: fixture.dialog });
  assert.equal(fixture.dialog.open, false, "a click on the backdrop closes it");
  assert.equal(focused.element, fixture.summary);
  await fixture.click(fixture.link);
  fixture.dialog.listeners.get("click")({ target: fixture.close });
  assert.equal(fixture.dialog.open, false);
});

test("arrow keys move between languages and wrap around", async () => {
  const fixture = page({ current: "ja" });
  await fixture.click(fixture.link);
  const key = (name) => {
    fixture.document.activeElement = focused.element;
    fixture.dialog.listeners.get("keydown")({ key: name, preventDefault() {} });
  };
  key("ArrowDown");
  assert.equal(focused.element, fixture.rows[0]);
  key("ArrowUp");
  assert.equal(focused.element, fixture.rows[2]);
  key("Home");
  assert.equal(focused.element, fixture.rows[0]);
});

test("a refused save or a missing dialog falls back to the language page", async () => {
  const refused = page({ fetchResult: async () => ({ type: "basic", ok: false, status: 422 }) });
  await refused.click(refused.link);
  await refused.submit(refused.rows[1]);
  assert.equal(refused.navigation.reloads, 0);
  assert.deepEqual(refused.navigation.assigned, ["/settings/language"]);

  const missing = page({ dialogHtml: false });
  await missing.click(missing.link);
  assert.deepEqual(missing.navigation.assigned, ["/settings/language"], "the link's own destination still works");
});

test("the header links the button to the language page before the theme buttons, in both shells", () => {
  for (const path of ["templates/base.html", "templates/render_probe/shell_open.html"]) {
    const shell = read(path);
    const desktop = shell.indexOf('<a href="/settings/language" class="btn btn-ghost btn-icon hidden sm:inline-flex" hx-boost="false" aria-haspopup="dialog"');
    assert.ok(desktop > 0, `${path} has the header button`);
    assert.ok(desktop < shell.indexOf('<form method="post" action="/themes" class="hidden sm:contents"'), "it precedes the theme switch");
    const macro = shell.slice(0, shell.indexOf("{% endmacro %}"));
    assert.ok(macro.indexOf("data-language-dialog-open") < macro.indexOf("data-theme-toggle"), "the phone menu row comes first too");
    assert.match(shell.slice(desktop, shell.indexOf("</a>", desktop)), /ui::icon\("languages", "size-4"\)/);
  }
  const app = read("assets/app.js");
  assert.match(app, /import "\.\/language-picker\.js";/);
  const dialog = read("templates/settings/language_dialog.html");
  assert.match(dialog, /<form class="lpk-list" id="lpk-form" method="post" action="\/settings\/language" hx-boost="false"/);
  assert.doesNotMatch(dialog, /\bstyle=|\bon[a-z]+=|<script/, "strict CSP: no inline styles, handlers or scripts");
});
