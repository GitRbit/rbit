import assert from "node:assert/strict";
import test from "node:test";
import { checkIdentityFeatures, modelGraphViolations } from "../rewrite/identity-features.mjs";

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
  dependency.features = ["credentials"];
  assert.deepEqual(checkIdentityFeatures(metadata), []);
  dependency.features = ["password-work"];
  assert.match(checkIdentityFeatures(metadata)[0], /exactly \[credentials\]/);
  pkg.name = "rbit-web"; dependency.kind = "dev";
  assert.deepEqual(checkIdentityFeatures(metadata), []);
});

test("isolated normal graphs reject transitive runtime work and missing evidence", () => {
  const core = "rbit-core v0.1.5 (private/source)\nrbit-identity v0.1.5\nrbit-contracts v0.1.5\nserde v1.0.0\n";
  assert.deepEqual(modelGraphViolations("rbit-core", core), []);
  for (const dependency of ["argon2", "tokio", "tokio-util", "rbit-kernel", "zeroize"]) {
    assert.match(modelGraphViolations("rbit-core", `${core}${dependency} v1.0.0 (*)\n`)[0], /unexpectedly includes/);
  }
  assert.match(modelGraphViolations("rbit-core", "")[0], /missing/);
  assert.match(modelGraphViolations("rbit-core", "rbit-core v0.1.5\n")[0], /missing/);
});
