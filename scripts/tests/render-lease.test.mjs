import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { preserveUncertainty } from "../render-compile/failure-priority.mjs";
import { measuredJob } from "../render-compile/run.mjs";
import { measurementEnv } from "../render-compile/options.mjs";
import { leased, acquire, UncertainProcessTreeError } from "../workspace/lease.mjs";

const source = { fingerprint: "fixed" };
const config = { command: "check", profile: "dev", jobs: 1, memory: "none" };
const job = { mode: "blocks", count: 8 };

async function fixture(t, { clean = false, observer = false, provenance = false, report = false, fallback = false, warmup = false } = {}) {
  const root = await mkdtemp(join(tmpdir(), "render-lease-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const path = join(root, ".lease");
  const uncertain = new UncertainProcessTreeError("Fake compiler termination");
  const observerReport = { reasons: ["observer-close-unproven"] };
  let sources = 0, launches = 0, finishes = 0, reports = 0;
  const dependencies = {
    // Exercise the production lease implementation, while never spawning Cargo.
    cargo: async (ctx, _command, args, options) => leased(path, () => options.execute(ctx, args, {
      ...options.env,
      CARGO_TARGET_DIR: join(root, "shared", "target"), CARGO_BUILD_BUILD_DIR: join(root, "shared", "intermediates"),
    }, () => {})),
    cargoProcess: async (_ctx, args) => {
      launches++;
      if (clean || args[0] !== "clean") throw uncertain;
    },
    sourceIdentity: async () => {
      if (++sources > 1 && provenance) throw new Error("Private source path");
      return source;
    },
    sampler: async () => ({
      attach() {},
      finish: async () => {
        finishes++;
        if (observer) throw Object.assign(new Error("Private observer command"), observer === "without-report" ? {} : { observerReport });
        return { reasons: [] };
      },
    }),
  };
  const write = async (record) => {
    reports++;
    if ((report && record.reason !== "helper-or-preparation-failed") || (fallback && record.reason === "helper-or-preparation-failed")) throw new Error("Private report path");
  };
  await assert.rejects(measuredJob({}, config, job, source, {}, write, measurementEnv({}), new Map(), warmup ? "warmup" : "sample", dependencies), (error) => {
    assert.equal(error, uncertain, "original uncertainty must reach leased()");
    assert.equal(error.retainControl, true);
    assert.ok(!JSON.stringify(error.secondaryFailures ?? []).includes("Private"));
    if (observer && observer !== "without-report") assert.deepEqual(error.observerReport, observerReport);
    return true;
  });
  const blocked = JSON.parse(await readFile(join(path, "blocked.json"), "utf8"));
  assert.equal(blocked.reason, uncertain.message);
  await assert.rejects(acquire(path, { timeout: 1 }), /Blocked lease/);
  assert.equal(launches, clean || warmup ? 1 : 2);
  assert.equal(finishes, clean ? 0 : 1);
  assert.ok(reports > 0);
}

for (const [name, options] of [
  ["observer failure", { observer: true }],
  ["observer failure without a report", { observer: "without-report", fallback: true }],
  ["observer and both report failures", { observer: true, report: true, fallback: true }],
  ["post-compile provenance and fallback failure", { provenance: true, fallback: true }],
  ["timed record and fallback failure", { report: true, fallback: true }],
  ["package-clean record and fallback failure", { clean: true, report: true, fallback: true }],
  ["package-clean failure with a writable report", { clean: true }],
  ["warmup observer and reporting failures", { warmup: true, observer: true, report: true, fallback: true }],
]) test(`uncertain compiler retains actual lease across ${name}`, (t) => fixture(t, options));


test("ordinary report failures preserve normal lease release", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "render-release-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const path = join(root, ".lease");
  const primary = new Error("Cargo exited 101");
  const secondary = new Error("Report failed");
  await assert.rejects(leased(path, () => preserveUncertainty(primary, "report", () => { throw secondary; })), (error) => error === secondary);
  const release = await acquire(path, { timeout: 1 });
  await release();
  assert.equal(primary.retainControl, undefined);
});
