import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import http from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import test from "node:test";
import vm from "node:vm";
import { requestSample, validateConfig } from "./measure.mjs";
import { serveOptions } from "./serve.mjs";
import { fingerprint, orderForRound, summarize } from "./support.mjs";

const root = resolve(import.meta.dirname, "../..");
const config = () => ({
  fixture: root, output: "result.json", notes: "synthetic harness test; not rbit performance data",
  variants: [{ name: "before", baseUrl: "http://127.0.0.1:3005", metadata: "before.json" }, { name: "after", baseUrl: "http://127.0.0.1:3006", metadata: "after.json" }],
  pages: [{ name: "page", path: "/page" }],
});

test("configuration rejects incomparable or unsafe inputs and defaults to release", () => {
  assert.equal(serveOptions([], root).profile, "release");
  assert.equal(serveOptions(["--source-revision", "327354d"], root).declaredSourceRevision, "327354d");
  assert.equal(serveOptions(["--profile", "debug", "--port", "3011"], root).port, 3011);
  for (const args of [["--profile", "fast"], ["--port", "0"], ["--port", "1e2"], ["--unknown", "value"], ["--port"]]) assert.throws(() => serveOptions(args, root));
  assert.equal(validateConfig(config()).rounds, 6);
  for (const url of ["https://example.com", "http://example.com", "http://user@localhost:3005", "http://localhost/path"]) {
    const input = config(); input.variants[0].baseUrl = url;
    assert.throws(() => validateConfig(input));
  }
  for (const path of ["//example.com/page", "/\\example.com/page", "/page#fragment"]) {
    const input = config(); input.pages[0].path = path;
    assert.throws(() => validateConfig(input));
  }
  assert.throws(() => validateConfig({ ...config(), warmups: 0 }));
  assert.throws(() => validateConfig({ ...config(), rounds: 1 }));
});

test("source exports cannot inherit an unrelated ancestor revision", () => {
  assert.equal(fingerprint(root).repository, true);
  const nested = fingerprint(import.meta.dirname);
  assert.equal(nested.repository, false);
  assert.equal(nested.revision, null);
});

test("paired ordering alternates and summaries exclude missing timings", () => {
  assert.deepEqual([0, 1, 2, 3].map((round) => orderForRound(["a", "b"], round)), [["a", "b"], ["b", "a"], ["a", "b"], ["b", "a"]]);
  assert.deepEqual(summarize([null, undefined, 10, 20, 30]), { count: 3, median: 20, p25: 15, p75: 25, p95: 29 });
  assert.equal(summarize([]).median, null);
});

async function server(t, handler) {
  const instance = http.createServer(handler);
  await new Promise((done) => instance.listen(0, "127.0.0.1", done));
  t.after(() => { instance.closeAllConnections(); return new Promise((done) => instance.close(done)); });
  return `http://127.0.0.1:${instance.address().port}`;
}

test("HTTP samples preserve payload and reject redirects, errors, and oversized responses", async (t) => {
  const base = await server(t, (request, response) => {
    assert.equal(request.headers["accept-encoding"], "identity");
    if (request.url === "/preload") {
      assert.equal(request.headers["hx-preloaded"], "true");
      response.writeHead(204); response.end(); return;
    }
    response.writeHead(request.url === "/redirect" ? 302 : 200, { "Content-Type": "text/html", "Server-Timing": "render;dur=12.5", Location: "/page" });
    response.end('<main id="main">é</main>');
  });
  const options = { timeoutMs: 1000, maxBytes: 1024 };
  const sample = await requestSample(`${base}/page`, {}, options);
  assert.equal(sample.valid, true);
  assert.equal(sample.bodyBytes, Buffer.byteLength('<main id="main">é</main>'));
  assert.equal(sample.renderMs, 12.5);
  assert.equal(sample.bodySha256.length, 64);
  assert.ok(sample.totalMs >= sample.headersMs);
  assert.equal((await requestSample(`${base}/redirect`, {}, options)).valid, false);
  assert.equal((await requestSample(`${base}/page`, { contains: "not present" }, options)).valid, false);
  assert.equal((await requestSample(`${base}/preload`, { mode: "preload", status: 204 }, options)).bodyBytes, 0);
  await assert.rejects(requestSample(`${base}/page`, {}, { ...options, maxBytes: 2 }), /maxBytes/);
});

test("CLI records every warmup/sample and refuses mismatched profiles", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "rbit-perf-test-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const base = await server(t, (_request, response) => { response.writeHead(200, { "Content-Type": "text/html" }); response.end('<main id="main">fixture</main>'); });
  const input = { ...config(), rounds: 3, warmups: 1 };
  input.variants.forEach((variant) => { variant.baseUrl = base; });
  const metadata = { baseUrl: base, profile: "release", executableSha256: "synthetic-test-only" };
  for (const variant of input.variants) await writeFile(join(dir, variant.metadata), JSON.stringify(metadata));
  const file = join(dir, "config.json");
  await writeFile(file, JSON.stringify(input));
  await promisify(execFile)(process.execPath, [join(import.meta.dirname, "measure.mjs"), file]);
  const report = JSON.parse(await readFile(join(dir, input.output), "utf8"));
  assert.equal(report.valid, true);
  assert.deepEqual(report.samples.map((sample) => sample.variant), ["before", "after", "before", "after", "after", "before", "before", "after"]);
  assert.equal(report.summary[0].totalMs.count, 3);
  assert.equal(report.summary[0].renderMs.count, 0, "missing timing is never counted as zero");
  assert.equal((await readFile(join(dir, `${input.output}.jsonl`), "utf8")).trim().split("\n").length, 9);
  await writeFile(join(dir, "after.json"), JSON.stringify({ ...metadata, profile: "debug" }));
  await assert.rejects(promisify(execFile)(process.execPath, [join(import.meta.dirname, "measure.mjs"), file]), /matching declared/);
});

test("a truncated response rejects and leaves no request timeout keeping Node alive", async (t) => {
  const base = await server(t, (_request, response) => {
    response.writeHead(200, { "Content-Type": "text/html", "Content-Length": "1024" });
    response.write('<main id="main">incomplete', () => setImmediate(() => response.destroy()));
  });
  const script = `
    import assert from "node:assert/strict";
    const { requestSample } = await import(process.argv[1]);
    await assert.rejects(
      requestSample(process.argv[2], {}, { timeoutMs: 60000, maxBytes: 2048 }, false),
      /aborted/i
    );
  `;
  // A leaked 60-second timer makes this subprocess exceed the five-second deadline.
  // agent:false prevents a keep-alive socket from being mistaken for a timer leak.
  await promisify(execFile)(process.execPath, ["--input-type=module", "-e", script, new URL("./measure.mjs", import.meta.url).href, base], { timeout: 5000 });
});

test("browser probe rejects a hidden or unstyled page without starting a sweep", async () => {
  const source = await readFile(join(import.meta.dirname, "scroll-probe.js"), "utf8");
  for (const hidden of [true, false]) {
    const listeners = new Set();
    const context = {
      document: { visibilityState: hidden ? "hidden" : "visible", readyState: "complete", querySelectorAll: () => [], querySelector: () => null, addEventListener: (name) => listeners.add(name), removeEventListener: (name) => listeners.delete(name) },
      innerWidth: 1280, innerHeight: 900, devicePixelRatio: 1, location: { href: "http://localhost/page" },
      addEventListener: (name) => listeners.add(name), removeEventListener: (name) => listeners.delete(name),
    };
    const result = await vm.runInNewContext(source, context);
    assert.equal(result.valid, false);
    assert.ok(result.invalidReasons.includes("required stylesheet missing or unloaded"));
    assert.equal(listeners.size, 0, "invalid probes clean up their event listeners");
  }
});

test("diff selector follows rendered row kinds independently of virtual blocks", async () => {
  const probe = await readFile(join(import.meta.dirname, "scroll-probe.js"), "utf8");
  const view = await readFile(join(root, "crates/rbit-web/src/views/commit.rs"), "utf8");
  const rowView = view.slice(view.indexOf("impl DiffLineView"), view.indexOf("const NUMBER_CLASSES"));
  const kinds = new Set([...rowView.matchAll(/(?:kind:|LineKind::\w+ =>)\s*"(\w+)"/g)].map((match) => match[1]));
  assert.equal(kinds.size, 5, "read the concrete kinds emitted by DiffLineView");
  const selector = JSON.parse(probe.match(/const DIFF_ROW = ("[^"]+");/)[1]);
  const selectedKinds = new Set([...selector.matchAll(/\.diff-([a-z]+)/g)].map((match) => match[1]).filter((kind) => kind !== "view" && kind !== "comment"));
  assert.deepEqual(selectedKinds, kinds, "all rendered diff rows are measured, with or without a vblock");
  assert.match(selector, /:is\(tr,/u, "legacy table rows remain supported");
  assert.match(selector, /:not\(\.diff-comment, \.diff-comment \*\)/u, "comment rows and descendants, including Markdown tables, are excluded");
  assert.doesNotMatch(selector, /vblock|>/u, "short diff rows do not depend on a wrapper");
  assert.match(probe, /diffRows: document\.querySelectorAll\(DIFF_ROW\)/u, "counts and hit sampling use the same selector");
});
