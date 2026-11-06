// The deterministic synthetic body is compared in memory, never saved as an artifact.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { productionOrigin, productionJob } from "./production-policy.mjs";

export function expectedLogText() {
  let text = "";
  for (let number = 1; number <= 1205; number++) {
    text += number === 500 ? "line 0500: <synthetic> & output\n" : `line ${String(number).padStart(4, "0")}\n`;
  }
  return text + "waiting for finish\ntask token ***\n";
}

export async function rawLogEvidence(page, gzip) {
  const evidence = await page.evaluate(async url => {
    const response = await fetch(url, { cache: "no-store" });
    // Refuse unexpectedly large bodies before hashing; no body/URL/text leaves this realm.
    const reader = response.body.getReader();
    const chunks = []; let length = 0;
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        length += value.byteLength;
        if (length > 16384) throw Error("FIXTURE_RAW_LIMIT");
        chunks.push(value);
      }
    } finally { await reader.cancel(); reader.releaseLock(); }
    const body = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) { body.set(chunk, offset); offset += chunk.byteLength; }
    const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", body));
    return { status: response.status, type: response.headers.get("content-type"),
      encoding: response.headers.get("content-encoding"), csp: response.headers.get("content-security-policy"),
      bytes: length, lines: body.reduce((count, byte) => count + (byte === 10 ? 1 : 0), 0),
      sha256: [...hash].map(byte => byte.toString(16).padStart(2, "0")).join("") };
  }, `${productionOrigin}${productionJob}/logs/raw`);
  const expected = expectedLogText();
  assert.equal(evidence.status, 200);
  assert.equal(evidence.type, "text/plain; charset=utf-8");
  assert.equal(evidence.csp, "sandbox; default-src 'none'");
  // Live raw responses still pass through the page transport compressor. Fetch
  // returns decoded bytes; a live Brotli/gzip header is not on-disk gzip evidence.
  if (gzip) assert.equal(evidence.encoding, "gzip");
  else assert.ok([null, "br", "gzip", "deflate"].includes(evidence.encoding));
  assert.equal(evidence.bytes, Buffer.byteLength(expected));
  assert.equal(evidence.lines, 1207);
  assert.equal(evidence.sha256, createHash("sha256").update(expected).digest("hex"));
  return { bytes: evidence.bytes, lines: evidence.lines, sha256: evidence.sha256,
    storage: gzip ? "final-gzip" : "live", contentEncoding: evidence.encoding };
}
