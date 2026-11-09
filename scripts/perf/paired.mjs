// Summarizes paired measure.mjs reports: per page, each variant's median with a 95%
// bootstrap interval, p95, and the change of the second variant's median relative to
// the first with a paired (round-resampled) 95% interval. Deterministic: the bootstrap
// uses a fixed seed, so the same reports always print the same numbers.
//
//   node scripts/perf/paired.mjs [--metric totalMs] [--first NAME] [--json out.json] REPORT.json...
//
// `--first` names the reference variant, so reports that list the variants in either
// order pool correctly.
//
// A page is flagged when the second variant's median is more than 5% higher and the two
// median intervals do not overlap: slower beyond noise, not proof of a cause.
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const RESAMPLES = 2000;
export const THRESHOLD = 0.05;

function quantile(sorted, p) {
  if (!sorted.length) return null;
  const position = (sorted.length - 1) * p;
  const lower = Math.floor(position);
  return sorted[lower] + (sorted[Math.ceil(position)] - sorted[lower]) * (position - lower);
}
const median = (values) => quantile([...values].sort((a, b) => a - b), 0.5);

function generator(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** `pairs` is [[first, second], ...] per measured round (one report or several pooled). */
export function comparePairs(pairs, seed = 0x5eed) {
  const random = generator(seed);
  const first = pairs.map(([a]) => a);
  const second = pairs.map(([, b]) => b);
  const changes = [], firstMedians = [], secondMedians = [];
  for (let i = 0; i < RESAMPLES; i++) {
    const picked = Array.from(pairs, () => pairs[Math.floor(random() * pairs.length)]);
    const a = median(picked.map(([x]) => x));
    const b = median(picked.map(([, y]) => y));
    firstMedians.push(a); secondMedians.push(b); changes.push(b / a - 1);
  }
  for (const list of [changes, firstMedians, secondMedians]) list.sort((x, y) => x - y);
  const interval = (list) => [quantile(list, 0.025), quantile(list, 0.975)];
  const result = {
    count: pairs.length,
    first: { median: median(first), interval: interval(firstMedians), p95: quantile([...first].sort((x, y) => x - y), 0.95) },
    second: { median: median(second), interval: interval(secondMedians), p95: quantile([...second].sort((x, y) => x - y), 0.95) },
    change: median(second) / median(first) - 1, changeInterval: interval(changes),
  };
  result.flagged = result.change > THRESHOLD && result.second.interval[0] > result.first.interval[1];
  return result;
}

/** Measured, valid pairs per page from complete reports, matched by round. */
export function pagePairs(reports, metric, firstName) {
  const pages = new Map();
  for (const report of reports) {
    if (!report.valid) throw new Error("only complete valid reports can be summarized");
    const names = report.config.variants.map((variant) => variant.name);
    if (firstName && !names.includes(firstName)) throw new Error(`no variant named ${firstName}`);
    const [first, second] = firstName && names[1] === firstName ? names.reverse() : names;
    const measured = report.samples.filter((sample) => sample.phase === "measured");
    for (const page of report.config.pages) {
      const value = (variant, round) => measured.find((sample) => sample.page === page.name && sample.variant === variant && sample.round === round)?.[metric];
      const list = pages.get(page.name) ?? { names: [first, second], pairs: [] };
      for (let round = 1; round <= report.config.rounds; round++) {
        const pair = [value(first, round), value(second, round)];
        if (pair.every(Number.isFinite)) list.pairs.push(pair);
      }
      pages.set(page.name, list);
    }
  }
  return pages;
}

function main(argv) {
  let metric = "totalMs", json = null, firstName = null;
  const files = [];
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--metric") metric = argv[++i];
    else if (argv[i] === "--json") json = argv[++i];
    else if (argv[i] === "--first") firstName = argv[++i];
    else files.push(argv[i]);
  }
  if (!files.length || !metric) throw new Error("usage: paired.mjs [--metric totalMs] [--first NAME] [--json out.json] REPORT.json...");
  const reports = files.map((file) => JSON.parse(readFileSync(resolve(file), "utf8")));
  const rows = [];
  for (const [page, { names, pairs }] of pagePairs(reports, metric, firstName)) {
    if (pairs.length < 3) continue;
    rows.push({ page, metric, variants: names, ...comparePairs(pairs) });
  }
  const ms = (value) => value >= 100 ? value.toFixed(0) : value >= 10 ? value.toFixed(1) : value.toFixed(2);
  const pct = (value) => `${value >= 0 ? "+" : ""}${(value * 100).toFixed(1)}%`;
  for (const row of rows) {
    console.log(`${row.page.padEnd(22)} n=${row.count} ${ms(row.first.median)} [${row.first.interval.map(ms).join("-")}] p95 ${ms(row.first.p95)} -> ${ms(row.second.median)} [${row.second.interval.map(ms).join("-")}] p95 ${ms(row.second.p95)}  ${pct(row.change)} (${row.changeInterval.map(pct).join(" to ")})${row.flagged ? "  FLAGGED" : ""}`);
  }
  if (json) writeFileSync(resolve(json), `${JSON.stringify({ metric, files: files.length, rows }, null, 2)}\n`, { flag: "wx" });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(process.argv.slice(2)); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
