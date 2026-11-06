import assert from "node:assert/strict";
import test from "node:test";
import { EventEmitter } from "node:events";
import { readFile } from "node:fs/promises";
import { ProductionFixture, parseFixtureReady } from "../../devtools/actions-visual/production-process.mjs";
import { productionBinary, productionOrigin, fixtureArguments } from "../../devtools/actions-visual/production-policy.mjs";

const token = "a".repeat(43);
class Child extends EventEmitter {
  stdout = new EventEmitter(); stderr = new EventEmitter(); kills = [];
  kill(signal) { this.kills.push(signal); queueMicrotask(() => this.emit("close", null, signal)); return true; }
}
function fake(command = async () => {}) {
  const child = new Child();
  const owner = new ProductionFixture((binary, args, options) => {
    assert.equal(binary, productionBinary); assert.deepEqual(args, [...fixtureArguments]);
    assert.equal(options.windowsHide, true); assert.equal(options.cwd, "/tmp");
    assert.deepEqual(options.stdio, ["ignore", "pipe", "pipe"]);
    assert.equal(options.env.RBIT_ACTIONS_VISUAL_FIXTURE, "isolated-linux-v1");
    assert.equal(Object.hasOwn(options.env, "GITHUB_TOKEN"), false);
    return child;
  }, command);
  return { child, owner, ready: () => child.stdout.emit("data", Buffer.from(`RBIT_ACTIONS_FIXTURE_READY ${JSON.stringify({ version: 1, cookie: token })}\n`)) };
}

test("private readiness protocol accepts only the fixed bounded ephemeral cookie shape", () => {
  assert.equal(parseFixtureReady("running 1 test"), null);
  assert.equal(parseFixtureReady(`RBIT_ACTIONS_FIXTURE_READY ${JSON.stringify({ version: 1, cookie: token })}`), token);
  for (const body of ["not JSON", JSON.stringify({ version: 1, cookie: token, path: "/private" }), JSON.stringify({ version: 1, cookie: "secret" })]) {
    assert.throws(() => parseFixtureReady(`RBIT_ACTIONS_FIXTURE_READY ${body}`), error => error.message === "FIXTURE_READY_REFUSED");
  }
});

test("cookies remain private and are attached only to the fixed loopback origin", async () => {
  const events = [], owned = fake(async value => { events.push(value); owned.child.emit("close", 0, null); });
  owned.ready(); await owned.owner.ready();
  await owned.owner.authenticate({ addCookies: async cookies => {
    assert.equal(cookies.length, 1); assert.equal(cookies[0].url, productionOrigin);
    assert.equal(cookies[0].name, "rbit_session"); assert.equal(cookies[0].value, token); assert.equal(cookies[0].httpOnly, true);
  } });
  assert.equal(JSON.stringify(owned.owner), "{}");
  await owned.owner.stop();
  assert.deepEqual(events, ["stop\n"]);
  assert.equal(owned.owner.status().closed, true);
});

test("the private frame starts a new line after serial libtest's unterminated test name", async () => {
  const source = await readFile(new URL("../../crates/rbit-web/tests/actions_pages/production_fixture.rs", import.meta.url), "utf8");
  assert.ok(source.includes('println!("\\nRBIT_ACTIONS_FIXTURE_READY {}"'));
  const owned = fake(async () => owned.child.emit("close", 0, null));
  const prefix = "\nrunning 1 test\ntest actions_pages::production_fixture::serve ... ";
  owned.child.stdout.emit("data", Buffer.from(prefix));
  // Split the actual newline-delimited frame across pipe chunks, including its prefix.
  owned.child.stdout.emit("data", Buffer.from("\nRBIT_ACTIONS_"));
  owned.child.stdout.emit("data", Buffer.from(`FIXTURE_READY ${JSON.stringify({ version: 1, cookie: token })}\n`));
  await owned.owner.ready();
  await owned.owner.authenticate({ addCookies: async cookies => assert.equal(cookies[0].value, token) });
  await owned.owner.stop();
});

test("stop acknowledgment is insufficient until the actual owned process closes", async () => {
  let settled = false;
  const owned = fake(); owned.ready(); await owned.owner.ready();
  const stopping = owned.owner.stop().then(() => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(settled, false);
  owned.child.emit("close", 0, null);
  await stopping;
  assert.equal(settled, true);
  assert.deepEqual(owned.child.kills, []);
});

test("malformed readiness and early exit fail without reflecting private child output", async () => {
  const owned = fake();
  owned.child.stdout.emit("data", Buffer.from("RBIT_ACTIONS_FIXTURE_READY private-data\n"));
  await assert.rejects(owned.owner.ready(), error => error.message === "FIXTURE_READY_REFUSED");
  await assert.rejects(owned.owner.stop());
  assert.equal(owned.owner.status().closed, true);
  assert.doesNotMatch(JSON.stringify(owned.owner.status()), /private-data/);
  const early = fake(); early.child.emit("close", 101, null);
  await assert.rejects(early.owner.ready(), /EXITED_BEFORE_READY/);
  await assert.rejects(early.owner.stop());
});
