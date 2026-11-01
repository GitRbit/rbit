// Serial paired HTTP measurements, not browser rendering or cache-use measurements.
// Configuration and protocol: scripts/perf/README.md. No browser or build is launched.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, appendFileSync } from "node:fs";
import http from "node:http";
import { cpus, freemem, platform, release, totalmem } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { fingerprint, hashFile, orderForRound, summarize } from "../../../../scripts/perf/support.mjs";
import { bootstrapContexts, postflightContexts, hash, publicHeaders } from "./contexts.mjs";

function pageMarker(page) {
  for (const key of ['requiredMarkers', 'forbiddenMarkers']) if (page[key] !== undefined && !Array.isArray(page[key])) throw new Error('Semantic marker lists must be arrays');
  const values = [page.contains ?? 'id="main"', ...(page.requiredMarkers ?? []), ...(page.forbiddenMarkers ?? [])];
  if (values.length > 17 || values.some(value => typeof value !== 'string' || !value || Buffer.byteLength(value) > 256)) throw new Error('Semantic markers require at most 17 nonempty strings of 1-256 bytes');
  return values.map((value, index) => ({ bytes: Buffer.from(value), forbidden: index >= 1 + (page.requiredMarkers ?? []).length, seen: false, tail: Buffer.alloc(0) }));
}

export function validateConfig(input) {
  const config = { rounds: 6, warmups: 2, timeoutMs: 60000, maxBytes: 64 * 1024 * 1024, ...input };
  for (const [key, minimum, maximum] of [["rounds", 3, 100], ["warmups", 1, 20], ["timeoutMs", 1, 300000], ["maxBytes", 1, 1024 ** 3]]) {
    if (!Number.isInteger(config[key]) || config[key] < minimum || config[key] > maximum) throw new Error(`invalid ${key}`);
  }
  if (!Array.isArray(config.variants) || config.variants.length !== 2) throw new Error("exactly two variants are required");
  if (!config.output || !config.fixture || !config.notes) throw new Error("output, fixture and notes are required");
  if (!config.requestContexts || typeof config.requestContexts !== "object" || Array.isArray(config.requestContexts)) throw new Error("Explicit request contexts are required");
  const names = new Set();
  for (const variant of config.variants) {
    const url = new URL(variant.baseUrl);
    if (url.protocol !== "http:" || !["127.0.0.1", "[::1]", "localhost"].includes(url.hostname) || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
      throw new Error("baseUrl must be an HTTP loopback origin without credentials or a path");
    }
    if (!variant.name || names.has(variant.name) || !variant.metadata) throw new Error("variants need unique names and server metadata files");
    names.add(variant.name);
  }
  if (!Array.isArray(config.pages) || config.pages.length === 0) throw new Error("pages are required");
  const pages = new Set();
  for (const page of config.pages) {
    if (!page.name || pages.has(page.name) || typeof page.path !== "string" || !page.path.startsWith("/") || page.path.startsWith("//") || /[\\#\r\n]/.test(page.path)) throw new Error("invalid or duplicate page");
    pages.add(page.name);
    if (page.mode !== undefined && !["document", "partial", "preload"].includes(page.mode)) throw new Error("invalid page mode");
    if (page.status !== undefined && ![200, 204].includes(page.status)) throw new Error("expected status must be 200 or 204");
    if (page.mode === "partial" && !page.scope) throw new Error("partial pages require a repository scope");
    if (!config.requestContexts[page.context]) throw new Error("Page request context is not declared");
    pageMarker(page);
  }
  return config;
}

export function requestSample(url, page, options, agent, jar = null) {
  const markers = pageMarker(page);
  const headers = { "Accept-Encoding": "identity", Accept: "text/html" };
  const cookie = jar?.header(url) ?? "";
  if (cookie) headers.Cookie = cookie;
  const cookieFingerprint = cookie ? hash(cookie) : null;
  if (["partial", "preload"].includes(page.mode)) Object.assign(headers, { "HX-Request": "true", "Rbit-Nav-Epoch": "benchmark" });
  if (page.scope) headers["Rbit-Partial"] = page.scope;
  if (page.mode === "preload") headers["HX-Preloaded"] = "true";
  return new Promise((done, reject) => {
    const start = performance.now();
    const request = http.get(url, { agent, headers }, (response) => {
      const headersMs = performance.now() - start;
      let bytes = 0;
      const hash = createHash("sha256");
      response.on("data", (chunk) => {
        bytes += chunk.length;
        if (bytes > options.maxBytes) {
          clearTimeout(timer);
          reject(new Error("response exceeded maxBytes"));
          request.destroy();
          return;
        }
        hash.update(chunk);
        for (const marker of markers) {
          if (marker.seen) continue;
          const overlap = marker.bytes.length - 1;
          marker.seen = chunk.includes(marker.bytes) || Buffer.concat([marker.tail, chunk.subarray(0, overlap)]).includes(marker.bytes);
          marker.tail = marker.seen ? Buffer.alloc(0) : chunk.length >= overlap
            ? Buffer.from(chunk.subarray(chunk.length - overlap))
            : Buffer.from(Buffer.concat([marker.tail, chunk]).subarray(-overlap));
        }
      });
      // A truncated body can fail after headers without a request-level error.
      response.once("error", (error) => { clearTimeout(timer); reject(error); });
      response.once("end", () => {
        const totalMs = performance.now() - start;
        clearTimeout(timer);
        const errors = [];
        const status = response.statusCode;
        if (status !== (page.status ?? 200)) errors.push(`unexpected HTTP ${status}`);
        if (response.headers["content-encoding"] && response.headers["content-encoding"] !== "identity") errors.push("server ignored identity encoding");
        if (status === 200 && !String(response.headers["content-type"]).includes("text/html")) errors.push("expected HTML");
        if (status === 200 && markers.some(marker => marker.forbidden ? marker.seen : !marker.seen)) errors.push("registered semantic marker check failed");
        if (page.contentLanguage && response.headers["content-language"] !== page.contentLanguage) errors.push("locale metadata differs from registered workload");
        try { jar?.absorb(url, response.headers["set-cookie"], true); } catch { errors.push("private cookie state changed after bootstrap"); }
        if (status === 204 && bytes !== 0) errors.push("204 response contained a body");
        const timing = response.headers["server-timing"] ?? "";
        const render = /(?:^|,)\s*render;dur=([\d.]+)/.exec(timing);
        done({
          url: String(url), requestContext: page.context, cookieFingerprint, requestHeaders: publicHeaders(headers), status, valid: errors.length === 0, errors,
          headersMs, totalMs, renderMs: render ? Number(render[1]) : null, bodyBytes: bytes, bodySha256: hash.digest("hex"),
          responseHeaders: Object.fromEntries(["content-type", "content-length", "content-encoding", "server-timing", "cache-control", "vary", "etag", "content-language", "rbit-locale-revision"].map((name) => [name, response.headers[name] ?? null])),
        });
      });
    });
    const timer = setTimeout(() => request.destroy(new Error(`request exceeded ${options.timeoutMs} ms`)), options.timeoutMs);
    request.once("error", (error) => { clearTimeout(timer); reject(error); });
  });
}

async function main() {
  if (process.argv.length !== 3) throw new Error("usage: node scripts/perf/i18n-extraction/harness/measure.mjs CONFIG.json");
  const file = resolve(process.argv[2]);
  const config = validateConfig(JSON.parse(readFileSync(file, "utf8")));
  const relative = (path) => resolve(dirname(file), path);
  const metadata = config.variants.map((variant) => JSON.parse(readFileSync(relative(variant.metadata), "utf8")));
  if (new Set(metadata.map((item) => item.profile)).size !== 1 || !["debug", "release"].includes(metadata[0].profile)) throw new Error("comparison requires matching declared release/debug profiles");
  for (const [index, variant] of config.variants.entries()) {
    if (new URL(metadata[index].baseUrl).origin !== new URL(variant.baseUrl).origin || !metadata[index].executableSha256) throw new Error("server metadata must match its variant origin and include a binary hash");
  }
  const bootstraps = [];
  for (const variant of config.variants) bootstraps.push(await bootstrapContexts(variant, config));
  const output = relative(config.output);
  mkdirSync(dirname(output), { recursive: true });
  const report = {
    schema: 1, startedAt: new Date().toISOString(), workload: "registered origin-scoped guest/admin or anonymous serial HTTP, warmed server, identity body encoding, persistent connection per variant, no browser cache",
    bootstrap: bootstraps.map((result, index) => ({ variant: config.variants[index].name, contexts: result.evidence })),
    config, servers: metadata, fixture: fingerprint(relative(config.fixture)),
    harness: {
      source: fingerprint(resolve(import.meta.dirname, "../../../..")),
      measureSha256: await hashFile(fileURLToPath(import.meta.url)),
      supportSha256: await hashFile(resolve(import.meta.dirname, "../../../../scripts/perf/support.mjs")),
      contextsSha256: await hashFile(resolve(import.meta.dirname, "contexts.mjs")),
    },
    machine: { platform: platform(), release: release(), node: process.version, cpu: cpus()[0]?.model, logicalCpus: cpus().length, totalMemory: totalmem(), freeMemoryAtStart: freemem() },
    samples: [],
  };
  writeFileSync(`${output}.jsonl`, `${JSON.stringify({ type: "metadata", ...report })}\n`, { flag: "wx" });
  writeFileSync(output, `${JSON.stringify({ ...report, complete: false }, null, 2)}\n`, { flag: "wx" });
  const agents = config.variants.map(() => new http.Agent({ keepAlive: true, maxSockets: 1 }));
  try {
    for (const phase of ["warmup", "measured"]) {
      const rounds = phase === "warmup" ? config.warmups : config.rounds;
      for (let round = 0; round < rounds; round++) {
        for (const page of config.pages) {
          for (const index of orderForRound([0, 1], round)) {
            const variant = config.variants[index];
            const sample = { phase, round: round + 1, variant: variant.name, page: page.name, startedAt: new Date().toISOString() };
            try { Object.assign(sample, await requestSample(new URL(page.path, variant.baseUrl), page, config, agents[index], bootstraps[index].contexts.get(page.context))); }
            catch { Object.assign(sample, { valid: false, errors: ["registered request transport/context failed"] }); }
            report.samples.push(sample);
            appendFileSync(`${output}.jsonl`, `${JSON.stringify(sample)}\n`);
            console.log(`${phase} ${round + 1} ${variant.name}/${page.name}: ${sample.valid ? `${sample.status}, ${sample.bodyBytes} bytes, ${sample.totalMs.toFixed(1)} ms` : sample.errors.join("; ")}`);
          }
        }
      }
    }
    report.postflight = [];
    for (const [index, variant] of config.variants.entries()) report.postflight.push({ variant: variant.name, pages: await postflightContexts(variant, config, bootstraps[index].contexts) });
    report.complete = true;
  } finally {
    for (const agent of agents) agent.destroy();
    report.endedAt = new Date().toISOString();
    report.valid = Boolean(report.complete) && report.samples.every((sample) => sample.valid);
    report.summary = config.pages.flatMap((page) => config.variants.map((variant) => {
      const samples = report.samples.filter((sample) => sample.phase === "measured" && sample.page === page.name && sample.variant === variant.name && sample.valid);
      return { page: page.name, variant: variant.name, ...Object.fromEntries(["headersMs", "totalMs", "renderMs", "bodyBytes"].map((metric) => [metric, summarize(samples.map((sample) => sample[metric]))])) };
    }));
    writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
    if (!report.valid) process.exitCode = 1;
    console.log(`Raw samples and summaries: ${output}; valid=${report.valid}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
