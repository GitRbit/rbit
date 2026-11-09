import assert from "node:assert/strict";
import test from "node:test";
import { checkIdentityFeatures, modelGraphViolations, tokenGraphViolations, sessionGraphViolations, inventoryGraphViolations } from "../rewrite/identity-features.mjs";

test("renamed and target-specific identity edges cannot silently enable crypto defaults", () => {
  const dependency = { name: "rbit-identity", rename: "models", target: "cfg(windows)", kind: null, uses_default_features: false, features: [] };
  const pkg = { id: "core", name: "rbit-core", dependencies: [dependency] };
  const metadata = { packages: [pkg], workspace_members: [pkg.id] };
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  dependency.uses_default_features = true;
  assert.match(checkIdentityFeatures(metadata)[0], /disable defaults/);
  dependency.uses_default_features = false;
  dependency.features = ["credentials"];
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[\]/);
  pkg.name = "rbit-storage";
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[\]/);
  dependency.features = [];
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  pkg.name = "rbit-auth";
  dependency.features = ["credentials", "session-tokens"];
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[session-tokens\]/, "the retired password facade cannot return");
  dependency.features = ["session-tokens"];
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  pkg.name = "rbit-repositories";
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[\]/);
  dependency.features = [];
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  pkg.name = "rbit-collaboration";
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  dependency.features = ["sessions"];
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[\]/);
  dependency.features = [];
  dependency.uses_default_features = true;
  assert.match(checkIdentityFeatures(metadata)[0], /disable defaults/);
  dependency.uses_default_features = false;
  pkg.name = "rbit-auth";
  dependency.features = ["password-work"];
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[session-tokens\]/);
  pkg.name = "rbit-web"; dependency.kind = "dev";
  assert.deepEqual(checkIdentityFeatures(metadata), []);
});

test("isolated normal graphs reject transitive runtime work and missing evidence", () => {
  const core = "rbit-core v0.1.5 (private/source)\nrbit-identity-models v0.1.5\nrbit-contracts v0.1.5\nserde v1.0.0\n";
  assert.deepEqual(modelGraphViolations("rbit-core", core), []);
  for (const dependency of ["argon2", "tokio", "tokio-util", "rbit-kernel", "zeroize", "base64", "sha2", "getrandom", "subtle"]) {
    assert.match(modelGraphViolations("rbit-core", `${core}${dependency} v1.0.0 (*)\n`)[0], /unexpectedly includes/);
  }
  assert.match(modelGraphViolations("rbit-core", "")[0], /missing/);
  assert.match(modelGraphViolations("rbit-core", "rbit-core v0.1.5\n")[0], /missing/);
});

test("token-only graphs require their crypto edges and exclude password or runtime work", () => {
  const graph = "rbit-identity v0.1.5\nbase64 v0.23.1\nsha2 v0.11.1\ngetrandom v0.4.3\n";
  assert.deepEqual(tokenGraphViolations(graph), []);
  for (const dependency of ["argon2", "tokio", "tokio-util", "rbit-kernel", "zeroize", "subtle"]) {
    assert.match(tokenGraphViolations(`${graph}${dependency} v1.0.0 (*)\n`)[0], /unexpectedly includes/);
  }
  for (const dependency of ["rbit-identity", "base64", "sha2", "getrandom"]) {
    assert.match(tokenGraphViolations(graph.split("\n").filter(line => !line.startsWith(`${dependency} `)).join("\n"))[0], /incomplete/);
  }
});

test("session application capability is explicit and cannot widen model consumers", () => {
  const dependency = { name: "rbit-identity", kind: null, uses_default_features: false, features: ["password-work", "sessions"] };
  const pkg = { id: "services", name: "rbit-services", dependencies: [dependency] };
  const metadata = { packages: [pkg], workspace_members: [pkg.id] };
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  dependency.features = ["password-work"];
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[password-work, sessions\]/);
  dependency.features = ["password-work", "sessions"];
  pkg.name = "rbit-storage";
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[\]/);
});

test("session-only graphs retain async ownership without password work", () => {
  const graph = "rbit-identity v0.1.5\nbase64 v0.23.1\nsha2 v0.11.1\ngetrandom v0.4.3\ntokio v1.0.0\ntokio-util v0.7.0\n";
  assert.deepEqual(sessionGraphViolations(graph), []);
  for (const dependency of ["argon2", "rbit-kernel", "zeroize", "subtle"]) {
    assert.match(sessionGraphViolations(`${graph}${dependency} v1.0.0 (*)\n`)[0], /unexpectedly includes/);
  }
  assert.match(sessionGraphViolations(graph.replace("tokio-util v0.7.0\n", ""))[0], /incomplete/);
});

test("offline inventory requests only explicit credentials without widening storage models", () => {
  const dependency = { name: "rbit-identity", kind: null, uses_default_features: false, features: ["credentials"] };
  const pkg = { id: "inventory", name: "rbit-credential-inventory", dependencies: [dependency] };
  const metadata = { packages: [pkg], workspace_members: [pkg.id] };
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  for (const features of [[], ["password-work"], ["credentials", "sessions"]]) {
    dependency.features = features;
    assert.match(checkIdentityFeatures(metadata)[0], /exactly \[credentials\]/);
  }
  dependency.features = ["credentials"]; dependency.uses_default_features = true;
  assert.match(checkIdentityFeatures(metadata)[0], /disable defaults/);
  dependency.uses_default_features = false; pkg.name = "rbit-storage";
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[\]/);
});

test("inventory normal graph proves metadata parsing but excludes application initialization", () => {
  const graph = "rbit-credential-inventory v0.1.5\nrbit-identity v0.1.5\nargon2 v0.6.0\nsqlx v0.9.0\nzeroize v1.8.2\ntokio v1.0.0\n";
  assert.deepEqual(inventoryGraphViolations(graph), []);
  for (const name of ["rbit-storage", "rbit-services", "rbit-core", "rbit-web", "rbit-server", "rbit-kernel"]) {
    assert.match(inventoryGraphViolations(`${graph}${name} v0.1.5\n`)[0], /unexpectedly includes/);
  }
  assert.match(inventoryGraphViolations(graph.replace("argon2 v0.6.0\n", ""))[0], /incomplete/);
});
