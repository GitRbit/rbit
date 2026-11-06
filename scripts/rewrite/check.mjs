/** Ratchet legacy file sizes and dependencies while enforcing strict bounds on new code. */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { checkIdentityFeatures, checkIdentityModelGraphs, checkIdentityTokenGraph } from "./identity-features.mjs";

export const LIMIT = 400;
const GENERATED = new Set([
  "crates/rbit-web/assets/app.bundle.js", "crates/rbit-web/src/themes/catalog.rs",
]);
const FEATURE_EDGES = {
  "rbit-kernel": [], "rbit-contracts": [], "rbit-localization": [], "rbit-rewrite-check": [], "rbit-render-probe": [],
  "rbit-identity": ["rbit-contracts", "rbit-kernel"],
  "rbit-access": [],
  "rbit-repositories": ["rbit-contracts", "rbit-access", "rbit-kernel"],
  "rbit-collaboration": ["rbit-contracts", "rbit-access", "rbit-repositories", "rbit-kernel"],
  "rbit-actions": ["rbit-contracts", "rbit-access", "rbit-workflow", "rbit-kernel"],
  "rbit-presentation": ["rbit-contracts", "rbit-localization"],
};
// Reviewed seams added to legacy adapters during incremental replacement.
const ADDED_EDGES = {
  "rbit-core": ["rbit-contracts", "rbit-access", "rbit-identity"], "rbit-web": ["rbit-localization"],
  "rbit-auth": ["rbit-identity", "rbit-access"], "rbit-services": ["rbit-identity", "rbit-kernel"],
  "rbit-server": ["rbit-kernel"],
  "rbit-storage": ["rbit-identity"],
};
// HTTP acceptance fixtures may inject the shared owner without adding production edges.
const DEV_EDGES = { "rbit-web": ["rbit-identity", "rbit-kernel"] };

export function authored(path) {
  const prototypeStyle = path.startsWith("docs/design/prototypes/") && path.endsWith(".css");
  return (/\.(rs|html|mjs|js|ps1|cs)$/.test(path) || prototypeStyle) && !GENERATED.has(path)
    && !path.startsWith("crates/rbit-web/assets/vendor/")
    && !path.startsWith("crates/rbit-web/assets/mermaid/");
}

/** Source trees never follow directory links into another checkout or generated cache. */
export function sourceFiles(root) {
  const result = [];
  function visit(path) {
    for (const entry of readdirSync(join(root, path), { withFileTypes: true })) {
      const next = `${path}/${entry.name}`;
      if (entry.isSymbolicLink()) throw new Error(`Source tree contains a link: ${next}`);
      if (entry.isDirectory()) visit(next);
      else if (entry.isFile() && authored(next)) result.push(next);
    }
  }
  for (const directory of ["crates", "scripts", "tools", "docs/design/prototypes"]) {
    try { visit(directory); } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  return result.sort();
}

export function checkSizes(files, baseline) {
  const ceilings = new Map(baseline.authoredSizeBaseline.map((entry) => [entry.path, entry.lines]));
  return [...files].flatMap(([path, source]) => {
    const count = source === "" ? 0 : source.split("\n").length - Number(source.endsWith("\n"));
    const allowed = Math.max(LIMIT, ceilings.get(path) ?? LIMIT);
    return count > allowed ? [`${path}: ${count} physical lines exceeds ${allowed}; split by responsibility`] : [];
  });
}

/** Dependency names come from Cargo, including renamed, target-specific and dev edges. */
export function checkEdges(metadata, baseline) {
  const legacy = new Map(baseline.crates.map((crate) => [crate.name, crate.internalDependencies]));
  const members = new Map(metadata.packages.filter((pkg) => metadata.workspace_members.includes(pkg.id))
    .map((pkg) => [pkg.name, pkg]));
  const violations = [];
  for (const [name, pkg] of members) {
    const allowed = FEATURE_EDGES[name] ?? (legacy.has(name)
      ? [...legacy.get(name), ...(ADDED_EDGES[name] ?? [])] : undefined);
    if (!allowed) { violations.push(`${name}: declare an explicit architecture policy before adding a workspace crate`); continue; }
    for (const dependency of pkg.dependencies) {
      const testOnly = dependency.kind === "dev" && (DEV_EDGES[name] ?? []).includes(dependency.name);
      if (members.has(dependency.name) && !allowed.includes(dependency.name) && !testOnly) {
        violations.push(`${name} -> ${dependency.name}: forbidden dependency (${dependency.kind ?? "normal"})`);
      }
    }
  }
  return violations;
}

export function checkMigrations(root, baseline) {
  return baseline.migrations.flatMap(({ path, sha256 }) => {
    try {
      const source = readFileSync(join(root, path), "utf8").replaceAll("\r\n", "\n");
      return createHash("sha256").update(source).digest("hex") === sha256 ? []
        : [`${path}: immutable baseline migration changed; add a new migration instead`];
    } catch (error) {
      if (error.code === "ENOENT") return [`${path}: immutable baseline migration missing`];
      throw error;
    }
  });
}

/** A completed ledger row needs replacement source and concrete acceptance evidence. */
export function checkLedger(root, ledger) {
  const violations = [], ids = new Set();
  const safe = (path) => typeof path === "string" && path.length > 0 && !isAbsolute(path)
    && !path.split(/[\\/]/).includes("..") && existsSync(join(root, path));
  for (const feature of ledger.features) {
    if (ids.has(feature.id)) violations.push(`Feature ledger: duplicate ${feature.id}`);
    ids.add(feature.id);
    if (!["baseline-retained", "ported", "changed"].includes(feature.status)) {
      violations.push(`${feature.id}: unknown ledger status`);
    }
    if (feature.status === "baseline-retained") {
      for (const field of ["source", "test"]) {
        if (!safe(feature[field])) violations.push(`${feature.id}: missing ${field} evidence`);
      }
    } else {
      if (!Array.isArray(feature.replacement) || !feature.replacement.length || !feature.replacement.every(safe)) {
        violations.push(`${feature.id}: completed feature needs replacement paths`);
      }
      if (!safe(feature.acceptance)) violations.push(`${feature.id}: completed feature needs acceptance evidence`);
      if (feature.status === "changed" && !safe(feature.decision)) violations.push(`${feature.id}: changed behavior needs a decision`);
    }
  }
  return violations;
}

export function check(root) {
  const baseline = JSON.parse(readFileSync(join(root, "docs/design/rewrite/main-inventory.json"), "utf8"));
  const ledger = JSON.parse(readFileSync(join(root, "docs/design/rewrite/feature-ledger.json"), "utf8"));
  if (ledger.baseline !== baseline.baseline) throw new Error("Ledger and inventory baselines differ");
  const files = sourceFiles(root).map((path) => [path, readFileSync(join(root, path), "utf8")]);
  const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--no-deps", "--locked", "--format-version=1"], {
    cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
  }));
  return { files: files.length, violations: [...checkSizes(files, baseline), ...checkEdges(metadata, baseline),
    ...checkIdentityFeatures(metadata), ...checkIdentityModelGraphs(root), ...checkIdentityTokenGraph(root),
    ...checkMigrations(root, baseline), ...checkLedger(root, ledger)] };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const root = resolve(fileURLToPath(new URL("../../", import.meta.url)));
    const result = check(root);
    for (const violation of result.violations) process.stderr.write(`${violation}\n`);
    process.stdout.write(`Rewrite policy: ${result.files} authored files; ${result.violations.length} violations\n`);
    process.exitCode = Number(result.violations.length > 0);
  } catch (error) { process.stderr.write(`${error.message}\n`); process.exitCode = 1; }
}
