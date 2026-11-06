import assert from "node:assert/strict";
import test from "node:test";
import { normalizeGraph } from "../workspace/retention/graph.mjs";
const policy = { compatibility: "a".repeat(20), manifestDigest: "b".repeat(64), configurationDigest: "c".repeat(64) };
const graph = (args, command = "test", extra = {}) => normalizeGraph({ command, args, policy, ...extra });

test("equivalent package/feature order, duplicates and test harness filters do not rotate graphs", () => {
  const expected = graph(["-p", "core", "-p", "web", "--features", "html,search", "--no-default-features", "--test", "integration"]);
  for (const args of [
    ["--features=search html", "--package=web", "-pcore", "-pcore", "--no-default-features", "--test=integration", "filter_one", "--", "--nocapture", "--test-threads=1"],
    ["--test", "integration", "-pweb", "-p", "core", "-Fhtml", "-Fsearch", "--no-default-features", "--locked", "--color=never", "-j1", "other_filter"],
  ]) {
    const actual = graph(args);
    assert.equal(actual.confidence, "known");
    assert.equal(actual.id, expected.id);
  }
});

test("material selection dimensions cannot collapse to one scheduling graph", () => {
  const baseline = graph([]);
  for (const [command, args] of [
    ["build", []], ["check", []], ["clippy", []], ["test", ["--workspace"]],
    ["test", ["--workspace", "--exclude", "web"]], ["test", ["-pweb"]],
    ["test", ["--all-features"]], ["test", ["--no-default-features"]],
    ["test", ["--features", "html"]], ["test", ["--release"]],
    ["test", ["--profile", "custom"]], ["test", ["--target", "wasm32-unknown-unknown"]],
    ["test", ["--lib"]], ["test", ["--all-targets"]], ["test", ["--test", "integration"]],
  ]) {
    const actual = graph(args, command);
    assert.equal(actual.confidence, "known", `${command} ${args}`);
    assert.notEqual(actual.id, baseline.id, `${command} ${args}`);
  }
  assert.equal(graph(["--all"]).id, graph(["--workspace"]).id);
  assert.equal(graph([], "build").id, graph(["--profile=dev"], "build").id);
  assert.equal(graph(["--all-targets", "--lib"]).id, graph(["--all-targets"]).id);
  assert.equal(graph(["--all-features", "--no-default-features"]).id, graph(["--all-features"]).id);
});

test("unknown switches and compiler arguments remain conservative, not guessed harmless", () => {
  for (const [command, args] of [
    ["build", ["--future-artifact-switch"]], ["test", ["--config", "build.target=other"]],
    ["test", ["--exclude", "web"]], ["build", ["--", "-Cincremental=/outside"]],
    ["clippy", ["--", "-Copt-level=3"]], ["test", ["--profile", "release", "--release"]],
    ["test", ["--features"]], ["test", ["--all-features=false"]],
    ["test", ["--workspace", "-pweb"]], ["build", ["--jobs", "--target", "other"]],
  ]) assert.equal(graph(args, command).confidence, "conservative", `${command} ${args}`);
  assert.equal(graph(["--", "-D", "warnings", "-Aclippy::style"], "clippy").confidence, "known");
  assert.throws(() => graph(["nul\0argument"]), /NUL/);
});

test("root policy and profile environment affect identity without entry-order instability", () => {
  const first = graph([], "build", { env: { CARGO_PROFILE_DEV_DEBUG: "0", CARGO_INCREMENTAL: "0" } });
  const second = graph([], "build", { policy: { configurationDigest: policy.configurationDigest,
    manifestDigest: policy.manifestDigest, compatibility: policy.compatibility },
    env: { CARGO_INCREMENTAL: "0", CARGO_PROFILE_DEV_DEBUG: "0" } });
  assert.equal(first.id, second.id);
  for (const extra of [
    { env: { CARGO_PROFILE_DEV_DEBUG: "1", CARGO_INCREMENTAL: "0" } },
    { policy: { ...policy, manifestDigest: "d".repeat(64) } },
    { policy: { ...policy, configurationDigest: "e".repeat(64) } },
    { policy: { ...policy, compatibility: "f".repeat(20) } },
  ]) assert.notEqual(first.id, graph([], "build", extra).id);
  assert.equal(graph([], "build", { env: { PRIVATE_TOKEN: "ignored" } }).id, graph([], "build").id);
  assert.equal(graph([], "build", { policy: undefined }).confidence, "conservative");
  assert.notEqual(graph([], "build", { env: { CARGO_BUILD_TARGET: "target-one" } }).id,
    graph([], "build", { env: { CARGO_BUILD_TARGET: "target-two" } }).id);
});

test("nextest runner profile/filter changes do not masquerade as Cargo profile changes", () => {
  const first = graph(["run", "-pweb", "--profile", "ci", "-E", "test(one)", "--retries", "2"], "nextest");
  const second = graph(["list", "-pweb", "--profile", "default", "test_two", "--", "--ignored"], "nextest");
  assert.equal(first.confidence, "known");
  assert.equal(first.id, second.id);
  assert.notEqual(first.id, graph(["run", "-pweb", "--cargo-profile", "release"], "nextest").id);
});
