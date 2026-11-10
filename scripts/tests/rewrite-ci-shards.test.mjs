import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { MAX_RESUMED_TESTS, escapeFilterText, failureRecord, filterExpression, parseAttempt, parseJUnitFailures, parseShard, resumePlan }
  from "../ci/nextest-failures.mjs";

// Shaped like cargo-nextest's report: one <testsuite> per binary, classname = binary id.
function junit(suites) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="9" failures="1" errors="0">
${suites.map(([binary, cases]) => `    <testsuite name="${binary}" tests="${cases.length}">
${cases.map(([name, body = ""]) => `        <testcase name="${name}" classname="${binary}" time="0.1">${body}</testcase>`).join("\n")}
    </testsuite>`).join("\n")}
</testsuites>
`;
}
const failure = `<failure type="test failure with exit code 101">thread panicked at &apos;boom&apos;</failure><system-out><![CDATA[<testcase name="fake">]]></system-out>`;

test("JUnit parsing finds failed tests, including errors, across binaries", () => {
  const report = junit([
    ["rbit-web::integration", [["pages::renders"], ["pages::fails", failure], ["timeouts::hangs", `<error type="test timeout"/>`]]],
    ["rbit-core", [["parse::ok"], ["parse::flaky", `<flakyFailure type="test failure"/>`], ["skip::me", "<skipped/>"]]],
    ["rbit-services::integration", [["lfs::a_cancelled_upload_leaves_no_partial_object_or_lock", failure]]],
  ]);
  assert.deepEqual(parseJUnitFailures(report), [
    { binaryId: "rbit-services::integration", name: "lfs::a_cancelled_upload_leaves_no_partial_object_or_lock" },
    { binaryId: "rbit-web::integration", name: "pages::fails" },
    { binaryId: "rbit-web::integration", name: "timeouts::hangs" },
  ]);
  assert.deepEqual(parseJUnitFailures(junit([["rbit-core", [["parse::ok"]]]])), []);
  assert.deepEqual(parseJUnitFailures(`<testsuites name="nextest-run" tests="0"/>`), []);
});

test("JUnit names with quotes, entities and markup-like text are decoded exactly", () => {
  const report = junit([["odd bin", [["quote &quot;double&quot; &apos;single&apos; &lt;tag&gt; &amp; &#x263A; &#65;", failure],
    [`regex .*+?^$[](){}|\\ (a, b)`, failure]]]]);
  assert.deepEqual(parseJUnitFailures(report).map((item) => item.name).sort(),
    [`quote "double" 'single' <tag> & ☺ A`, `regex .*+?^$[](){}|\\ (a, b)`].sort());
  // A '>' inside a quoted attribute does not end the tag.
  assert.deepEqual(parseJUnitFailures(`<testsuites><testsuite name="b"><testcase name="a>b" classname="b"><failure/></testcase></testsuite></testsuites>`),
    [{ binaryId: "b", name: "a>b" }]);
});

test("malformed or foreign reports are rejected rather than read as passing", () => {
  for (const bad of ["", "<testsuite name=\"x\"></testsuite>", "<testsuites><testsuite name=\"x\">",
    "<testsuites><testcase name=\"x\"></testsuites>", "<testsuites><testsuite name=\"x\"><testcase name=\"t\" classname=\"x\"><failure/>",
    "<testsuites><testsuite><testcase><failure/></testcase></testsuite></testsuites>", "<testsuites><!-- open"]) {
    assert.throws(() => parseJUnitFailures(bad), undefined, bad);
  }
  assert.throws(() => parseJUnitFailures(undefined), TypeError);
});

test("filter text escapes everything that could end the matcher or start another expression", () => {
  assert.equal(escapeFilterText("module::test_name-1.2"), "module::test_name-1.2");
  assert.equal(escapeFilterText("a)b,c"), "a\\u{29}b\\u{2c}c");
  assert.equal(escapeFilterText(`"q" 'q' \\ /`), "\\u{22}q\\u{22}\\u{20}\\u{27}q\\u{27}\\u{20}\\u{5c}\\u{20}\\u{2f}");
  assert.equal(escapeFilterText("x\n$(rm -rf)`id`;|&"), "x\\u{a}\\u{24}\\u{28}rm\\u{20}-rf\\u{29}\\u{60}id\\u{60}\\u{3b}\\u{7c}\\u{26}");
  assert.equal(escapeFilterText("\u{1F600}é"), "\\u{1f600}\\u{e9}");
  for (const bad of ["", null, undefined, 42]) assert.throws(() => escapeFilterText(bad));
  const hostile = `a) | all() | test(=b`;
  // Every byte outside the safe set is escaped, so the expression has no structure of its own.
  assert.doesNotMatch(escapeFilterText(hostile), /[()|&,\s=]/);
});

test("filter expressions select exact tests grouped by binary, deterministically", () => {
  const failures = [
    { binaryId: "rbit-web::integration", name: "b::second" },
    { binaryId: "rbit-core", name: "only" },
    { binaryId: "rbit-web::integration", name: "a::first" },
    { binaryId: "rbit-web::integration", name: "a::first" },
  ];
  const expected = "(binary_id(=rbit-core) & (test(=only))) | (binary_id(=rbit-web::integration) & (test(=a::first) | test(=b::second)))";
  assert.equal(filterExpression(failures), expected);
  assert.equal(filterExpression([...failures].reverse()), expected);
  assert.equal(filterExpression([{ binaryId: "x", name: "it's (1, 2)" }]), "(binary_id(=x) & (test(=it\\u{27}s\\u{20}\\u{28}1\\u{2c}\\u{20}2\\u{29})))");
  assert.throws(() => filterExpression([]), /No failed tests/);
});

test("shard and attempt inputs are validated", () => {
  assert.deepEqual(parseShard("2/4"), { index: 2, count: 4 });
  for (const bad of ["0/4", "5/4", "2", "2/4/1", "a/b", "2 /4", "", undefined, "1/4;x"]) assert.throws(() => parseShard(bad), undefined, String(bad));
  assert.equal(parseAttempt("3"), 3);
  for (const bad of ["0", "-1", "1.5", "", undefined, "2;rm"]) assert.throws(() => parseAttempt(bad), undefined, String(bad));
});

const id = { os: "windows-latest", shard: "2/4" };
const failed = [{ binaryId: "rbit-services::integration", name: "lfs::upload" }];
const record = (attempt, overrides = {}) => ({ schema: 1, ...id, attempt, trusted: true, reason: "", failed, ...overrides });

test("a re-run resumes the latest trusted failure list of the same shard", () => {
  assert.deepEqual(resumePlan([record(1)], { ...id, attempt: 1 }), { mode: "full", reason: "first attempt" });
  assert.equal(resumePlan([], { ...id, attempt: 2 }).mode, "full");
  const plan = resumePlan([record(1)], { ...id, attempt: 2 });
  assert.equal(plan.mode, "resume"); assert.equal(plan.from, 1); assert.deepEqual(plan.tests, failed);
  assert.equal(plan.filter, "(binary_id(=rbit-services::integration) & (test(=lfs::upload)))");
  // The newest earlier attempt decides; the current and later attempts are ignored.
  const second = [{ binaryId: "rbit-core", name: "still::failing" }];
  assert.deepEqual(resumePlan([record(1), record(2, { failed: second }), record(3, { failed: [] })], { ...id, attempt: 3 }).tests, second);
  // An attempt in which this shard did not run leaves the earlier list in charge.
  assert.equal(resumePlan([record(1)], { ...id, attempt: 4 }).from, 1);
});

test("passed, incomplete, foreign or oversized lists run the whole shard", () => {
  const attempt = { ...id, attempt: 2 };
  assert.match(resumePlan([record(1, { failed: [] })], attempt).reason, /no failed tests/);
  assert.match(resumePlan([record(1, { trusted: false, reason: "test step cancelled", failed: [] })], attempt).reason, /incomplete/);
  // A newer untrusted record overrides an older trusted one.
  assert.equal(resumePlan([record(1), record(2, { trusted: false, failed: [] })], { ...id, attempt: 3 }).mode, "full");
  for (const foreign of [record(1, { os: "ubuntu-latest" }), record(1, { shard: "1/4" }), record(1, { schema: 2 }),
    record(1, { attempt: "1" }), record(1, { failed: [{ binaryId: "", name: "x" }] }), null, "text", []]) {
    assert.equal(resumePlan([foreign], attempt).mode, "full", JSON.stringify(foreign));
  }
  const many = Array.from({ length: MAX_RESUMED_TESTS + 1 }, (_, index) => ({ binaryId: "b", name: `t${index}` }));
  assert.equal(resumePlan([record(1, { failed: many })], attempt).mode, "full");
  const long = Array.from({ length: 50 }, (_, index) => ({ binaryId: "b", name: `${"é".repeat(60)}${index}` }));
  assert.match(resumePlan([record(1, { failed: long })], attempt).reason, /too long/);
});

test("only a finished nextest run produces a trusted failure list", () => {
  const report = junit([["rbit-core", [["fails", failure], ["passes"]]]]);
  const base = { ...id, attempt: 1 };
  const trusted = failureRecord({ ...base, outcome: "failure", exitCode: 100, junit: report });
  assert.equal(trusted.trusted, true); assert.deepEqual(trusted.failed, [{ binaryId: "rbit-core", name: "fails" }]);
  const passed = failureRecord({ ...base, outcome: "success", exitCode: 0, junit: junit([["rbit-core", [["passes"]]]]) });
  assert.equal(passed.trusted, true); assert.deepEqual(passed.failed, []);
  for (const [why, input] of [
    ["cancelled step", { outcome: "cancelled", exitCode: 100, junit: report }],
    ["skipped step", { outcome: "", exitCode: null, junit: null }],
    ["interrupted run", { outcome: "failure", exitCode: 106, junit: report }],
    ["listing error", { outcome: "failure", exitCode: 104, junit: null }],
    ["no exit code", { outcome: "failure", exitCode: null, junit: report }],
    ["no report", { outcome: "failure", exitCode: 100, junit: null }],
    ["broken report", { outcome: "failure", exitCode: 100, junit: "<testsuites>" }],
    ["failure without failed test", { outcome: "failure", exitCode: 100, junit: junit([["rbit-core", [["passes"]]]]) }],
    ["passing run with failed test", { outcome: "success", exitCode: 0, junit: report }],
  ]) {
    const result = failureRecord({ ...base, ...input });
    assert.equal(result.trusted, false, why); assert.deepEqual(result.failed, [], why); assert.ok(result.reason, why);
  }
});

test("the record command writes the list a later attempt reads", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "rbit-test-shards-")); t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "target", "nextest", "ci"), { recursive: true });
  await writeFile(join(root, "target", "nextest", "ci", "junit.xml"), junit([["rbit-web::integration", [["a &quot;quoted&quot; (name)", failure], ["ok"]]]]));
  await writeFile(join(root, "status.json"), JSON.stringify({ exitCode: 100, mode: "full" }));
  const cli = fileURLToPath(new URL("../ci/nextest-shard.mjs", import.meta.url));
  const env = { ...process.env, TEST_OS: "windows-latest", TEST_SHARD: "2/4", RUN_ATTEMPT: "1", TEST_OUTCOME: "failure",
    SHARD_STATUS: join(root, "status.json"), FAILURES_OUTPUT: join(root, "failed.json") };
  execFileSync(process.execPath, [cli, "record"], { cwd: root, env, windowsHide: true });
  const written = JSON.parse(await readFile(join(root, "failed.json"), "utf8"));
  assert.equal(written.trusted, true);
  const plan = resumePlan([written], { os: "windows-latest", shard: "2/4", attempt: 2 });
  assert.deepEqual(plan.tests, [{ binaryId: "rbit-web::integration", name: `a "quoted" (name)` }]);
  // Without nextest's exit status the attempt is untrusted.
  await rm(join(root, "status.json"));
  execFileSync(process.execPath, [cli, "record"], { cwd: root, env, windowsHide: true });
  assert.equal(JSON.parse(await readFile(join(root, "failed.json"), "utf8")).trusted, false);
});

// The workflow is checked as text, like the other CI contract tests: the job layout is
// deliberately regular, and drift fails here instead of being half-understood.
function job(workflow, id) {
  const start = workflow.indexOf(`\n  ${id}:\n`);
  assert.ok(start >= 0, `job ${id} exists`);
  return workflow.slice(start + 1).split(/\n(?=  [a-z][a-z0-9-]*:\n)/)[0];
}

test("each platform builds once, shards from the archive and gates on every part", async () => {
  const workflow = (await readFile(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8")).replaceAll("\r\n", "\n");
  for (const [platform, os, shards] of [["linux", "ubuntu-latest", 2], ["windows", "windows-latest", 4]]) {
    const prefix = `tests-${platform}`;
    const build = job(workflow, `${prefix}-build`), shard = job(workflow, `${prefix}-shards`);
    const extras = job(workflow, `${prefix}-extras`), gate = job(workflow, prefix);

    assert.match(build, new RegExp(`runs-on: ${os}\\n`));
    assert.match(build, /- run: cargo nextest archive --workspace --locked --archive-file target\/nextest-archive\.tar\.zst\n/);
    assert.ok(build.includes(`name: nextest-archive-${os}\n`)); assert.match(build, /overwrite: true/);
    assert.match(build, /if-no-files-found: error/);
    for (const asset of ["app.css", "deferred.css"]) assert.ok(build.includes(`crates/rbit-web/assets/${asset}`));

    assert.ok(shard.includes(`name: Tests (${os}, \${{ matrix.shard }}/\${{ matrix.total }})\n`));
    assert.match(shard, new RegExp(`needs: ${prefix}-build\\n`)); assert.match(shard, new RegExp(`runs-on: ${os}\\n`));
    assert.match(shard, /fail-fast: false/);
    assert.ok(shard.includes(`shard: [${Array.from({ length: shards }, (_, index) => index + 1).join(", ")}]\n        total: [${shards}]\n`));
    assert.ok(shard.includes(`TEST_OS: ${os}\n`)); assert.ok(shard.includes("TEST_SHARD: ${{ matrix.shard }}/${{ matrix.total }}\n"));
    assert.ok(shard.includes("RUN_ATTEMPT: ${{ github.run_attempt }}\n"));
    assert.ok(shard.includes(`name: nextest-archive-${os}\n`));
    assert.doesNotMatch(shard, /rust-cache|cargo (?:build|test|nextest run)|--locked/, "shards compile nothing");
    // Failed-test lists: uploaded per shard and attempt, always; downloaded by pattern on re-runs.
    const list = `nextest-failed-${os}-shard-\${{ matrix.shard }}-of-\${{ matrix.total }}-attempt-`;
    assert.ok(shard.includes(`- if: \${{ github.run_attempt != '1' }}\n        uses: actions/download-artifact@`));
    assert.ok(shard.includes(`pattern: ${list}*\n`)); assert.match(shard, /merge-multiple: true/);
    assert.ok(shard.includes(`- if: \${{ always() }}\n        uses: actions/upload-artifact@`));
    assert.ok(shard.includes(`name: ${list}\${{ github.run_attempt }}\n`));
    assert.ok(shard.includes(`failed-${os}-\${{ matrix.shard }}-of-\${{ matrix.total }}-attempt-\${{ github.run_attempt }}.json\n`));
    assert.ok(shard.includes("- id: tests\n        run: node scripts/ci/nextest-shard.mjs run\n"));
    assert.ok(shard.includes("- if: ${{ always() }}\n        run: node scripts/ci/nextest-shard.mjs record\n"));
    assert.ok(shard.includes("TEST_OUTCOME: ${{ steps.tests.outcome }}\n"));
    // Expressions reach scripts through env only, never through shell source.
    for (const line of shard.split("\n").filter((text) => /^\s+(?:- )?run:/.test(text))) assert.doesNotMatch(line, /\$\{\{/);

    assert.match(extras, /- run: cargo test --workspace --doc --locked\n/);
    assert.equal(extras.includes("cargo build --locked --package rbit-server"), platform === "linux");
    assert.equal(extras.includes("scripts/tests/release-rehearsal.test.mjs"), platform === "linux");

    assert.ok(gate.includes(`    name: Tests (${os})\n`)); assert.ok(gate.includes("if: ${{ always() }}"));
    assert.ok(gate.includes(`needs: [${prefix}-build, ${prefix}-shards, ${prefix}-extras]\n`));
    for (const part of ["build", "shards", "extras"]) {
      assert.ok(gate.includes(`: \${{ needs.${prefix}-${part}.result }}\n`), part);
    }
    assert.match(gate, /\[ "\$BUILD" = success \] && \[ "\$SHARDS" = success \] && \[ "\$EXTRAS" = success \]\n/);
  }
  const nextest = await readFile(new URL("../../.config/nextest.toml", import.meta.url), "utf8");
  assert.match(nextest.replaceAll("\r\n", "\n"), /\[profile\.ci\.junit\]\n(?:#[^\n]*\n)*path = "junit\.xml"\n/);
  assert.doesNotMatch(workflow, /actions: (?:read|write)|pull_request_target|secrets\./);
});
