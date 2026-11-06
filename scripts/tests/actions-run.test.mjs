import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/actions-run.js", import.meta.url), "utf8");
class Element {}
class Field extends Element {
  constructor(name, value = "", checkbox = false) {
    super(); this.name = name; this.tagName = checkbox ? "INPUT" : "TEXTAREA";
    this.type = checkbox ? "checkbox" : "textarea";
    this.value = value; this.defaultValue = ""; this.checked = false; this.defaultChecked = false;
    this.selectionStart = 2; this.selectionEnd = 4; this.selectionDirection = "forward";
  }
  focus(options) { this.document.activeElement = this; this.focusOptions = options; }
  matches(selector) { assert.equal(selector, ":disabled"); return this.disabled || this.fieldsetDisabled; }
  setSelectionRange(start, end, direction) { this.selection = [start, end, direction]; }
}
class Form extends Element {
  constructor(kind, fields, environment = "production", action = "/run/review") {
    super(); this.dataset = { runReview: kind }; this.method = "post"; this.action = action;
    this.fields = fields; this.environment = { value: environment };
  }
  querySelectorAll() { return this.fields; }
  querySelector() { return this.environment; }
}
class Run extends Element {
  constructor(forms = []) { super(); this.id = "run"; this.forms = forms; }
  querySelectorAll() { return this.forms; }
}
function browser(href = "https://forge.example/owner/repo/actions/runs/1") {
  const listeners = new Map();
  const document = { body: {}, activeElement: null, addEventListener: (type, listener) => listeners.set(type, listener) };
  document.activeElement = document.body;
  const location = new URL(href);
  vm.runInNewContext(source, { Element, document, location, URL });
  const fire = (name, detail) => listeners.get(name)({ detail });
  return {
    document, location,
    swap(old, next, { status = 200, verb = "get", source = old, path = location.href,
      responseURL = location.href, headers = {}, boosted = false, beforeAfter = () => {} } = {}) {
      const xhr = { status, responseURL, getResponseHeader: name => headers[name] ?? null };
      const requestConfig = { verb, elt: source, path, boosted };
      fire("htmx:beforeSwap", { target: old, xhr, requestConfig });
      document.activeElement = document.body; beforeAfter({ xhr, requestConfig });
      for (const form of next.forms) for (const field of form.fields) field.document = document;
      fire("htmx:afterSwap", { target: next, xhr, requestConfig });
    },
  };
}

test("GET poll restores edited review text, checkbox, focus and caret into newly authorized controls", () => {
  const page = browser(), comment = new Field("comment", "private review"), trust = new Field("trust_author", "true", true);
  trust.checked = true; page.document.activeElement = comment;
  const old = new Run([new Form("deployment", [comment]), new Form("approval", [trust], "", "/run/approve")]);
  const nextComment = new Field("comment"), nextTrust = new Field("trust_author", "true", true);
  const next = new Run([new Form("deployment", [nextComment]), new Form("approval", [nextTrust], "", "/run/approve")]);
  page.swap(old, next);
  assert.equal(nextComment.value, "private review"); assert.equal(nextTrust.checked, true);
  assert.equal(page.document.activeElement, nextComment);
  assert.deepEqual(nextComment.selection, [2, 4, "forward"]);
  assert.equal(nextComment.focusOptions.preventScroll, true);
});

test("new permission, environment, action and disabled-control decisions take precedence over saved input", () => {
  for (const replacement of [[], [new Form("deployment", [new Field("comment")], "staging")],
    [new Form("deployment", [new Field("comment")], "production", "/different-run/review")]]) {
    const page = browser(), old = new Run([new Form("deployment", [new Field("comment", "secret")])]);
    const next = new Run(replacement); page.swap(old, next);
    assert.ok(next.forms.every(form => form.fields.every(field => field.value !== "secret")));
    assert.equal(next.forms.length, replacement.length, "removed forms are never reconstructed");
  }
  const page = browser(), disabled = new Field("comment"); disabled.disabled = true;
  page.swap(new Run([new Form("deployment", [new Field("comment", "secret")])]), new Run([new Form("deployment", [disabled])]));
  assert.equal(disabled.value, "");
});

test("POST, authorization failures, navigation and nonpoll sources never restore old input", () => {
  for (const options of [{ verb: "post" }, { status: 403 }, { status: 404 }, { source: new Element() },
    { beforeAfter: null }]) {
    const page = browser(), field = new Field("comment");
    const settings = options.beforeAfter === null ? { beforeAfter: () => { page.location.pathname = "/another/run"; } } : options;
    page.swap(new Run([new Form("deployment", [new Field("comment", "secret")])]), new Run([new Form("deployment", [field])]), settings);
    assert.equal(field.value, "");
  }
});

test("pristine values follow the new server defaults, and unrelated keyboard focus is never stolen", () => {
  const page = browser(), old = new Field("comment"), next = new Field("comment", "new server default");
  page.document.activeElement = old;
  const elsewhere = {};
  page.swap(new Run([new Form("deployment", [old])]), new Run([new Form("deployment", [next])]), {
    beforeAfter: () => { page.document.activeElement = elsewhere; },
  });
  assert.equal(next.value, "new server default"); assert.equal(page.document.activeElement, elsewhere);
});

test("request and final response must both identify the same deployed run, regardless of matching form actions", () => {
  const current = "https://forge.example/mount/owner/repo/actions/runs/1";
  for (const options of [
    { path: "/owner/repo/actions/runs/1" },
    { path: "/mount/owner/repo/actions/runs/2" },
    { path: "https://other.example/mount/owner/repo/actions/runs/1" },
    { responseURL: "https://forge.example/mount/owner/repo/actions/runs/2" },
    { responseURL: "https://other.example/mount/owner/repo/actions/runs/1" },
    { responseURL: "https://forge.example:444/mount/owner/repo/actions/runs/1" },
    { responseURL: "http://forge.example/mount/owner/repo/actions/runs/1" },
    { responseURL: "https://user@forge.example/mount/owner/repo/actions/runs/1" },
    { path: "http://[" }, { path: null }, { responseURL: "" }, { boosted: true },
    ...["HX-Redirect", "HX-Location", "HX-Retarget", "HX-Reswap", "HX-Refresh", "HX-Push-Url", "HX-Replace-Url"]
      .map(name => ({ headers: { [name]: current } })),
  ]) {
    const page = browser(current), field = new Field("comment");
    page.swap(new Run([new Form("deployment", [new Field("comment", "private draft")])]),
      new Run([new Form("deployment", [field])]), options);
    assert.equal(field.value, "", JSON.stringify(options));
  }
});

test("relative and absolute same-run URLs preserve subpaths and tolerate nonselecting query and fragment normalization", () => {
  const current = "https://forge.example/mount/owner/repo/actions/runs/1?b=two+words&a=1#jobs";
  for (const options of [
    { path: "?a=1&b=two%20words", responseURL: current },
    { path: "/mount/owner/repo/actions/runs/1", responseURL: current.split("?")[0] },
    { path: "./1#fragment", responseURL: "https://forge.example/mount/owner/repo/actions/runs/1?unused=value" },
  ]) {
    const page = browser(current), field = new Field("comment");
    page.swap(new Run([new Form("deployment", [new Field("comment", "draft")])]),
      new Run([new Form("deployment", [field])]), options);
    assert.equal(field.value, "draft");
  }
});

test("read-only and inherited fieldset disabled controls remain under the new server decision", () => {
  for (const property of ["readOnly", "fieldsetDisabled"]) {
    const page = browser(), field = new Field("comment"); field[property] = true;
    page.swap(new Run([new Form("deployment", [new Field("comment", "draft")])]), new Run([new Form("deployment", [field])]));
    assert.equal(field.value, "");
  }
});
