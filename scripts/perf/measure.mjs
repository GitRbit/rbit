// Serial paired HTTP measurements, not browser rendering or cache-use measurements.
// Configuration and protocol: scripts/perf/README.md. No browser or build is launched.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, appendFileSync } from "node:fs";
import http from "node:http";
import { StringDecoder } from "node:string_decoder";
import { DEFAULT_PAGE_MARKER, streamingMarker, validateMarker } from "./http-marker.mjs";
import { renderHeaderMs, viewHeaderMs, footerSsrMs } from "./http-timing.mjs";
import { sessionCookie, publicConfiguration } from "./http-auth.mjs";
import { cpus, freemem, platform, release, totalmem } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { fingerprint, orderForRound, summarize } from "./support.mjs";

export function validateConfig(input) {
  const config = { rounds: 6, warmups: 2, timeoutMs: 60000, maxBytes: 64 * 1024 * 1024, ...input };
  const known = new Set(["rounds", "warmups", "timeoutMs", "maxBytes", "variants", "pages", "output", "fixture", "notes", "acceptLanguage"]);
  if (Object.keys(input).some(key => !known.has(key))) throw new Error("unknown comparison option; never embed credentials or cookies in configuration");
  for (const [key, minimum, maximum] of [["rounds", 3, 100], ["warmups", 1, 20], ["timeoutMs", 1, 300000], ["maxBytes", 1, 1024 ** 3]]) {
    if (!Number.isInteger(config[key]) || config[key] < minimum || config[key] > maximum) throw new Error(`invalid ${key}`);
  }
  if (!Array.isArray(config.variants) || config.variants.length !== 2) throw new Error("exactly two variants are required");
  if (!config.output || !config.fixture || !config.notes) throw new Error("output, fixture and notes are required");
  const names = new Set();
  const authenticated = config.variants.filter(variant => variant.credentialsFile).length;
  if (authenticated !== 0 && authenticated !== 2) throw new Error("both variants must use the same authentication mode");
  if (config.acceptLanguage !== undefined && (typeof config.acceptLanguage !== "string" || !/^[A-Za-z0-9,;=. *-]{1,128}$/.test(config.acceptLanguage))) throw new Error("invalid acceptLanguage");
  for (const variant of config.variants) {
    if (Object.keys(variant).some(key => !["name", "baseUrl", "metadata", "credentialsFile", "requireViewTiming"].includes(key))) throw new Error("unknown variant option; use a private credentialsFile");
    if (variant.credentialsFile !== undefined && (typeof variant.credentialsFile !== "string" || !variant.credentialsFile)) throw new Error("invalid private credentials file path");
    if (variant.requireViewTiming !== undefined && typeof variant.requireViewTiming !== "boolean") throw new Error("invalid requireViewTiming");
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
    validateMarker(page.contains === undefined ? DEFAULT_PAGE_MARKER : page.contains);
    if (page.mode !== undefined && !["document", "partial", "preload", "revalidate"].includes(page.mode)) throw new Error("invalid page mode");
    if (page.status !== undefined && ![200, 204, 304].includes(page.status)) throw new Error("expected status must be 200, 204 or 304");
    if (page.status === 304 && page.mode !== "revalidate") throw new Error("304 cases must use revalidate mode");
    if (page.requireFooter !== undefined && typeof page.requireFooter !== "boolean") throw new Error("invalid requireFooter");
    if (page.requireTiming !== undefined && typeof page.requireTiming !== "boolean") throw new Error("invalid requireTiming");
    if (page.mode === "partial" && !page.scope) throw new Error("partial pages require a repository scope");
  }
  return config;
}

export function requestSample(url, page, options, agent) {
  const marker = streamingMarker(page.contains === undefined ? DEFAULT_PAGE_MARKER : page.contains);
  const headers = { "Accept-Encoding": "identity", Accept: "text/html", "Accept-Language": options.acceptLanguage ?? "en" };
  if (options.cookie) headers.Cookie = options.cookie;
  if (options.ifNoneMatch) headers["If-None-Match"] = options.ifNoneMatch;
  if (["partial", "preload"].includes(page.mode)) Object.assign(headers, { "HX-Request": "true", "Rbit-Nav-Epoch": "benchmark" });
  if (page.scope) headers["Rbit-Partial"] = page.scope;
  if (page.mode === "preload") headers["HX-Preloaded"] = "true";
  return new Promise((done, reject) => {
    const start = performance.now();
    const request = http.get(url, { agent, headers }, (response) => {
      const headersMs = performance.now() - start;
      let bytes = 0;
      let tail = "";
      const decoder = new StringDecoder("utf8");
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
        const decoded = decoder.write(chunk);
        marker.push(decoded);
        tail = (tail + decoded).slice(-65536);
      });
      // A truncated body can fail after headers without a request-level error.
      response.once("error", (error) => { clearTimeout(timer); reject(error); });
      response.once("end", () => {
        const decoded = decoder.end();
        marker.push(decoded);
        tail = (tail + decoded).slice(-65536);
        const totalMs = performance.now() - start;
        clearTimeout(timer);
        const errors = [];
        const status = response.statusCode;
        if (status !== (page.status ?? (page.mode === "revalidate" ? 304 : 200))) errors.push(`unexpected HTTP ${status}`);
        if (response.headers["content-encoding"] && response.headers["content-encoding"] !== "identity") errors.push("server ignored identity encoding");
        if (status === 200 && !String(response.headers["content-type"]).includes("text/html")) errors.push("expected HTML");
        if (status === 200 && !marker.found) errors.push("expected page marker missing from streamed HTML");
        if ([204, 304].includes(status) && bytes !== 0) errors.push("empty-status response contained a body");
        const timing = response.headers["server-timing"] ?? "";
        const render = renderHeaderMs(timing);
        const view = viewHeaderMs(timing);
        if (options.requireViewTiming && view === null) errors.push("valid footer-interval view Server-Timing missing");
        const footer = status === 200 ? footerSsrMs(tail) : null;
        if (page.requireTiming && render === null) errors.push("valid render Server-Timing missing");
        if (page.requireFooter && status === 200 && footer === null) errors.push("valid SSR footer missing from last 64 KiB");
        if (page.mode === "revalidate" && !response.headers.etag) errors.push("304 validator missing");
        done({
          url: String(url), requestHeaders: { ...headers, ...(headers.Cookie ? { Cookie: "[redacted]" } : {}) }, status, valid: errors.length === 0, errors,
          headersMs, totalMs, renderMs: render, viewMs: view, footerSsrMs: footer, bodyBytes: bytes, bodySha256: hash.digest("hex"),
          responseHeaders: Object.fromEntries(["content-type", "content-length", "content-encoding", "server-timing", "cache-control", "vary", "etag", "location"].map((name) => [name, response.headers[name] ?? null])),
        });
      });
    });
    const timer = setTimeout(() => request.destroy(new Error(`request exceeded ${options.timeoutMs} ms`)), options.timeoutMs);
    request.once("error", (error) => { clearTimeout(timer); reject(error); });
  });
}

async function main() {
  if (process.argv.length !== 3) throw new Error("usage: node scripts/perf/measure.mjs CONFIG.json");
  const file = resolve(process.argv[2]);
  const config = validateConfig(JSON.parse(readFileSync(file, "utf8")));
  const relative = (path) => resolve(dirname(file), path);
  const metadata = config.variants.map((variant) => JSON.parse(readFileSync(relative(variant.metadata), "utf8")));
  if (new Set(metadata.map((item) => item.profile)).size !== 1 || !["debug", "release"].includes(metadata[0].profile)) throw new Error("comparison requires matching declared release/debug profiles");
  for (const [index, variant] of config.variants.entries()) {
    if (new URL(metadata[index].baseUrl).origin !== new URL(variant.baseUrl).origin || !metadata[index].executableSha256) throw new Error("server metadata must match its variant origin and include a binary hash");
  }
  const output = relative(config.output);
  mkdirSync(dirname(output), { recursive: true });
  const report = {
    schema: 1, startedAt: new Date().toISOString(), workload: `${config.variants[0].credentialsFile ? "authenticated" : "anonymous"} serial HTTP, warmed server, identity body encoding, persistent connection per variant, no browser cache`,
    config: publicConfiguration(config), servers: metadata, fixture: fingerprint(relative(config.fixture)),
    machine: { platform: platform(), release: release(), node: process.version, cpu: cpus()[0]?.model, logicalCpus: cpus().length, totalMemory: totalmem(), freeMemoryAtStart: freemem() },
    samples: [],
  };
  writeFileSync(`${output}.jsonl`, `${JSON.stringify({ type: "metadata", ...report })}\n`, { flag: "wx" });
  writeFileSync(output, `${JSON.stringify({ ...report, complete: false }, null, 2)}\n`, { flag: "wx" });
  const cookies = await Promise.all(config.variants.map(variant => variant.credentialsFile ? sessionCookie(variant.baseUrl, relative(variant.credentialsFile), config.timeoutMs) : null));
  const validators = new Map();
  const agents = config.variants.map(() => new http.Agent({ keepAlive: true, maxSockets: 1 }));
  try {
    for (const phase of ["warmup", "measured"]) {
      const rounds = phase === "warmup" ? config.warmups : config.rounds;
      for (let round = 0; round < rounds; round++) {
        for (const page of config.pages) {
          for (const index of orderForRound([0, 1], round)) {
            const variant = config.variants[index];
            const sample = { phase, round: round + 1, variant: variant.name, page: page.name, startedAt: new Date().toISOString() };
            try {
              const options = { ...config, cookie: cookies[index], requireViewTiming: variant.requireViewTiming };
              if (page.mode === "revalidate") {
                const key = `${index}:${page.path}`;
                if (!validators.has(key)) {
                  const prime = await requestSample(new URL(page.path, variant.baseUrl), { ...page, mode: "document", status: 200 }, options, agents[index]);
                  report.samples.push({ ...sample, phase: "cache-prime", ...prime });
                  appendFileSync(`${output}.jsonl`, `${JSON.stringify(report.samples.at(-1))}\n`);
                  if (!prime.valid || !prime.responseHeaders.etag) throw new Error("revalidation prime did not return valid HTML and ETag");
                  validators.set(key, prime.responseHeaders.etag);
                }
                options.ifNoneMatch = validators.get(key);
              }
              Object.assign(sample, await requestSample(new URL(page.path, variant.baseUrl), page, options, agents[index]));
            }
            catch (error) { Object.assign(sample, { valid: false, errors: [error.message] }); }
            report.samples.push(sample);
            appendFileSync(`${output}.jsonl`, `${JSON.stringify(sample)}\n`);
            console.log(`${phase} ${round + 1} ${variant.name}/${page.name}: ${sample.valid ? `${sample.status}, ${sample.bodyBytes} bytes, ${sample.totalMs.toFixed(1)} ms` : sample.errors.join("; ")}`);
          }
        }
      }
    }
    report.complete = true;
  } finally {
    for (const agent of agents) agent.destroy();
    report.endedAt = new Date().toISOString();
    report.valid = Boolean(report.complete) && report.samples.every((sample) => sample.valid);
    report.summary = config.pages.flatMap((page) => config.variants.map((variant) => {
      const samples = report.samples.filter((sample) => sample.phase === "measured" && sample.page === page.name && sample.variant === variant.name && sample.valid);
      return { page: page.name, variant: variant.name, ...Object.fromEntries(["headersMs", "totalMs", "renderMs", "viewMs", "footerSsrMs", "bodyBytes"].map((metric) => [metric, summarize(samples.map((sample) => sample[metric]))])) };
    }));
    writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
    if (!report.valid) process.exitCode = 1;
    console.log(`Raw samples and summaries: ${output}; valid=${report.valid}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
