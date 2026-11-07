// Model consumers must never activate collaboration application features implicitly.
import { execFileSync } from "node:child_process";

const consumers = { "rbit-core": [], "rbit-notifications": [], "rbit-services": ["applications"], "rbit-storage": ["applications"] };
const forbidden = ["rbit-core", "rbit-storage", "rbit-services", "rbit-identity", "rbit-kernel", "tokio", "tokio-util", "argon2", "zeroize", "base64", "sha2", "getrandom", "subtle"];

export function checkCollaborationFeatures(metadata) {
  const violations = [];
  for (const pkg of metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id))) {
    for (const edge of pkg.dependencies) {
      if (edge.name !== "rbit-collaboration" || edge.kind) continue;
      const expected = consumers[pkg.name];
      if (!expected) continue;
      if (edge.uses_default_features !== false || [...edge.features].sort().join(",") !== expected.join(",")) {
        violations.push(`${pkg.name}: collaboration must disable defaults and request exactly [${expected.join(", ")}]`);
      }
    }
  }
  return violations;
}

export function collaborationGraphViolations(tree) {
  const packages = new Set(tree.trim().split(/\r?\n/).map(line => line.trim().split(/\s+/)[0]));
  if (!packages.has("rbit-collaboration") || !packages.has("rbit-contracts")) return ["collaboration: missing model graph evidence"];
  return forbidden.filter(name => packages.has(name)).map(name => `collaboration: runtime/adapter dependency ${name} in pure graph`);
}

export function checkCollaborationGraphs(root) {
  return [[], ["--features", "applications"]].flatMap(features => {
    const tree = execFileSync("cargo", ["tree", "--package", "rbit-collaboration", "--no-default-features", ...features,
      "--edges", "normal", "--prefix", "none", "--locked"], {
      cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
    });
    return collaborationGraphViolations(tree);
  });
}
