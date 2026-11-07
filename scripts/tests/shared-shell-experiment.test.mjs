import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const root = new URL("../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const templates = "crates/rbit-web/templates/";
const boundary = "{# probe-boundary: preserve source whitespace #}";
const macroImport = '{% import "macros.html" as ui %}';

test("experiment shell preserves production source bytes and final newline semantics", () => {
  const [prefix, suffix] = read(`${templates}base.html`).split("{% block content %}{% endblock %}");
  assert.equal(read(`${templates}render_probe/shell_open.html`), prefix + boundary);
  assert.equal(read(`${templates}render_probe/shell_close.html`), macroImport + suffix.replace(/\n$/, "") + boundary);
});

test("experiment bodies remain exact compiled copies of real page content", () => {
  for (const [page, body] of [["home/explore.html", "explore_body.html"], ["auth/login.html", "login_body.html"]]) {
    const source = read(templates + page);
    const content = source.split("{% block content %}")[1].split("{% endblock %}")[0];
    assert.equal(read(`${templates}render_probe/${body}`), macroImport + content + boundary);
  }
});

test("production candidate is opt-in and removes inherited derives only when selected", () => {
  const manifest = read("crates/rbit-web/Cargo.toml");
  const features = manifest.split("[features]")[1].split("[dependencies]")[0];
  assert.match(features, /^shared-shell-experiment = \[\]$/m);
  assert.doesNotMatch(features, /^default\s*=/m);
  for (const [path, name] of [["explore.rs", "ExplorePage"], ["auth.rs", "LoginPage"]]) {
    const view = read(`crates/rbit-web/src/views/${path}`).split(`pub struct ${name}`)[0];
    assert.match(view, /cfg_attr\(not\(feature = "shared-shell-experiment"\), derive\(Template\)\)/);
  }
});
