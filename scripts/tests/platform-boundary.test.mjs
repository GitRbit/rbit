import assert from "node:assert/strict";
import test from "node:test";
import { inspectPlatformBoundary } from "../rewrite/platform-boundary.mjs";

const workspace = '[workspace.lints.rust]\nunsafe_code = "forbid"\nmissing_debug_implementations = "warn"\n[workspace.lints.clippy]\nall = "warn"\n';
const platform = '[lints.rust]\nunsafe_code = "deny"\nunsafe_op_in_unsafe_fn = "deny"\nmissing_debug_implementations = "warn"\n[lints.clippy]\nall = "warn"\n';
const base = () => ({ workspace, platform, sources: [
  ["crates/rbit-platform/src/windows/native.rs", "#![allow(unsafe_code)]\n"],
  ["crates/rbit-platform/src/windows/mod.rs", "mod native;\n"],
], manifests: [["crates/rbit-core/Cargo.toml", "[lints]\nworkspace = true\n"]] });

test("named platform exception retains all other lints and global forbid", () => {
  assert.deepEqual(inspectPlatformBoundary(base()), []);
  const fixture = base();
  fixture.workspace = workspace.replace('"forbid"', '"deny"');
  assert.match(inspectPlatformBoundary(fixture).join("\n"), /Workspace unsafe prohibition/);
});
test("an extra native exception or another crate opting out is refused", () => {
  const fixture = base();
  fixture.sources.push(["crates/rbit-platform/src/windows/extra.rs", "#![allow(unsafe_code)]\n"]);
  assert.match(inspectPlatformBoundary(fixture).join("\n"), /unreviewed unsafe/);
  fixture.manifests[0][1] = '[lints.rust]\nunsafe_code = "allow"\n';
  assert.match(inspectPlatformBoundary(fixture).join("\n"), /must inherit/);
});
test("public native exports, unsafe impl and weakened lint copies are refused", () => {
  const fixture = base();
  fixture.sources[1][1] = "pub mod native;\nunsafe impl Send for Example {}\n";
  fixture.platform = platform.replace('all = "warn"', 'all = "allow"');
  const result = inspectPlatformBoundary(fixture).join("\n");
  assert.match(result, /remain private/);
  assert.match(result, /trait implementations/);
  assert.match(result, /every workspace Clippy/);
});
test("removing or renaming the single exception fails closed", () => {
  const fixture = base();
  fixture.sources[0][0] = "crates/rbit-platform/src/windows/renamed.rs";
  assert.match(inspectPlatformBoundary(fixture).join("\n"), /Exactly one/);
});
test("cfg_attr cannot hide a second unsafe lint exception", () => {
  const fixture = base();
  fixture.sources.push(["crates/rbit-platform/src/extra.rs", "#![cfg_attr(windows, allow(unsafe_code))]\nunsafe fn extra_boundary() {}\n"]);
  assert.match(inspectPlatformBoundary(fixture).join("\n"), /conditional unsafe lint policy/);
  fixture.sources[2][1] = "#![cfg_attr(windows, cfg_attr(feature = \"extra\", allow(unsafe_code)))]\n";
  assert.match(inspectPlatformBoundary(fixture).join("\n"), /conditional unsafe lint policy/);
});
