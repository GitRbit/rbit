// Failed-test bookkeeping for sharded CI test jobs.
//
// A shard job records the tests that failed in each attempt. When GitHub re-runs the
// failed job, the next attempt reads that record and runs only those tests. Every
// function here is pure so the contract tests can cover it without cargo-nextest.

export const LIST_SCHEMA = 1;
// nextest's exit codes: 0 for a passing run, 100 for a completed run with failures.
// Anything else (interrupted run, setup or listing error) leaves the record untrusted.
export const NEXTEST_OK = 0;
export const NEXTEST_TEST_RUN_FAILED = 100;
// Beyond these limits a resumed attempt runs the whole shard: the filter would be
// unwieldy, and on Windows the command line must stay well under 32 767 characters.
export const MAX_RESUMED_TESTS = 300;
export const MAX_FILTER_LENGTH = 16_000;

const entities = { lt: "<", gt: ">", amp: "&", quot: "\"", apos: "'" };

function decodeEntities(text) {
  return text.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-z]+);/g, (whole, body) => {
    if (body[0] === "#") {
      const code = body[1] === "x" ? Number.parseInt(body.slice(2), 16) : Number.parseInt(body.slice(1), 10);
      return Number.isInteger(code) && code >= 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
    }
    return Object.hasOwn(entities, body) ? entities[body] : whole;
  });
}

function parseAttributes(source) {
  const attributes = {};
  const pattern = /([A-Za-z_][\w.:-]*)\s*=\s*("([^"]*)"|'([^']*)')/g;
  for (const match of source.matchAll(pattern)) attributes[match[1]] = decodeEntities(match[3] ?? match[4]);
  return attributes;
}

// A deliberately small XML reader: elements, attributes, comments, CDATA, processing
// instructions and doctype declarations. Text content is not needed and is skipped.
function* elements(xml) {
  let index = 0;
  while (index < xml.length) {
    const open = xml.indexOf("<", index);
    if (open < 0) return;
    if (xml.startsWith("<!--", open)) { index = expectEnd(xml, "-->", open); continue; }
    if (xml.startsWith("<![CDATA[", open)) { index = expectEnd(xml, "]]>", open); continue; }
    if (xml.startsWith("<?", open)) { index = expectEnd(xml, "?>", open); continue; }
    if (xml.startsWith("<!", open)) { index = expectEnd(xml, ">", open); continue; }
    // Quoted attribute values may contain '>', so scan the tag quote-aware.
    let end = open + 1, quote = null;
    for (; end < xml.length; end++) {
      const char = xml[end];
      if (quote) { if (char === quote) quote = null; }
      else if (char === "\"" || char === "'") quote = char;
      else if (char === ">") break;
    }
    if (end >= xml.length) throw new Error("JUnit report ends inside a tag");
    const body = xml.slice(open + 1, end);
    index = end + 1;
    if (body.startsWith("/")) { yield { kind: "close", name: body.slice(1).trim() }; continue; }
    const selfClosing = body.endsWith("/");
    const inner = selfClosing ? body.slice(0, -1) : body;
    const name = inner.match(/^[^\s/>]+/)?.[0];
    if (!name) throw new Error("JUnit report has a malformed tag");
    yield { kind: "open", name, attributes: parseAttributes(inner.slice(name.length)) };
    if (selfClosing) yield { kind: "close", name };
  }
}

function expectEnd(xml, terminator, from) {
  const end = xml.indexOf(terminator, from);
  if (end < 0) throw new Error("JUnit report ends inside a comment or declaration");
  return end + terminator.length;
}

/**
 * Returns the failed tests in a nextest JUnit report as `{ binaryId, name }`, sorted
 * and without duplicates. A test failed when its `<testcase>` has a `<failure>` or
 * `<error>` child; `<flakyFailure>` (passed on retry) and `<skipped>` do not count.
 */
export function parseJUnitFailures(xml) {
  if (typeof xml !== "string") throw new TypeError("JUnit report must be text");
  const stack = [], failures = new Map();
  let suite = null, testcase = null, sawSuites = false;
  for (const token of elements(xml)) {
    if (token.kind === "open") {
      stack.push(token.name);
      if (token.name === "testsuites") sawSuites = true;
      else if (token.name === "testsuite") suite = token.attributes.name ?? null;
      else if (token.name === "testcase") {
        testcase = { binaryId: token.attributes.classname || suite, name: token.attributes.name, depth: stack.length, failed: false };
      } else if (testcase && stack.length === testcase.depth + 1 && (token.name === "failure" || token.name === "error")) {
        testcase.failed = true;
      }
      continue;
    }
    if (stack.at(-1) !== token.name) throw new Error(`JUnit report closes <${token.name}> out of order`);
    stack.pop();
    if (token.name === "testcase" && testcase) {
      if (testcase.failed) {
        if (!testcase.binaryId || !testcase.name) throw new Error("A failed JUnit test case lacks its binary or test name");
        const failure = { binaryId: testcase.binaryId, name: testcase.name };
        failures.set(JSON.stringify([failure.binaryId, failure.name]), failure);
      }
      testcase = null;
    } else if (token.name === "testsuite") suite = null;
  }
  if (stack.length) throw new Error("JUnit report is truncated");
  if (!sawSuites) throw new Error("Not a JUnit report: <testsuites> is missing");
  return [...failures.values()].sort((a, b) => a.binaryId.localeCompare(b.binaryId) || a.name.localeCompare(b.name));
}

/**
 * Encodes text for an exact (`=`) nextest filterset matcher. Every character other
 * than ASCII letters, digits, `_`, `-`, `.` and `:` becomes a `\u{…}` escape, so test
 * names with quotes, spaces, parentheses, commas or regex syntax reach nextest as
 * plain literals and can never close the matcher or start another expression.
 */
export function escapeFilterText(text) {
  if (typeof text !== "string" || text.length === 0) throw new Error("Filter text must be a non-empty string");
  let escaped = "";
  for (const char of text) {
    escaped += /^[A-Za-z0-9_.:-]$/.test(char) ? char : `\\u{${char.codePointAt(0).toString(16)}}`;
  }
  return escaped;
}

/** A nextest filterset selecting exactly the given tests, grouped by test binary. */
export function filterExpression(failures) {
  if (!Array.isArray(failures) || failures.length === 0) throw new Error("No failed tests to select");
  const byBinary = new Map();
  for (const { binaryId, name } of failures) {
    if (!byBinary.has(binaryId)) byBinary.set(binaryId, new Set());
    byBinary.get(binaryId).add(name);
  }
  return [...byBinary].sort(([a], [b]) => a.localeCompare(b)).map(([binaryId, names]) => {
    const tests = [...names].sort().map((name) => `test(=${escapeFilterText(name)})`).join(" | ");
    return `(binary_id(=${escapeFilterText(binaryId)}) & (${tests}))`;
  }).join(" | ");
}

/** Validates a shard like `2/4`; returns `{ index, count }`. */
export function parseShard(value) {
  const match = /^([1-9][0-9]?)\/([1-9][0-9]?)$/.exec(String(value ?? ""));
  if (!match || Number(match[1]) > Number(match[2])) throw new Error(`Invalid shard: ${JSON.stringify(value)}`);
  return { index: Number(match[1]), count: Number(match[2]) };
}

export function parseAttempt(value) {
  if (!/^[1-9][0-9]{0,5}$/.test(String(value ?? ""))) throw new Error(`Invalid run attempt: ${JSON.stringify(value)}`);
  return Number(value);
}

/**
 * Builds the record a shard uploads after each attempt. `failed` is filled only when
 * the attempt's result can be trusted: nextest finished (exit 0 or 100), the step was
 * not cancelled, and a failed run named at least one failed test. Anything else makes
 * the next attempt run the whole shard.
 */
export function failureRecord({ os, shard, attempt, outcome, exitCode, junit }) {
  const record = { schema: LIST_SCHEMA, os, shard, attempt, trusted: false, reason: "", failed: [] };
  if (outcome !== "success" && outcome !== "failure") { record.reason = `test step ${outcome || "did not run"}`; return record; }
  if (exitCode !== NEXTEST_OK && exitCode !== NEXTEST_TEST_RUN_FAILED) {
    record.reason = exitCode === null ? "nextest did not report an exit code" : `nextest exited with ${exitCode}`;
    return record;
  }
  if (junit === null) { record.reason = "no JUnit report"; return record; }
  let failed;
  try { failed = parseJUnitFailures(junit); }
  catch (error) { record.reason = `unreadable JUnit report: ${error.message}`; return record; }
  if (exitCode === NEXTEST_TEST_RUN_FAILED && failed.length === 0) { record.reason = "nextest failed without a failed test"; return record; }
  if (exitCode === NEXTEST_OK && failed.length > 0) { record.reason = "a passing run reported failed tests"; return record; }
  record.trusted = true;
  record.reason = failed.length ? `${failed.length} failed` : "passed";
  record.failed = failed.map(({ binaryId, name }) => ({ binaryId, name }));
  return record;
}

function validRecord(value, os, shard) {
  return value && typeof value === "object" && value.schema === LIST_SCHEMA && value.os === os && value.shard === shard &&
    Number.isInteger(value.attempt) && value.attempt >= 1 && typeof value.trusted === "boolean" && Array.isArray(value.failed) &&
    value.failed.every((item) => item && typeof item.binaryId === "string" && item.binaryId && typeof item.name === "string" && item.name);
}

/**
 * Chooses what a shard runs in `attempt`, given the records of earlier attempts. The
 * most recent earlier record decides: a trusted, non-empty failure list resumes those
 * tests; anything else (none, passed, untrusted, too large) runs the whole shard.
 */
export function resumePlan(records, { os, shard, attempt }) {
  if (attempt <= 1) return { mode: "full", reason: "first attempt" };
  const earlier = records.filter((record) => validRecord(record, os, shard) && record.attempt < attempt)
    .sort((a, b) => b.attempt - a.attempt);
  const previous = earlier[0];
  if (!previous) return { mode: "full", reason: "no failed-test list from an earlier attempt" };
  const from = `attempt ${previous.attempt}`;
  if (!previous.trusted) return { mode: "full", reason: `${from} is incomplete (${previous.reason || "untrusted"})` };
  if (previous.failed.length === 0) return { mode: "full", reason: `${from} had no failed tests` };
  if (previous.failed.length > MAX_RESUMED_TESTS) return { mode: "full", reason: `${from} had ${previous.failed.length} failed tests` };
  const filter = filterExpression(previous.failed);
  if (filter.length > MAX_FILTER_LENGTH) return { mode: "full", reason: `${from}'s filter is too long` };
  return { mode: "resume", reason: `${previous.failed.length} failed test(s) from ${from}`, from: previous.attempt, tests: previous.failed, filter };
}
