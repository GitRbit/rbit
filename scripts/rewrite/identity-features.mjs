// Model consumers must not acquire credential/runtime work through Cargo defaults.
import { execFileSync } from "node:child_process";

const consumers = {
  "rbit-core": { normal: [] },
  "rbit-storage": { normal: [] },
  "rbit-auth": { normal: ["credentials"] },
  "rbit-services": { normal: ["password-work"] },
  "rbit-web": { dev: ["password-work"] },
};
const runtime = new Set(["argon2", "tokio", "tokio-util", "rbit-kernel", "zeroize"]);

export function checkIdentityFeatures(metadata) {
  const violations = [];
  for (const pkg of metadata.packages.filter((pkg) => metadata.workspace_members.includes(pkg.id))) {
    for (const dependency of pkg.dependencies) {
      if (dependency.name !== "rbit-identity") continue;
      const expected = consumers[pkg.name]?.[dependency.kind ?? "normal"];
      if (!expected) continue; // The independent dependency-edge policy governs other consumers.
      if (dependency.uses_default_features !== false || [...(dependency.features ?? [])].sort().join(",") !== expected.join(",")) {
        violations.push(`${pkg.name}: identity must disable defaults and request exactly [${expected.join(", ")}] for ${dependency.kind ?? "normal"} use`);
      }
    }
  }
  return violations;
}

export function modelGraphViolations(name, tree) {
  const packages = new Set(tree.trim().split(/\r?\n/).filter(Boolean).map((line) => line.trim().split(/\s+/)[0]));
  if (!packages.has(name) || !packages.has("rbit-identity")) return [`${name}: model dependency graph is missing its expected root or identity edge`];
  return [...packages].filter((pkg) => runtime.has(pkg)).map((pkg) => `${name}: model-only normal graph unexpectedly includes ${pkg}`);
}

/** Cargo tree is read-only: no compiler, build script, target directory or lock update. */
export function checkIdentityModelGraphs(root) {
  return ["rbit-identity", "rbit-core"].flatMap((name) => {
    const tree = execFileSync("cargo", ["tree", "--package", name, "--no-default-features", "--edges", "normal", "--prefix", "none", "--locked"], {
      cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
    });
    return modelGraphViolations(name, tree);
  });
}
