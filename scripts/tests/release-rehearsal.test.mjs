// The release rehearsal's helpers, and the fast rehearsal itself when a candidate binary
// is named by RBIT_REHEARSAL_CANDIDATE (CI sets it after building rbit).
import assert from "node:assert/strict";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { attributeAfter, base32Decode, sshPublicKey, totp } from "../release-rehearsal/http.mjs";
import { configText } from "../release-rehearsal/instance.mjs";
import { lfsPointer } from "../release-rehearsal/gitops.mjs";
import { applyMigrations, countsPreserved, inspectDatabase, readMigrations, treeChanges } from "../release-rehearsal/store.mjs";
import { BASELINE_SCHEMA } from "../release-rehearsal/fast.mjs";
import { killDelays } from "../release-rehearsal/safety.mjs";
import { newestMigration, parseArgs, rehearse } from "../release-rehearsal.mjs";

const MIGRATIONS = fileURLToPath(new URL("../../crates/rbit-storage/migrations/", import.meta.url));

test("TOTP codes match the RFC 6238 SHA-1 vectors", () => {
  const secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
  assert.equal(base32Decode(secret).toString(), "12345678901234567890");
  assert.equal(totp(secret, Math.floor(59 / 30)), "287082");
  assert.equal(totp(secret, Math.floor(1111111109 / 30)), "081804");
  assert.equal(totp(secret, Math.floor(2000000000 / 30)), "279037");
});

test("arguments choose the mode and reject what they do not know", () => {
  assert.equal(parseArgs(["--candidate", "new"]).mode, "fast");
  assert.deepEqual(parseArgs(["--candidate", "new", "--baseline", "old", "--keep"]), { candidate: "new", baseline: "old", keep: true, mode: "full" });
  assert.throws(() => parseArgs([]), /--candidate/);
  assert.throws(() => parseArgs(["--candidate", "new", "--baseline-runner", "r"]), /needs --baseline/);
  assert.throws(() => parseArgs(["--candidate", "new", "--bogus"]), /unknown/);
});

test("the shared configuration names only paths and baseline keys", () => {
  const text = configText({ dataDir: "C:\\data\\rbit", port: 4321 });
  assert.match(text, /data_dir = "C:\/data\/rbit"/);
  assert.match(text, /bind = "127\.0\.0\.1:4321"/);
  assert.match(text, /\[actions\]\nenabled = true/);
});

test("page scraping, LFS pointers and SSH keys have the formats clients expect", () => {
  assert.equal(attributeAfter('<input id="new-token" class="x" value="rbit_abc&#x2F;d">', 'id="new-token"', "value"), "rbit_abc/d");
  assert.equal(attributeAfter("<p>nothing</p>", 'id="new-token"', "value"), null);
  assert.match(lfsPointer(Buffer.from("x")), /^version https:\/\/git-lfs\.github\.com\/spec\/v1\noid sha256:2d71.+\nsize 1\n$/);
  const [kind, blob, comment] = sshPublicKey("me@example.test").split(" ");
  assert.equal(kind, "ssh-ed25519");
  assert.equal(Buffer.from(blob, "base64").length, 51);
  assert.equal(comment, "me@example.test");
});

test("the synthetic baseline schema records sqlx's history and is inspectable", async () => {
  const dir = await mkdtemp(join(tmpdir(), "rehearsal-store-"));
  try {
    const migrations = await readMigrations(MIGRATIONS, 3);
    assert.deepEqual(migrations.map((m) => m.version), [1, 2, 3]);
    applyMigrations(join(dir, "rbit.db"), migrations);
    const state = inspectDatabase(join(dir, "rbit.db"));
    assert.equal(state.latest, 3);
    assert.deepEqual(state.failed, []);
    assert.deepEqual(state.integrity, []);
    assert.ok("accounts" in state.counts);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("the baseline schema is older than the candidate's and the comparisons are exact", async () => {
  assert.ok(BASELINE_SCHEMA < (await newestMigration()));
  assert.ok((await readdir(MIGRATIONS)).some((name) => name.startsWith(String(BASELINE_SCHEMA).padStart(4, "0"))));
  assert.deepEqual(countsPreserved({ a: 2, b: 1 }, { a: 2, b: 3, c: 1 }), []);
  assert.deepEqual(countsPreserved({ a: 2, b: 1 }, { a: 1 }), ["a: 2 → 1", "b: dropped"]);
  assert.deepEqual(treeChanges({ x: "1", y: "2" }, { x: "1", y: "3", z: "4" }), ["y", "z"]);
});

test("kill points cover the whole measured migration run, about every 5 ms", () => {
  const short = killDelays(0.05);
  assert.equal(short.length, 16);
  assert.deepEqual([short[0], short.at(-1)], [10, 90]);
  const slow = killDelays(0.3);
  assert.equal(slow.length, 60);
  assert.equal(slow.at(-1), 310);
  assert.ok(slow.every((delay, index) => index === 0 || delay > slow[index - 1]));
});

const candidate = process.env.RBIT_REHEARSAL_CANDIDATE;

test("fast rehearsal: the candidate upgrades, serves, restores, refuses and survives kills", { skip: !candidate && "set RBIT_REHEARSAL_CANDIDATE to an rbit binary", timeout: 900_000 }, async () => {
  const report = await rehearse(parseArgs(["--candidate", candidate]));
  const failed = report.steps.filter((step) => !step.ok).map((step) => `${step.name}: ${step.error ?? JSON.stringify(step.checks.filter((c) => !c.ok))}`);
  assert.deepEqual(failed, []);
});
