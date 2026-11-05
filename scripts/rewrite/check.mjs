/** Ratchet legacy file sizes and dependencies while enforcing strict bounds on new code. */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";

export const LIMIT = 400;
const GENERATED = new Set([
  "crates/rbit-web/assets/app.bundle.js", "crates/rbit-web/src/themes/catalog.rs",
]);
const FEATURE_EDGES = {
  "rbit-kernel": [], "rbit-contracts": [], "rbit-localization": [], "rbit-rewrite-check": [],
  "rbit-identity": ["rbit-contracts", "rbit-kernel"],
  "rbit-access": ["rbit-contracts"],
  "rbit-repositories": ["rbit-contracts", "rbit-access", "rbit-kernel"],
  "rbit-collaboration": ["rbit-contracts", "rbit-access", "rbit-repositories", "rbit-kernel"],
  "rbit-actions": ["rbit-contracts", "rbit-access", "rbit-workflow", "rbit-kernel"],
  "rbit-presentation": ["rbit-contracts", "rbit-localization"],
};

export function authored(path) {
  return /\.(rs|html|mjs|js)$/.test(path) && !GENERATED.has(path)
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
  for (const directory of ["crates", "scripts", "tools"]) {
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
    const allowed = FEATURE_EDGES[name] ?? legacy.get(name);
    if (!allowed) { violations.push(`${name}: declare an explicit architecture policy before adding a workspace crate`); continue; }
    for (const dependency of pkg.dependencies) {
      if (members.has(dependency.name) && !allowed.includes(dependency.name)) {
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

export function check(root) {
  const baseline = JSON.parse(readFileSync(join(root, "docs/design/rewrite/main-inventory.json"), "utf8"));
  const files = sourceFiles(root).map((path) => [path, readFileSync(join(root, path), "utf8")]);
  const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--no-deps", "--locked", "--offline", "--format-version=1"], {
    cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
  }));
  return { files: files.length, violations: [...checkSizes(files, baseline), ...checkEdges(metadata, baseline), ...checkMigrations(root, baseline)] };
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
