// Cases for evaluating an agent (or an agent guide) against a frozen candidate instance.
// Each case is a branch whose CI outcome is seeded and then left alone, so the true answer
// never changes while agents are graded against it.
//
// Cases are generated from a seed. The generator is public; the seed, and so which case is
// which and where its failure is, stays with whoever runs the eval. Case ids, branch names
// and commit messages are opaque, so nothing the agent can read through the API gives the
// answer away.
//
// The question for every case: "what is the CI result of commit <sha> on <branch>, and if
// it failed, where (file and line)?" The answer is
//   {"verdict": "success" | "failure" | "pending", "path": …, "line": …}
import { createHash } from "node:crypto";
import { lineOf, workflow } from "./fixtures.mjs";

/** How many cases of each kind a full set has (30 in all). */
export const MIX = { green: 5, lint: 7, test: 7, both: 4, pending: 3, stale: 4 };

/** A workflow whose jobs ask for a label no runner has: its run stays queued (for 24 hours). */
export function stuckWorkflow() {
  return workflow().replaceAll(/runs-on: \S+/g, "runs-on: no-such-runner");
}

/** A small deterministic generator (SplitMix32 over a SHA-256 of the seed). */
export function generator(seed) {
  let state = createHash("sha256").update(String(seed)).digest().readUInt32LE(0);
  const next = () => {
    state = (state + 0x9e3779b9) | 0;
    let z = state;
    z = Math.imul(z ^ (z >>> 16), 0x85ebca6b);
    z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35);
    return ((z ^ (z >>> 16)) >>> 0) / 2 ** 32;
  };
  return { next, pick: (items) => items[Math.floor(next() * items.length)] };
}

const FUNCTIONS = ["add() {", "sub() {", "mul() {", "div() {", "square() {", "cube() {"];
const CASE_LINES = ["add 2 3 5", "sub 7 4 3", "mul 6 7 42", "div 9 3 3", "square 5 0 25", "cube 3 0 27"];

function breakFunction(rng, read) {
  const text = read("src/calc.sh").replace(rng.pick(FUNCTIONS), (match) => `${match} @@`);
  return { "src/calc.sh": text };
}

function breakTest(rng, read) {
  const target = rng.pick(CASE_LINES);
  const parts = target.split(" ");
  parts[parts.length - 1] = String(Number(parts.at(-1)) + 1 + Math.floor(rng.next() * 5));
  return { "tests/cases.txt": read("tests/cases.txt").replace(target, parts.join(" ")), changedLine: parts.join(" ") };
}

/** Builds one case of `kind`: the files to change on top of `main` and the truth. */
function build(kind, rng) {
  const lintTruth = (changed) => ({ path: "src/calc.sh", line: lineOf(changed["src/calc.sh"], "@@") });
  const testTruth = (changed, line) => ({ path: "tests/cases.txt", line: lineOf(changed["tests/cases.txt"], line) });
  const harmless = (read) => ({ "src/calc.sh": `${read("src/calc.sh")}# ${Math.floor(rng.next() * 1e6)}\n` });
  switch (kind) {
    case "green": return { files: harmless, truth: () => ({ verdict: "success" }) };
    case "lint": return { files: (read) => breakFunction(rng, read), truth: (changed) => ({ verdict: "failure", ...lintTruth(changed) }) };
    case "test": {
      let line;
      return {
        files: (read) => { const { changedLine, ...files } = breakTest(rng, read); line = changedLine; return files; },
        truth: (changed) => ({ verdict: "failure", ...testTruth(changed, line) }),
      };
    }
    case "both": {
      // Two jobs fail; either location is a right answer.
      let line;
      return {
        files: (read) => { const { changedLine, ...tests } = breakTest(rng, read); line = changedLine; return { ...breakFunction(rng, read), ...tests }; },
        truth: (changed) => ({ verdict: "failure", ...lintTruth(changed), also: [testTruth(changed, line)] }),
      };
    }
    case "pending": return { files: (read) => ({ ...harmless(read), ".rbit/workflows/ci.yml": stuckWorkflow() }), truth: () => ({ verdict: "pending" }) };
    // A failed run on this branch, followed by a newer green run on another branch (seeded
    // as `chaser`), so "the repository's newest run" is the wrong run.
    case "stale": {
      let line;
      return {
        files: (read) => { const { changedLine, ...files } = breakTest(rng, read); line = changedLine; return files; },
        truth: (changed) => ({ verdict: "failure", ...testTruth(changed, line) }),
        chaser: true,
      };
    }
    default: throw new Error(`unknown kind ${kind}`);
  }
}

/**
 * The full case set for `seed`, shuffled, with opaque ids `c01`…`c30`. Each item:
 * `{ id, kind, files(read), truth(changed), chaser? }`. `kind` is for the grader only.
 */
export function generateCases(seed, mix = MIX) {
  const rng = generator(seed);
  const kinds = Object.entries(mix).flatMap(([kind, count]) => Array.from({ length: count }, () => kind));
  for (let i = kinds.length - 1; i > 0; i -= 1) {
    const j = Math.floor(rng.next() * (i + 1));
    [kinds[i], kinds[j]] = [kinds[j], kinds[i]];
  }
  return kinds.map((kind, i) => ({ id: `c${String(i + 1).padStart(2, "0")}`, kind, ...build(kind, rng) }));
}

/** Strips what does not change a location: a leading `./`, a trailing `:col`. */
export function normalizeAnswer(answer) {
  if (!answer || typeof answer !== "object") return null;
  let path = typeof answer.path === "string" ? answer.path.trim().replace(/^\.\//, "") : answer.path;
  let line = answer.line;
  const embedded = typeof path === "string" ? path.match(/^(.*?):(\d+)(?::\d+)?$/) : null;
  if (embedded) [, path, line] = [null, embedded[1], line ?? embedded[2]];
  return { verdict: typeof answer.verdict === "string" ? answer.verdict.trim().toLowerCase() : answer.verdict, path, line: line == null ? null : Number(line) };
}

/** Grades one answer: the verdict must match; a failure's location must be a true one. */
export function gradeAnswer(raw, truth) {
  const answer = normalizeAnswer(raw);
  if (!answer) return { correct: false, reason: "no answer" };
  if (answer.verdict !== truth.verdict) return { correct: false, reason: `verdict ${answer.verdict} != ${truth.verdict}` };
  if (truth.verdict !== "failure") return { correct: true, reason: "verdict" };
  const places = [{ path: truth.path, line: truth.line }, ...(truth.also ?? [])];
  const hit = places.some((place) => place.path === answer.path && place.line === answer.line);
  return hit ? { correct: true, reason: "verdict and location" } : { correct: false, reason: `location ${answer.path}:${answer.line} != ${truth.path}:${truth.line}` };
}

/** A split stratified by kind: about a third of each kind is held out. `kinds` maps id to kind. */
export function splitCases(kinds) {
  const byKind = {};
  for (const [id, kind] of Object.entries(kinds)) (byKind[kind] ??= []).push(id);
  const train = [];
  const test = [];
  for (const ids of Object.values(byKind)) ids.sort().forEach((id, i) => (i % 3 === 2 ? test : train).push(id));
  return { train: train.sort(), test: test.sort() };
}
