import assert from "node:assert/strict";
import test from "node:test";
import { checkI18nWebDependencies } from "../check-i18n-web-dependencies.mjs";

const root = "rbit-i18n-web v0.1.0 (/workspace/crates/rbit-i18n-web)";
const normalGraph = `${root}\nrbit-core v0.1.0\nrbit-actions-core v0.1.0\nrbit-messages v0.1.0\nrbit-plugin-api v0.1.0\nrbit-plugin-runtime v0.1.0\nrbit-plugin-web v0.1.0\naskama v0.16.0\ntokio v1.0.0\nserde v1.0.0\npercent-encoding v2.0.0\n`;
const output = stdout => ({ status: 0, stdout, stderr: "" });

test("language UI checks all-feature, all-target normal graphs, excluding dev and build edges", () => {
  let calls = 0;
  checkI18nWebDependencies((command, args, options) => {
    calls++;
    assert.equal(command, "cargo");
    assert.equal(args[0], "tree");
    assert.equal(args[args.indexOf("--package") + 1], "rbit-i18n-web");
    assert.ok(args.includes("--locked"));
    assert.ok(args.includes("--all-features"));
    assert.equal(args[args.indexOf("--target") + 1], "all");
    assert.equal(args[args.indexOf("--edges") + 1], "normal");
    assert.equal(args[args.indexOf("--prefix") + 1], "none");
    assert.ok(!args.includes("--workspace"));
    assert.equal(options.encoding, "utf8");
    // A dev-only Axum fixture must not contaminate the normal UI graph.
    return output(args[args.indexOf("--edges") + 1] === "normal" ? normalGraph : `${normalGraph}axum v0.8.0\n`);
  });
  assert.equal(calls, 1);
});

test("target-specific host backedges cannot hide behind the current machine's clean graph", () => {
  assert.throws(() => checkI18nWebDependencies((_, args) => {
    // An optional Windows-only HTTP adapter is absent from a Linux/macOS-only traversal.
    const allTargets = args[args.indexOf("--target") + 1] === "all";
    return output(allTargets ? `${normalGraph}rbit-web v0.1.0\n` : normalGraph);
  }), /rbit-web/);
});

test("indirect implementation dependencies are rejected even behind a permitted shared package", () => {
  for (const leaked of [
    "rbit-web", "rbit-services", "rbit-services-base", "rbit-storage", "rbit-git", "rbit-auth",
    "rbit-transport", "rbit-mail", "rbit-actions", "rbit-actions-web", "rbit-workflow", "serde-saphyr",
    "rbit-plugin-host", "rbit-plugin-host-native", "rbit-plugin-actions", "rbit-plugin-actions-native", "rbit-plugin-i18n",
    "rbit-plugin-packages", "rbit-plugin-packages-native",
    "rbit-i18n", "fluent-bundle", "fluent-syntax", "intl_pluralrules", "axum", "tower-http",
    "http", "http-body", "http-body-util", "hyper", "hyper-util", "hyper-rustls", "reqwest", "reqwest-middleware",
    "sqlx-core", "wasm-bindgen", "wasmtime", "wasmi", "wasmer", "wasmparser", "wat", "wast",
  ]) {
    // cargo tree --prefix none includes transitive nodes; a direct-manifest check misses this edge.
    const graph = `${normalGraph}${leaked} v1.0.0\n`;
    assert.throws(() => checkI18nWebDependencies(() => output(graph)), error => error.message.includes(leaked), leaked);
  }
});

test("missing, wrong, or empty roots cannot silently certify the boundary", () => {
  for (const graph of ["", "rbit-web v0.1.0\nrbit-i18n-web v0.1.0\n", "rbit-i18n-web-other v0.1.0\n", "error: missing package"]) {
    assert.throws(() => checkI18nWebDependencies(() => output(graph)), /expected rbit-i18n-web root/);
  }
});

test("Cargo failure or inability to start fails closed even when stdout looks valid", () => {
  assert.throws(() => checkI18nWebDependencies(() => ({ ...output(normalGraph), status: 101, stderr: "lock file needs updating" })), /lock file needs updating/);
  assert.throws(() => checkI18nWebDependencies(() => ({ ...output(normalGraph), error: new Error("cargo not found") })), /cargo not found/);
  assert.throws(() => checkI18nWebDependencies(() => ({ ...output(normalGraph), status: null })), /Unable to inspect/);
});
