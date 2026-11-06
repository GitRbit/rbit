import assert from "node:assert/strict";
import test from "node:test";
import { expectedLogText, rawLogEvidence } from "../../devtools/actions-visual/production-log-evidence.mjs";
import { productionOrigin, productionJob, allowedProductionRequest } from "../../devtools/actions-visual/production-policy.mjs";

test("only the pinned synthetic job, its two step windows and its whole raw read are allowed", () => {
  for (const path of [productionJob, `${productionJob}/logs?step=0`, `${productionJob}/logs?step=1`, `${productionJob}/logs/raw`]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), true, path);
    assert.equal(allowedProductionRequest(productionOrigin + path, "POST"), false, path);
  }
  for (const path of [productionJob.replace("jobs/2", "jobs/3"), `${productionJob}/logs`,
    `${productionJob}/logs?step=999`, `${productionJob}/logs?step=0&step=1`, `${productionJob}/logs?step=0&file=private`,
    `${productionJob}/logs/raw?file=private`, `${productionJob}/rerun`, `${productionJob}/cancel`]) {
    assert.equal(allowedProductionRequest(productionOrigin + path, "GET"), false, path);
  }
});

async function readBody(body, gzip, change = {}) {
  const original = globalThis.fetch;
  globalThis.fetch = async (url, options) => {
    assert.equal(url, productionOrigin + productionJob + "/logs/raw");
    assert.equal(options.cache, "no-store");
    return new Response(new ReadableStream({
      start(controller) { controller.enqueue(new TextEncoder().encode(body)); controller.close(); },
    }), { status: change.status ?? 200, headers: {
      "content-type": "text/plain; charset=utf-8", "content-security-policy": "sandbox; default-src 'none'",
      ...(gzip ? { "content-encoding": "gzip" } : {}), ...change.headers,
    } });
  };
  try {
    // The browser Fetch API exposes decompressed bytes while retaining encoding headers.
    return await rawLogEvidence({ evaluate: (operation, argument) => operation(argument) }, gzip);
  } finally { globalThis.fetch = original; }
}

test("live and browser-decoded gzip evidence has byte/digest parity without exposing body or credentials", async () => {
  const body = expectedLogText();
  assert.equal(body.split("\n").length - 1, 1207);
  assert.ok(body.includes("line 0500: <synthetic> & output\n") && body.endsWith("task token ***\n"));
  const live = await readBody(body, false), final = await readBody(body, true);
  assert.equal(live.sha256, final.sha256); assert.equal(live.bytes, final.bytes);
  assert.deepEqual(Object.keys(live).sort(), ["bytes", "contentEncoding", "lines", "sha256", "storage"]);
  assert.equal(live.storage, "live"); assert.equal(final.storage, "final-gzip");
  assert.doesNotMatch(JSON.stringify({ live, final }), /line 0500|task token|cookie|responseURL/);
});

test("live transport compression is distinct from final on-disk gzip and cannot change plaintext parity", async () => {
  for (const encoding of ["br", "gzip", "deflate"]) {
    const evidence = await readBody(expectedLogText(), false, { headers: { "content-encoding": encoding } });
    assert.equal(evidence.storage, "live"); assert.equal(evidence.contentEncoding, encoding);
    assert.equal(evidence.lines, 1207);
  }
});

test("wrong bytes, status, compression identity and oversized raw output cannot pass", async () => {
  const body = expectedLogText();
  await assert.rejects(readBody(body.replace("line 0001", "line 9999"), false));
  await assert.rejects(readBody(body, false, { status: 403 }));
  await assert.rejects(readBody(body, true, { headers: { "content-encoding": "br" } }));
  await assert.rejects(readBody(body, false, { headers: { "content-encoding": "unknown" } }));
  await assert.rejects(readBody("x".repeat(16385), false), /FIXTURE_RAW_LIMIT/);
});
