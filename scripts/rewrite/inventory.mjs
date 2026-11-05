/** Generate the compatibility inventory from a pinned commit, including schema fingerprints. */
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readSnapshot } from "./git-snapshot.mjs";

export const BASELINE = "39f932a00b7aa35b324e06a9ff2343a57d4bed76";
const METHODS = new Set(["get", "post", "put", "patch", "delete", "head", "options"]);

function selected(path) {
  return /^(crates|scripts|\.github|\.cargo|\.config)\//.test(path)
    && /\.(rs|toml|sql|html|mjs|js|json|ya?ml)$/.test(path)
    || ["Cargo.toml", "Cargo.lock", "package.json", "package-lock.json"].includes(path);
}

function lines(source) {
  return source === "" ? 0 : source.split("\n").length - Number(source.endsWith("\n"));
}

function apiPaths(path, source) {
  if (!path.includes("/openapi/") && !path.endsWith("/openapi.json")) return [];
  const schema = JSON.parse(source);
  return Object.entries(schema.paths ?? {}).flatMap(([url, methods]) =>
    Object.keys(methods).filter((method) => METHODS.has(method))
      .map((method) => ({ method: method.toUpperCase(), path: url, source: path })));
}

/** Counts source declarations, not runtime reachability; OpenAPI and integration tests complement it. */
export function summarizeSnapshot({ commit, files }) {
  const rust = [], migrations = [], routes = [], openapi = [], manifests = [], configuration = [];
  const authoredFiles = [];
  for (const [path, bytes] of files) {
    const source = bytes.toString("utf8");
    const entry = { path, lines: lines(source), sha256: createHash("sha256").update(bytes).digest("hex") };
    if (/\.(rs|html|mjs|js)$/.test(path) && !/\/assets\/(?:vendor|mermaid)/.test(path)) authoredFiles.push(entry);
    if (path.endsWith(".rs")) rust.push(entry);
    if (/^crates\/rbit-storage\/migrations\/[^/]+\.sql$/.test(path)) migrations.push(entry);
    if (path.endsWith("Cargo.toml")) {
      const name = /^name\s*=\s*"([^"]+)"/m.exec(source)?.[1];
      if (name) manifests.push({ name, path, internalDependencies: [...source.matchAll(/^(rbit-[\w-]+)(?:\.workspace)?\s*=/gm)].map((m) => m[1]).sort() });
    }
    if (/^crates\/rbit-core\/src\/config(?:\/|\.rs)/.test(path)) configuration.push(entry);
    if (/^crates\/rbit-web\/src\/routes\//.test(path)) {
      for (const match of source.matchAll(/\.route\s*\(\s*"([^"\n]+)"/g)) {
        routes.push({ path: match[1], source: path, line: source.slice(0, match.index).split("\n").length });
      }
    }
    if (path.endsWith(".json")) openapi.push(...apiPaths(path, source));
  }
  return {
    formatVersion: 1, baseline: commit,
    scope: "Immutable tracked source; route declarations are static evidence, not a complete runtime route table.",
    totals: { crates: manifests.length, rustFiles: rust.length, rustLines: rust.reduce((sum, f) => sum + f.lines, 0), rustFilesOver400: rust.filter((f) => f.lines > 400).length },
    crates: manifests, migrations, configuration,
    routeDeclarations: routes, openapiOperations: openapi,
    rustFiles: rust, authoredSizeBaseline: authoredFiles.filter((f) => f.lines > 400),
    testSources: rust.filter((f) => f.path.includes("/tests/") || /(?:^|\/)test[^/]*\.rs$/.test(f.path)),
    buildInputs: [...files].filter(([path]) => ["Cargo.lock", "Cargo.toml", "package.json", "package-lock.json", ".cargo/config.toml"].includes(path))
      .map(([path, bytes]) => ({ path, sha256: createHash("sha256").update(bytes).digest("hex") })),
  };
}

export function inventory(root, revision = BASELINE) {
  return summarizeSnapshot(readSnapshot(root, revision, selected));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [, , output, revision = BASELINE] = process.argv;
  if (!output) throw new Error("Usage: node scripts/rewrite/inventory.mjs <output.json> [revision]");
  const report = inventory(process.cwd(), revision);
  writeFileSync(resolve(output), `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`Inventoried ${report.baseline}: ${report.totals.crates} crates, ${report.totals.rustFiles} Rust files, ${report.migrations.length} migrations\n`);
}
