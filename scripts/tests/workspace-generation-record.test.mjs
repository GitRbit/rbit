import assert from "node:assert/strict";
import test from "node:test";
import { readFile, readdir, rename, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { leased } from "../workspace/lease.mjs";
import { writeGenerationRecord } from "../workspace/retention/generation-record.mjs";
import { fixture, namespace } from "./retention-observation-fixture.mjs";

async function prepared(t, action) {
  const { ctx, lane } = await fixture(t);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const path = join(lane, "generation.json");
    const original = { schema: 1, source: "managed-candidate", namespace, id: "fixture", status: "allocated" };
    await writeGenerationRecord(ctx, path, original, lease, true);
    await action({ ctx, lane, lease, path, original });
  });
}

test("Windows replacement retries a bounded transient denial without unlinking the old record", async t => {
  await prepared(t, async ({ ctx, lane, lease, path, original }) => {
    let attempts = 0, waits = 0;
    await writeGenerationRecord(ctx, path, { ...original, status: "measuring" }, lease, false, {
      platform: "win32",
      wait: async () => { waits++; },
      replace: async (...args) => {
        if (++attempts === 1) throw Object.assign(new Error("temporary denial"), { code: "EPERM" });
        await rename(...args);
      },
    });
    assert.equal(attempts, 2);
    assert.equal(waits, 1);
    assert.equal(JSON.parse(await readFile(path, "utf8")).status, "measuring");
    assert.deepEqual(await readdir(lane), ["generation.json"]);
  });
});

test("persistent replacement denial preserves the old record after exactly three attempts", async t => {
  await prepared(t, async ({ ctx, lane, lease, path, original }) => {
    let attempts = 0;
    await assert.rejects(writeGenerationRecord(ctx, path, { ...original, status: "measuring" }, lease, false, {
      platform: "win32", wait: async () => {},
      replace: async () => { attempts++; throw Object.assign(new Error("persistent denial"), { code: "EPERM" }); },
    }), /persistent denial/);
    assert.equal(attempts, 3);
    assert.deepEqual(JSON.parse(await readFile(path, "utf8")), original);
    assert.deepEqual(await readdir(lane), ["generation.json"]);
  });
});

test("a record changed during retry is refused instead of replaced", async t => {
  await prepared(t, async ({ ctx, lease, path, original }) => {
    let attempts = 0;
    await assert.rejects(writeGenerationRecord(ctx, path, { ...original, status: "measuring" }, lease, false, {
      platform: "win32",
      wait: async () => { await writeFile(path, "private unrelated sentinel"); },
      replace: async () => { attempts++; throw Object.assign(new Error("temporary denial"), { code: "EPERM" }); },
    }), /UNRECOGNIZED/);
    assert.equal(attempts, 1);
    assert.equal(await readFile(path, "utf8"), "private unrelated sentinel");
  });
});
