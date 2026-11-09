// The full rehearsal: a real baseline release seeded through its own interfaces, its
// backup, the upgrade to the candidate, post-upgrade writes, the documented rollback,
// cross-version refusals and interrupted migrations on the seeded data.
import { cp, readFile, rename, rm } from "node:fs/promises";
import { join } from "node:path";
import { Instance } from "./instance.mjs";
import { Git } from "./gitops.mjs";
import { createAccounts, seed } from "./seed.mjs";
import { verifySeed } from "./verify.mjs";
import { probeWrites, writeAfterUpgrade } from "./writes.mjs";
import { countsPreserved, exists, inspectDatabase } from "./store.mjs";
import { expectRefusal, interruptedMigrations, killDelays, markNewer } from "./safety.mjs";

/** Failures of the previous release itself, reported without failing its steps. */
const BASELINE_KNOWN = {
  artifactDownload: "known defect of the previous release (its artifact download stream panics after its end; fixed in 0.2.0)",
};
/** After a restore everyone signs in again: sessions are never in a backup. */
const RESTORED_KNOWN = { ...BASELINE_KNOWN, session: "expected: sessions are not in backups" };
const ACTIONS_FILES = ["artifacts", "caches", "logs", "secrets.key"];

export async function full(report, { work, baseline, runner, candidate, target }) {
  const git = new Git(join(work, "git-home"));
  await git.init();
  const data = join(work, "data");
  const instance = await Instance.create(work, "instance", data);
  const db = join(data, "rbit.db");
  const ctx = { git, work, expected: null, app: null, baselineState: null, upgraded: null, written: null, migrateSeconds: 1 };

  await report.step("baseline: create accounts with `rbit user create`", () => createAccounts(instance, baseline));
  await report.step("baseline: seed through web forms, API, smart HTTP, LFS and Actions", async (step) => {
    await instance.start(baseline);
    try {
      ctx.expected = await seed({ instance, git, work, runner });
      ctx.app = { dir: join(work, "src", "alice-app") };
      if (!runner) step.notes.push("no baseline rbit-runner given: Actions history not seeded");
    } finally {
      await instance.stop();
    }
    ctx.baselineState = inspectDatabase(db);
    step.notes.push(`baseline database at migration ${ctx.baselineState.latest}; ${Object.values(ctx.baselineState.counts).reduce((a, b) => a + b, 0)} rows`);
  });
  if (!ctx.expected) return;
  await report.step("baseline: serves its own seed (control)", async () => {
    await instance.start(baseline);
    try { return await verifySeed(instance, ctx.expected, { git, work, label: "control", known: BASELINE_KNOWN }); } finally { await instance.stop(); }
  });
  const backup = join(work, "backup-pre-upgrade");
  await report.step("baseline: `rbit backup` before the upgrade (server stopped)", async (step) => {
    const result = await instance.run(baseline, ["backup", "--output", backup]);
    if (result.code !== 0) throw new Error(result.output);
    const manifest = JSON.parse(await readFile(join(backup, "manifest.json"), "utf8"));
    step.notes.push(`backup format ${manifest.format} by rbit ${manifest.rbit_version}`);
  });
  const pristine = join(work, "pre-upgrade-copy");
  await cp(data, pristine, { recursive: true });

  await report.step(`candidate: \`rbit migrate\` through ${target}`, async (step) => {
    const started = performance.now();
    const result = await instance.run(candidate, ["migrate"]);
    ctx.migrateSeconds = (performance.now() - started) / 1000;
    if (result.code !== 0) throw new Error(result.output);
    ctx.upgraded = inspectDatabase(db);
    step.notes.push(`migrated ${ctx.baselineState.latest} → ${ctx.upgraded.latest} in ${ctx.migrateSeconds.toFixed(2)} s`);
    if (ctx.upgraded.latest !== target) throw new Error(`stopped at ${ctx.upgraded.latest}`);
    const problems = [...ctx.upgraded.integrity, ...countsPreserved(ctx.baselineState.counts, ctx.upgraded.counts)];
    if (ctx.upgraded.foreignKeyViolations) problems.push(`${ctx.upgraded.foreignKeyViolations} foreign key violations`);
    if (problems.length) throw new Error(problems.join("; "));
  });
  await report.step("candidate: serves everything the baseline wrote", async () => {
    await instance.start(candidate);
    try { return await verifySeed(instance, ctx.expected, { git, work, label: "upgraded" }); } finally { await instance.stop(); }
  });
  await report.step("candidate: accepts new writes (issue, comment, push, token, wiki)", async () => {
    await instance.start(candidate);
    try {
      ctx.written = await writeAfterUpgrade(instance, ctx.expected, { git, app: ctx.app });
      return await probeWrites(instance, ctx.expected, ctx.written, { git, label: "written", present: true });
    } finally {
      await instance.stop();
    }
  });
  await report.step("candidate: backup format 6 round trip of the upgraded instance", async (step) => {
    const out = join(work, "backup-upgraded");
    const taken = await instance.run(candidate, ["backup", "--output", out]);
    if (taken.code !== 0) throw new Error(taken.output);
    const manifest = JSON.parse(await readFile(join(out, "manifest.json"), "utf8"));
    step.notes.push(`format ${manifest.format}: ${manifest.repositories} repositories, ${manifest.wikis} wiki(s) with ${manifest.wiki_objects} objects, ${manifest.attachments} release file(s), ${manifest.lfs_objects} LFS object(s), ${manifest.artifacts} artifact(s), ${manifest.logs} log file(s)`);
    if (manifest.format < 6 || !(manifest.wikis >= 1)) throw new Error("the backup holds no wiki objects");
    const copy = await Instance.create(work, "restored-upgraded", join(work, "data-restored-upgraded"));
    const restored = await copy.run(candidate, ["restore", "--from", out]);
    if (restored.code !== 0) throw new Error(restored.output);
    // The Actions secrets key is never in a backup (deployment guide): put it back.
    await cp(join(data, "actions", "secrets.key"), join(copy.dataDir, "actions", "secrets.key"));
    await copy.start(candidate);
    try {
      const seeded = await verifySeed(copy, ctx.expected, { git, work, label: "restored-upgraded", afterWrites: true, known: { session: "expected: sessions are not in backups" } });
      const written = await probeWrites(copy, ctx.expected, ctx.written, { git, label: "restored-writes", present: true });
      return { results: [...seeded.results, ...written.results] };
    } finally {
      await copy.stop();
    }
  });
  await report.step("baseline refuses the candidate's database", (step) =>
    expectRefusal(step, { work, name: "baseline-on-upgraded", dataDir: data, binary: baseline, pattern: /migration|missing|previously applied/i }));

  const upgradedCopy = join(work, "upgraded-copy");
  await report.step("candidate refuses a database migrated by a newer build", async (step) => {
    await cp(data, upgradedCopy, { recursive: true });
    markNewer(upgradedCopy);
    return expectRefusal(step, { work, name: "candidate-on-newer", dataDir: upgradedCopy, binary: candidate, pattern: /9999|newer|missing/i });
  });

  await report.step("rollback: move the upgraded directory aside, `rbit restore` the pre-upgrade backup", async (step) => {
    await rename(data, join(work, "data-upgraded"));
    const result = await instance.run(candidate, ["restore", "--from", backup]);
    step.notes.push(...result.output.split("\n").filter((line) => /WARN|warn|restore complete/.test(line)).map((line) => line.trim()));
    if (result.code !== 0) throw new Error(result.output);
    const restored = inspectDatabase(db);
    if (restored.latest !== ctx.baselineState.latest) throw new Error(`restored database is at ${restored.latest}`);
    // Runbook step 4: a format 4 backup (the previous release's) has no Actions files, and
    // no backup has the Actions secrets key; both are unchanged in the moved-aside directory.
    const copied = [];
    for (const name of ACTIONS_FILES) {
      const from = join(work, "data-upgraded", "actions", name);
      if (!(await exists(from)) || (await exists(join(data, "actions", name)))) continue;
      await cp(from, join(data, "actions", name), { recursive: true });
      copied.push(`actions/${name}`);
    }
    step.notes.push(`copied back from the upgraded directory: ${copied.join(", ") || "nothing"}`);
  });
  await report.step("rollback: the baseline serves the restored backup", async () => {
    await instance.start(baseline);
    try { return await verifySeed(instance, ctx.expected, { git, work, label: "rolled-back", known: RESTORED_KNOWN }); } finally { await instance.stop(); }
  });
  await report.step("rollback: writes made after the upgrade are lost (recovery point)", async () => {
    if (!ctx.written) throw new Error("nothing was written after the upgrade");
    await instance.start(baseline);
    try {
      const checks = await probeWrites(instance, ctx.expected, ctx.written, { git, label: "lost", present: false });
      return checks;
    } finally {
      await instance.stop();
    }
  });

  await report.step("interrupted upgrade: kill `rbit migrate` mid-way, then restart", (step) => {
    return interruptedMigrations(step, { work, source: pristine, binary: candidate, delays: killDelays(ctx.migrateSeconds), target });
  });
  await rm(upgradedCopy, { recursive: true, force: true });
}
