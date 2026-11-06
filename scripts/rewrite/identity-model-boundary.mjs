// Prove model purity in the actual resolver graph with application features enabled.
import { execFileSync } from "node:child_process";

const consumers = new Set(["rbit-core", "rbit-storage", "rbit-repositories", "rbit-collaboration", "rbit-identity"]);
const roots = ["rbit-identity-models", "rbit-core", "rbit-repositories", "rbit-collaboration"];
const forbidden = new Set(["rbit-identity", "rbit-kernel", "rbit-storage", "rbit-services", "argon2", "tokio", "tokio-util", "zeroize", "base64", "sha2", "getrandom", "subtle", "sqlx", "redb", "axum"]);

export function modelEdgeViolations(metadata) {
  const violations = [];
  for (const pkg of metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id))) {
    if (!consumers.has(pkg.name)) continue;
    const normal = pkg.dependencies.filter(edge => !edge.kind);
    const model = normal.filter(edge => edge.name === "rbit-identity-models");
    if (model.length !== 1 || model.some(edge => edge.uses_default_features !== false || edge.features.length)) {
      violations.push(`${pkg.name}: requires one explicit feature-free identity-models dependency with defaults disabled`);
    }
    if (pkg.name !== "rbit-identity" && normal.some(edge => edge.name === "rbit-identity")) {
      violations.push(`${pkg.name}: model consumers must not depend on the feature-unified identity application`);
    }
  }
  const models = metadata.packages.find(pkg => pkg.name === "rbit-identity-models");
  if (!models || Object.keys(models.features).some(feature => feature !== "default") || (models.features.default ?? []).length) {
    violations.push("identity-models: model package cannot expose optional application features");
  }
  for (const edge of models?.dependencies ?? []) {
    if (!edge.kind && !["rbit-contracts", "serde"].includes(edge.name)) {
      violations.push(`identity-models: unexpected normal dependency ${edge.name}; models only use contracts and serde`);
    }
  }
  return violations;
}

export function unifiedModelViolations(metadata) {
  const names = new Map(metadata.packages.map(pkg => [pkg.id, pkg.name]));
  const nodes = new Map((metadata.resolve?.nodes ?? []).map(node => [node.id, node]));
  const workspace = new Set(metadata.workspace_members ?? []);
  const find = name => [...nodes.values()].find(node => workspace.has(node.id) && names.get(node.id) === name);
  const identity = find("rbit-identity"), collaboration = find("rbit-collaboration");
  if (!identity?.features.includes("password-work") || !identity.features.includes("sessions") || !collaboration?.features.includes("applications")) {
    return ["unified model proof requires identity password-work/sessions and collaboration applications enabled"];
  }
  const violations = [];
  for (const name of roots) {
    const root = find(name);
    if (!root) { violations.push(`${name}: missing unified resolver node`); continue; }
    const queue = [[root.id, [name]]], seen = new Set();
    for (let cursor = 0; cursor < queue.length; cursor++) {
      const [id, path] = queue[cursor];
      if (seen.has(id)) continue;
      seen.add(id);
      if (forbidden.has(names.get(id))) violations.push(`${name}: unified normal graph leaks ${path.join(" -> ")}`);
      const node = nodes.get(id);
      if (!node) { violations.push(`${name}: incomplete resolver evidence for ${path.join(" -> ")}`); continue; }
      for (const edge of node.deps) {
        if (edge.dep_kinds.some(kind => kind.kind === null)) queue.push([edge.pkg, [...path, names.get(edge.pkg) ?? "unknown"]]);
      }
    }
    if (![...seen].some(id => names.get(id) === "rbit-identity-models")) violations.push(`${name}: missing identity-models closure`);
  }
  return violations;
}

export function checkUnifiedModelBoundary(root, manifests) {
  const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--format-version=1", "--features",
    "rbit-identity/password-work,rbit-identity/sessions,rbit-collaboration/applications"], {
    cwd: root, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 16 * 1024 * 1024,
  }));
  return [...modelEdgeViolations(manifests), ...unifiedModelViolations(metadata)];
}
