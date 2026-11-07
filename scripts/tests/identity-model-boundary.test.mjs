import assert from "node:assert/strict";
import test from "node:test";
import { modelEdgeViolations, unifiedModelViolations } from "../rewrite/identity-model-boundary.mjs";

function fixture() {
  const names = ["rbit-core", "rbit-storage", "rbit-repositories", "rbit-collaboration", "rbit-identity", "rbit-identity-models", "rbit-contracts", "argon2", "tokio"];
  const edge = name => ({ name, kind: null, uses_default_features: false, features: [] });
  const packages = names.map(name => ({ id: name, name, features: name === "rbit-identity-models" ? { default: [] } : {}, dependencies: names.indexOf(name) < 5 ? [edge("rbit-identity-models")] : [] }));
  const dep = (pkg, kind = null) => ({ pkg, dep_kinds: [{ kind, target: null }] });
  const nodes = names.map(id => ({ id, features: id === "rbit-identity" ? ["password-work", "sessions"] : id === "rbit-collaboration" ? ["applications"] : [],
    deps: names.indexOf(id) < 5 ? [dep("rbit-identity-models")] : id === "rbit-identity-models" ? [dep("rbit-contracts")] : [] }));
  nodes.find(n => n.id === "rbit-identity").deps.push(dep("argon2"), dep("tokio"));
  return { packages, workspace_members: names, resolve: { nodes } };
}

test("pure closure survives password/session/application features unified elsewhere", () => {
  const metadata = fixture();
  assert.deepEqual(modelEdgeViolations(metadata), []);
  assert.deepEqual(unifiedModelViolations(metadata), []);
  metadata.resolve.nodes.find(node => node.id === "rbit-core").deps.push({ pkg: "rbit-identity", dep_kinds: [{ kind: "dev" }] });
  assert.deepEqual(unifiedModelViolations(metadata), [], "dev compatibility fixtures are outside the normal production closure");
});

test("an apparently feature-free identity edge still fails under the real feature union", () => {
  const metadata = fixture();
  const core = metadata.resolve.nodes.find(node => node.id === "rbit-core");
  core.deps.push({ pkg: "rbit-identity", dep_kinds: [{ kind: null, target: "cfg(windows)" }] });
  const violations = unifiedModelViolations(metadata);
  assert(violations.some(line => line.includes("rbit-core -> rbit-identity -> argon2")));
  assert(violations.some(line => line.includes("rbit-core -> rbit-identity -> tokio")));
  metadata.packages.find(pkg => pkg.name === "rbit-core").dependencies.push({ name: "rbit-identity", kind: null, features: [], uses_default_features: false });
  assert.match(modelEdgeViolations(metadata)[0], /must not depend/);
});

test("proof rejects absent runtime-feature evidence, missing nodes and direct model leaks", () => {
  const metadata = fixture();
  metadata.resolve.nodes.find(node => node.id === "rbit-identity").features = [];
  assert.match(unifiedModelViolations(metadata)[0], /requires identity/);
  metadata.resolve.nodes.find(node => node.id === "rbit-identity").features = ["password-work", "sessions"];
  metadata.resolve.nodes.find(node => node.id === "rbit-identity-models").deps.push({ pkg: "tokio", dep_kinds: [{ kind: null }] });
  assert(unifiedModelViolations(metadata).some(line => line.includes("rbit-identity-models -> tokio")));
  metadata.resolve.nodes = metadata.resolve.nodes.filter(node => node.id !== "rbit-contracts");
  assert(unifiedModelViolations(metadata).some(line => line.includes("incomplete resolver evidence")));
});

test("consumer defaults, aliases, target edges and future model features cannot widen purity", () => {
  const metadata = fixture();
  const edge = metadata.packages.find(pkg => pkg.name === "rbit-storage").dependencies[0];
  edge.rename = "models"; edge.target = "cfg(unix)"; edge.uses_default_features = true;
  assert.match(modelEdgeViolations(metadata)[0], /defaults disabled/);
  edge.uses_default_features = false; edge.features = ["runtime"];
  assert.match(modelEdgeViolations(metadata)[0], /feature-free/);
  edge.features = [];
  metadata.packages.find(pkg => pkg.name === "rbit-identity-models").features.runtime = [];
  assert(modelEdgeViolations(metadata).some(line => line.includes("cannot expose optional application features")));
  metadata.packages.find(pkg => pkg.name === "rbit-identity-models").dependencies.push({ name: "unexpected-executor", kind: null });
  assert(modelEdgeViolations(metadata).some(line => line.includes("unexpected normal dependency")));
});
