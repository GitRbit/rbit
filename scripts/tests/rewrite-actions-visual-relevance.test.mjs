import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, writeFile, rm, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { relevance, relevantPath } from "../../devtools/actions-visual/relevance.mjs";
const before = "a".repeat(40), after = "b".repeat(40);
const event = extra => ({ action: "synchronize", before, after, pull_request: { head: { sha: after } }, ...extra });

test("unknown, initial, manual and main events conservatively run without executing event strings", async () => {
  const noGit = () => { throw new Error("must not execute"); };
  for (const [name, payload] of [["push", {}], ["workflow_dispatch", {}], ["pull_request", { action: "opened" }],
    ["pull_request", { action: "reopened" }], ["pull_request", event({ before: "--upload-pack=unexpected" })],
    ["pull_request", event({ before: "0".repeat(40) })], ["pull_request", event({ after: before })],
    ["pull_request", event({ before: undefined })]]) {
    assert.equal((await relevance(name, payload, noGit)).run, true);
  }
});

test("path policy covers first workflow addition, removal, themes and review fixtures", () => {
  for (const path of [".github/workflows/actions-visual.yml", "docs/design/prototypes/actions/job.mjs",
    "crates/rbit-web/styles/app.css", "devtools/actions-visual/run.mjs", "package-lock.json",
    "scripts/tests/rewrite-actions-visual-relevance.test.mjs", "scripts/tests/fixtures/actions-markup.json",
    "docs/design/prototypes/actions", "devtools/actions-visual", "crates/rbit-web/styles"]) {
    assert.equal(relevantPath(path), true, path);
  }
  for (const path of ["README.md", "crates/rbit-services/src/accounts.rs", "package-lock.json.unrelated"]) assert.equal(relevantPath(path), false);
});

test("missing commits, unavailable Git and malformed framing cannot silently skip images", async () => {
  assert.equal((await relevance("pull_request", event(), async () => { throw new Error("unavailable"); })).run, true);
  assert.equal((await relevance("pull_request", event(), async args => args[0] === "diff" ? "README.md" : "")).run, true);
  const calls = [];
  let missing = true;
  const result = await relevance("pull_request", event(), async args => {
    calls.push(args);
    if (args[0] === "cat-file" && missing) { missing = false; throw new Error("shallow"); }
    return args[0] === "diff" ? "README.md\0" : "";
  });
  assert.equal(result.run, false);
  assert.deepEqual(calls.find(args => args[0] === "fetch"), ["fetch", "--no-tags", "--depth=1", "origin", before, after]);
  assert.deepEqual(calls.at(-1), ["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "-z", before, after, "--"]);
});

test("real before/after Git comparison ignores a prior relevant PR change but sees later deletion", async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-delta-"));
  const git = args => execFileSync("git", args, { cwd: directory, encoding: "utf8", windowsHide: true,
    env: { ...process.env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: process.platform === "win32" ? "NUL" : "/dev/null" } });
  const commit = () => { git(["add", "."]); git(["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "fixture"]); return git(["rev-parse", "HEAD"]).trim(); };
  try {
    git(["init", "-q"]);
    await writeFile(join(directory, "README.md"), "base"); commit();
    await mkdir(join(directory, ".github/workflows"), { recursive: true });
    await writeFile(join(directory, ".github/workflows/actions-visual.yml"), "new workflow");
    const added = commit();
    await writeFile(join(directory, "README.md"), "unrelated later update");
    const unrelated = commit();
    const payload = (a, b) => event({ before: a, after: b, pull_request: { head: { sha: b } } });
    assert.equal((await relevance("pull_request", payload(added, unrelated), git)).run, false);
    git(["mv", ".github/workflows/actions-visual.yml", "removed-workflow.txt"]);
    const removed = commit();
    assert.equal((await relevance("pull_request", payload(unrelated, removed), git)).run, true);
    assert.equal((await relevance("pull_request", payload(git(["rev-parse", `${added}^`]).trim(), added), git)).run, true);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("workflow gates only the heavy job and cannot cancel a running capture when new work skips", async () => {
  const workflow = await readFile(new URL("../../.github/workflows/actions-visual.yml", import.meta.url), "utf8");
  assert.doesNotMatch(workflow, /^concurrency:/m);
  assert.match(workflow, /prototype:\s+needs: relevance/);
  assert.match(workflow, /needs\.relevance\.result != 'success' \|\| needs\.relevance\.outputs\.run != 'false'/);
  assert.match(workflow, /group: actions-visual-capture-\$\{\{ github\.ref \}\}\s+cancel-in-progress: false/);
  assert.match(workflow, /Remove only the positively owned container[^\n]*\n\s+if: \$\{\{ always\(\) \}\}/);
});

test("CLI output cannot be injected by event text and never contains changed paths or secrets", async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-visual-event-"));
  try {
    const eventPath = join(directory, "event.json"), outputPath = join(directory, "output.txt");
    const sentinel = "private-value\nrun=false\nreason=forged";
    await writeFile(eventPath, JSON.stringify(event({ before: sentinel })));
    const stdout = execFileSync(process.execPath, [fileURLToPath(new URL("../../devtools/actions-visual/relevance.mjs", import.meta.url))], {
      encoding: "utf8", windowsHide: true, env: { ...process.env, GITHUB_EVENT_NAME: "pull_request",
        GITHUB_EVENT_PATH: eventPath, GITHUB_OUTPUT: outputPath },
    });
    assert.deepEqual(JSON.parse(stdout), { run: true, reason: "UNKNOWN_COMMIT_IDENTITY" });
    assert.equal(await readFile(outputPath, "utf8"), "run=true\nreason=UNKNOWN_COMMIT_IDENTITY\n");
    assert.equal(stdout.includes("private-value"), false);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
