// Model consumers must not acquire credential/runtime work through Cargo defaults.
import { execFileSync } from "node:child_process";

const consumers = {
  "rbit-credential-inventory": { normal: ["credentials"] },
  "rbit-collaboration": { normal: [] },
  "rbit-repositories": { normal: [] },
  "rbit-core": { normal: [] },
  "rbit-storage": { normal: [] },
  "rbit-auth": { normal: ["passkeys", "session-tokens", "two-factor"] },
  "rbit-services": { normal: ["access-tokens", "passkeys", "password-work", "sessions", "two-factor"] },
  "rbit-web": { dev: ["password-work"] },
};
const runtime = new Set(["argon2", "tokio", "tokio-util", "rbit-kernel", "zeroize", "base64", "sha2", "getrandom", "subtle"]);

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
  if (!packages.has(name) || !packages.has("rbit-identity-models")) return [`${name}: model dependency graph is missing its expected root or identity edge`];
  return [...packages].filter((pkg) => runtime.has(pkg)).map((pkg) => `${name}: model-only normal graph unexpectedly includes ${pkg}`);
}

/** Cargo tree is read-only: no compiler, build script, target directory or lock update. */
export function checkIdentityModelGraphs(root) {
  return ["rbit-identity-models", "rbit-identity", "rbit-core", "rbit-repositories"].flatMap((name) => {
    const tree = execFileSync("cargo", ["tree", "--package", name, "--no-default-features", "--edges", "normal", "--prefix", "none", "--locked"], {
      cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
    });
    return modelGraphViolations(name, tree);
  });
}

export function tokenGraphViolations(tree, feature = "session-tokens") {
  const packages = new Set(tree.trim().split(/\r?\n/).filter(Boolean).map(line => line.trim().split(/\s+/)[0]));
  const required = ["rbit-identity", "base64", "sha2", "getrandom"];
  if (required.some(name => !packages.has(name))) return [`identity ${feature}: incomplete normal dependency graph`];
  const forbidden = ["argon2", "tokio", "tokio-util", "rbit-kernel", "zeroize", "subtle"];
  return forbidden.filter(name => packages.has(name)).map(name => `identity ${feature}: crypto-only graph unexpectedly includes ${name}`);
}

/** Token operations, including the access token application, stay independent of
 * password algorithms and async runtimes. */
export function checkIdentityTokenGraph(root) {
  return ["session-tokens", "access-tokens"].flatMap(feature => {
    const tree = execFileSync("cargo", ["tree", "--package", "rbit-identity", "--no-default-features", "--features", feature,
      "--edges", "normal", "--prefix", "none", "--locked"], {
      cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
    });
    return tokenGraphViolations(tree, feature);
  });
}

export function sessionGraphViolations(tree) {
  const packages = new Set(tree.trim().split(/\r?\n/).filter(Boolean).map(line => line.trim().split(/\s+/)[0]));
  const required = ["rbit-identity", "base64", "sha2", "getrandom", "tokio", "tokio-util"];
  if (required.some(name => !packages.has(name))) return ["identity sessions: incomplete normal dependency graph"];
  return ["argon2", "rbit-kernel", "zeroize", "subtle"].filter(name => packages.has(name))
    .map(name => `identity sessions: password-independent graph unexpectedly includes ${name}`);
}

/** Session lifecycle may use async ownership without pulling in password work. */
export function checkIdentitySessionGraph(root) {
  const tree = execFileSync("cargo", ["tree", "--package", "rbit-identity", "--no-default-features", "--features", "sessions",
    "--edges", "normal", "--prefix", "none", "--locked"], {
    cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
  });
  return sessionGraphViolations(tree);
}

export function inventoryGraphViolations(tree) {
  const packages = new Set(tree.trim().split(/\r?\n/).filter(Boolean).map(line => line.trim().split(/\s+/)[0]));
  const required = ["rbit-credential-inventory", "rbit-identity", "argon2", "sqlx", "zeroize", "tokio"];
  if (required.some(name => !packages.has(name))) return ["credential inventory: incomplete normal dependency graph"];
  const forbidden = ["rbit-storage", "rbit-services", "rbit-core", "rbit-web", "rbit-server", "rbit-kernel"];
  return forbidden.filter(name => packages.has(name)).map(name => `credential inventory: application dependency unexpectedly includes ${name}`);
}

/** The offline tool owns no application store, startup or password-work owner.
 * This is the isolated selected-package graph, not a full-workspace feature-union
 * purity claim. A separate CI package test proves compilation in this scope.
 * Build-time graph inspection may fetch locked packages on a fresh CI host,
 * consistently with the other feature checks; this does not run the tool.
 */
export function checkInventoryGraph(root) {
  const tree = execFileSync("cargo", ["tree", "--package", "rbit-credential-inventory", "--edges", "normal", "--prefix", "none", "--locked"], {
    cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
  });
  return inventoryGraphViolations(tree);
}
