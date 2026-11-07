import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "node:http";
import { context, render } from "../../docs/design/prototypes/actions/render.mjs";
import { handler } from "../../docs/design/prototypes/actions/server.mjs";
import { logLines, rawLog } from "../../docs/design/prototypes/actions/fixtures.mjs";
const page = (screen, query = {}) => render(context(`/${screen}`, new URLSearchParams(query)));

test("all screens are server-rendered, escaped and submit only safe preview GET intents", () => {
  for (const screen of ["overview", "run", "job"]) {
    const html = page(screen, { branch: '<img src=x onerror="alert(1)">', q: '<script>alert(1)</script>' });
    assert.match(html, /<main id="main"/); assert.match(html, /Skip to Actions content/);
    assert.doesNotMatch(html, /<script>(?!<\/script>)/); assert.doesNotMatch(html, /<img src=x|<script>alert/);
    assert.doesNotMatch(html, /\sstyle=|\sonclick=|\smethod="post"/);
    for (const form of html.matchAll(/<form\b[^>]+>/g)) { assert.match(form[0], /method="get"/); assert.match(form[0], /action="\/(intent|overview|run|job)"/); }
  }
});

test("repository writes and environment reviews remain separate permission fixtures", () => {
  const reader = page("run", { viewer: "reader" });
  assert.doesNotMatch(reader, /<button[^>]*>Re-run|value="rerun-failed"|value="cancel-run"/);
  assert.match(page("run", { state: "approval", viewer: "writer" }), /name="trust_author"/);
  assert.doesNotMatch(page("run", { state: "approval", viewer: "reader" }), /name="trust_author"/);
  assert.match(page("run", { state: "deployment", viewer: "reviewer" }), /value="approve-deployment"/);
  assert.match(page("run", { state: "deployment", viewer: "reviewer" }), /value="reject-deployment"/);
  assert.doesNotMatch(page("run", { state: "deployment", viewer: "writer" }), /value="approve-deployment"/);
  assert.doesNotMatch(page("run", { state: "denied" }), /One job needs attention|a34c91e/);
});

test("long log data is real fixture content but DOM windows remain bounded with anchored lines", () => {
  assert.equal(rawLog().split("\n").length, 50000);
  assert.equal(logLines("failed").length, 80);
  const latest = page("job"); assert.equal([...latest.matchAll(/<li id="L\d+"/g)].length, 80);
  assert.match(latest, /id="L49984"/); assert.match(latest, /Show earlier lines/);
  const earlier = page("job", { earlier: "1" }); assert.match(earlier, /id="L49841"/); assert.doesNotMatch(earlier, /id="L49984"/);
  assert.match(earlier, /Return to latest lines/);
  assert.match(page("job", { state: "expired" }), /This log has expired/);
  assert.match(page("job", { state: "truncated" }), /cannot recover them/);
  assert.match(page("job", { state: "disconnected" }), /Connection lost/);
});

test("RTL and long labels are explicit diagnostics without claiming translated content", () => {
  const html = page("overview", { dir: "rtl", long: "1", theme: "dna-dark" });
  assert.match(html, /lang="en" dir="rtl" data-theme="dna-dark"/);
  assert.match(html, /layout diagnostic, not a translated locale/);
  assert.match(html, /long translated labels/);
  assert.throws(() => context("/job", new URLSearchParams({ theme: '" onload="bad' })));
});

test("preview server allowlists routes/assets and rejects mutation/private checkout requests", async (t) => {
  const publicAssets = new Map([["/prototype.css", { type: "text/css", body: "body{}" }]]);
  const server = createServer(handler(publicAssets));
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  t.after(async () => { server.closeAllConnections(); await new Promise((done) => server.close(done)); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  for (const path of ["/.git/config", "/Cargo.toml", "/.worktree-state/data", "/assets/../Cargo.toml", "/assets/secret.txt"]) {
    const response = await fetch(`${origin}${path}`); assert.equal(response.status, 404); await response.text();
  }
  const denied = await fetch(`${origin}/intent`, { method: "POST", body: "intent=rerun-all" }); assert.equal(denied.status, 405); await denied.text();
  const response = await fetch(`${origin}/run?viewer=reader`);
  assert.equal(response.status, 200); assert.match(response.headers.get("content-security-policy"), /connect-src 'none'/);
  assert.equal(response.headers.get("cache-control"), "no-store");
  assert.doesNotMatch(await response.text(), /value="rerun-failed"/);
  const simulated = await fetch(`${origin}/intent?intent=rerun-failed&return=run&viewer=reader`);
  const simulatedBody = await simulated.text();
  assert.match(simulatedBody, /No request was sent/); assert.match(simulatedBody, /not authorized/);
});
