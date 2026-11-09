import assert from "node:assert/strict";
import test from "node:test";
import { checkCollaborationFeatures, collaborationGraphViolations } from "../rewrite/collaboration-features.mjs";

test("core model aliases cannot silently activate collaboration applications or defaults", () => {
  const edge = { name: "rbit-collaboration", kind: null, rename: "labels", target: "cfg(windows)", uses_default_features: false, features: [] };
  const pkg = { id: "core", name: "rbit-core", dependencies: [edge] };
  const metadata = { workspace_members: [pkg.id], packages: [pkg] };
  assert.deepEqual(checkCollaborationFeatures(metadata), []);
  edge.features = ["applications"];
  assert.match(checkCollaborationFeatures(metadata)[0], /exactly \[\]/);
  pkg.name = "rbit-services";
  assert.deepEqual(checkCollaborationFeatures(metadata), []);
  edge.uses_default_features = true;
  assert.match(checkCollaborationFeatures(metadata)[0], /disable defaults/);
  pkg.name = "rbit-storage";
  edge.uses_default_features = false;
  assert.deepEqual(checkCollaborationFeatures(metadata), []);
});

test("model and application graphs reject transitive runtime, crypto and legacy adapters", () => {
  const graph = "rbit-collaboration v0.1.5\nrbit-contracts v0.1.5\nrbit-identity-models v0.1.5\nserde v1.0.0\n";
  assert.deepEqual(collaborationGraphViolations(graph), []);
  assert.match(collaborationGraphViolations("")[0], /missing/);
  for (const name of ["rbit-core", "rbit-storage", "rbit-services", "rbit-identity", "rbit-kernel", "tokio", "tokio-util", "argon2", "zeroize", "base64", "sha2", "getrandom", "subtle"]) {
    assert.match(collaborationGraphViolations(`${graph}${name} v1.0.0\n`)[0], /runtime\/adapter dependency/);
  }
});

test("only the application graph may carry the Markdown engine", () => {
  const graph = "rbit-collaboration v0.1.5\nrbit-contracts v0.1.5\npulldown-cmark v0.13.6\n";
  assert.deepEqual(collaborationGraphViolations(graph), [], "applications");
  assert.match(collaborationGraphViolations(graph, ["pulldown-cmark"])[0], /pulldown-cmark in pure graph/);
});

test("notifications borrow collaboration models without activating application features", () => {
  const edge = { name: "rbit-collaboration", kind: null, rename: null, target: null, uses_default_features: false, features: [] };
  const pkg = { id: "notifications", name: "rbit-notifications", dependencies: [edge] };
  const metadata = { workspace_members: [pkg.id], packages: [pkg] };
  assert.deepEqual(checkCollaborationFeatures(metadata), []);
  edge.features = ["applications"];
  assert.match(checkCollaborationFeatures(metadata)[0], /exactly \[\]/);
  edge.features = [];
  edge.uses_default_features = true;
  assert.match(checkCollaborationFeatures(metadata)[0], /disable defaults/);
});
