// The fast rehearsal: no baseline binary needed. A store at the baseline schema is built
// from the immutable migration files with sqlx's own bookkeeping, filled with the
// storage upgrade fixture and real bare repositories, then the candidate upgrades it,
// serves it, backs it up and restores it, refuses a newer database and survives kills.
import { cp, mkdir, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { Instance } from "./instance.mjs";
import { Git, authenticatedUrl } from "./gitops.mjs";
import { Client, attributeAfter, expect } from "./http.mjs";
import { signIn } from "./seed.mjs";
import { probeWiki, seedWiki } from "./wiki.mjs";
import { Checks } from "./verify.mjs";
import { applyMigrations, countsPreserved, inspectDatabase, readMigrations } from "./store.mjs";
import { expectRefusal, interruptedMigrations, killDelays, markNewer } from "./safety.mjs";

/** The last migration of the pinned baseline (main 39f932a0). */
export const BASELINE_SCHEMA = 59;
const FIXTURE_SCHEMA = 8;
const ADMIN = { name: "rehearsal", password: "rehearsal-admin-password" };

/** Builds the synthetic baseline data directory; returns the refs of its repositories. */
export async function syntheticBaseline(root, dataDir, git) {
  const migrations = await readMigrations(join(root, "crates/rbit-storage/migrations"), BASELINE_SCHEMA);
  const fixture = await readFile(join(root, "crates/rbit-storage/tests/fixtures/milestone1.sql"), "utf8");
  await mkdir(dataDir, { recursive: true });
  applyMigrations(join(dataDir, "rbit.db"), migrations, { after: (db, version) => version === FIXTURE_SCHEMA && db.exec(fixture) });
  const db = new DatabaseSync(join(dataDir, "rbit.db"), { readOnly: true });
  const repositories = db.prepare("SELECT r.id, r.owner_id, r.name, a.name AS owner, r.default_branch FROM repositories r JOIN accounts a ON a.id = r.owner_id").all();
  db.close();
  const refs = {};
  const source = await git.sample(join(dataDir, "..", "synthetic-source"), "synthetic");
  for (const repo of repositories) {
    const bare = join(dataDir, "repositories", String(repo.owner_id), `${repo.id}.git`);
    await mkdir(bare, { recursive: true });
    git.run(bare, ["init", "--quiet", "--bare", `--initial-branch=${repo.default_branch}`]);
    if (repo.name === "empty") continue;
    git.run(source, ["push", "--quiet", bare, `main:refs/heads/${repo.default_branch}`, "feature", "v1.0"]);
    refs[`${repo.owner}/${repo.name}`] = { refs: git.refs(bare), private: repo.name === "rocket" };
  }
  return refs;
}

async function verifySynthetic(instance, repos, { git, work, label, wiki }) {
  const checks = new Checks(label);
  const anonymous = new Client(instance.base);
  await checks.check("health endpoints answer", async () => {
    for (const path of ["/healthz", "/readyz"]) expect((await anonymous.get(path)).status === 200, `${path} is not healthy`);
  });
  let token;
  await checks.check("an administrator created after the upgrade signs in and gets a token", async () => {
    const admin = await signIn(instance.base, ADMIN.name, undefined, ADMIN.password);
    const page = await admin.form("/settings/tokens", { name: label, scope: "admin", expires: "never" });
    token = attributeAfter(page.text, 'id="new-token"', "value");
    expect(token, "a token is shown", page);
  });
  for (const [name, repo] of Object.entries(repos)) {
    const [owner, project] = name.split("/");
    await checks.check(`${name}: anonymous visibility (${repo.private ? "hidden" : "public"})`, async () => {
      const page = await anonymous.get(`/${name}`);
      expect(page.status === (repo.private ? 404 : 200), `GET /${name}`, page);
    });
    if (repo.private) continue;
    await checks.check(`${name}: refs, clone with git fsck, archive`, async () => {
      const url = authenticatedUrl(instance.base, owner, project);
      const refs = git.refs(url);
      for (const [ref, id] of Object.entries(repo.refs)) expect(refs[ref] === id, `${ref} is ${refs[ref]}, expected ${id}`);
      git.cloneAndCheck(url, join(work, "clones", `${label}-${owner}-${project}`));
      const archive = await anonymous.get(`/${name}/archive/v1.0.tar.gz`);
      expect(archive.status === 200 && archive.bytes[0] === 0x1f, "the archive downloads", archive);
    });
  }
  const admin = await signIn(instance.base, ADMIN.name, undefined, ADMIN.password);
  if (wiki.pages) {
    // A restored copy: the wiki written before the backup came back with it (format 6).
    await probeWiki(checks, admin, wiki.pages, true);
    return checks;
  }
  await checks.check("a new repository accepts a push and wiki pages (kept, renamed, deleted)", async () => {
    const name = `pushed-${label}`;
    const created = await admin.form("/new", { owner: ADMIN.name, name, description: "", visibility: "public" });
    expect(created.status === 303, "the repository is created", created);
    const dir = await git.sample(join(work, "src", name), name);
    git.run(dir, ["push", "--quiet", authenticatedUrl(instance.base, ADMIN.name, name, ADMIN.name, token), "main"]);
    wiki.pages = await seedWiki(admin, `${ADMIN.name}/${name}`);
  });
  if (wiki.pages) await probeWiki(checks, admin, wiki.pages, true);
  return checks;
}

export async function fast(report, { root, work, candidate, target }) {
  const git = new Git(join(work, "git-home"));
  await git.init();
  const data = join(work, "data");
  const pristine = join(work, "pre-upgrade-copy");
  let repos;
  const wiki = {};
  let before;
  let migrateSeconds = 0.3;
  await report.step(`synthetic baseline store at migration ${BASELINE_SCHEMA} with fixture rows and repositories`, async (step) => {
    repos = await syntheticBaseline(root, data, git);
    before = inspectDatabase(join(data, "rbit.db"));
    await cp(data, pristine, { recursive: true });
    step.notes.push(`${Object.keys(repos).length} repositories, ${Object.values(before.counts).reduce((a, b) => a + b, 0)} rows`);
  });
  if (!repos) return;
  const instance = await Instance.create(work, "instance", data);

  await report.step("candidate `rbit backup` leaves an older database unmigrated", async (step) => {
    const copy = join(work, "backup-source");
    await cp(pristine, copy, { recursive: true });
    const source = await Instance.create(work, "backup-source", copy);
    const result = await source.run(candidate, ["backup", "--output", join(work, "backup-of-older")]);
    if (result.code !== 0) throw new Error(result.output);
    const after = inspectDatabase(join(copy, "rbit.db")).latest;
    const inBackup = inspectDatabase(join(work, "backup-of-older", "rbit.db")).latest;
    step.notes.push(`source database at ${after}, backup at ${inBackup} after the backup`);
    if (after !== before.latest || inBackup !== before.latest) throw new Error(`taking a backup migrated the data (source ${after}, backup ${inBackup})`);
  });
  await report.step(`candidate \`rbit migrate\` ${BASELINE_SCHEMA} → ${target}`, async (step) => {
    const started = performance.now();
    const result = await instance.run(candidate, ["migrate"]);
    migrateSeconds = (performance.now() - started) / 1000;
    if (result.code !== 0) throw new Error(result.output);
    const after = inspectDatabase(join(data, "rbit.db"));
    step.notes.push(`now at ${after.latest}`);
    const problems = [...after.integrity, ...countsPreserved(before.counts, after.counts)];
    if (after.latest !== target) problems.push(`stopped at ${after.latest}`);
    if (after.foreignKeyViolations) problems.push(`${after.foreignKeyViolations} foreign key violations`);
    if (problems.length) throw new Error(problems.join("; "));
    const created = await instance.run(candidate, ["user", "create", ADMIN.name, "rehearsal@example.test", "--admin"], { input: `${ADMIN.password}\n` });
    if (created.code !== 0) throw new Error(created.output);
  });
  await report.step("candidate serves the upgraded store over HTTP and Git", async () => {
    await instance.start(candidate);
    try { return await verifySynthetic(instance, repos, { git, work, label: "upgraded", wiki }); } finally { await instance.stop(); }
  });
  await report.step("candidate backup round trip (format 6 with wiki objects): `rbit backup`, `rbit restore`, serve", async (step) => {
    const backup = join(work, "backup-upgraded");
    const taken = await instance.run(candidate, ["backup", "--output", backup]);
    if (taken.code !== 0) throw new Error(taken.output);
    const manifest = JSON.parse(await readFile(join(backup, "manifest.json"), "utf8"));
    step.notes.push(`backup format ${manifest.format}: ${manifest.wikis} wiki(s), ${manifest.wiki_objects} wiki object(s)`);
    if (manifest.format < 6 || !(manifest.wikis >= 1)) throw new Error("the backup holds no wiki objects");
    const restored = await Instance.create(work, "restored", join(work, "data-restored"));
    const result = await restored.run(candidate, ["restore", "--from", backup]);
    if (result.code !== 0) throw new Error(result.output);
    await restored.start(candidate);
    try { return await verifySynthetic(restored, repos, { git, work, label: "restored", wiki }); } finally { await restored.stop(); }
  });
  await report.step("candidate refuses a database migrated by a newer build", async (step) => {
    const newer = join(work, "newer");
    await cp(data, newer, { recursive: true });
    markNewer(newer);
    await expectRefusal(step, { work, name: "candidate-on-newer", dataDir: newer, binary: candidate, pattern: /9999|newer|missing/i });
    await rm(newer, { recursive: true, force: true });
  });
  await report.step("interrupted upgrade: kill `rbit migrate` mid-way, then restart", (step) =>
    interruptedMigrations(step, { work, source: pristine, binary: candidate, delays: killDelays(migrateSeconds), target }));
}
