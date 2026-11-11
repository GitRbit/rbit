// The benchmark's dummy project: a small shell "library" with a linter, a test runner and a
// workflow, generated fresh for every run so nothing about it is committed or remembered.
// Each scenario changes it in a known way, so the right answer (the failing file and line,
// or "no run expected") is known exactly and grading needs no judgement.
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { RUNNER_LABEL } from "./instance.mjs";

/** Lines of passing test output printed before any result: the noise a real suite makes. */
export const TEST_NOISE_LINES = 3000;

const LIBRARY = `#!/usr/bin/env bash
# A tiny calculator library. Each function prints its result.
add() { echo $(( $1 + $2 )); }
sub() { echo $(( $1 - $2 )); }
mul() { echo $(( $1 * $2 )); }
div() {
  if [ "$2" -eq 0 ]; then echo "division by zero" >&2; return 1; fi
  echo $(( $1 / $2 ))
}
square() { mul "$1" "$1"; }
cube() { echo $(( $1 * $1 * $1 )); }
`;

// The linter reports every line holding the marker, in the formats a real compiler and a
// GitHub-style tool use: a "path:line:col: error:" line and a workflow-command annotation.
const LINT = `#!/usr/bin/env bash
set -u
status=0
for file in src/*.sh; do
  bash -n "$file" || status=1
  while IFS=: read -r line text; do
    col=$(( $(printf '%s' "$text" | awk '{ print index($0, "@@") }') ))
    echo "$file:$line:$col: error: unknown token '@@'"
    echo "::error file=$file,line=$line,col=$col::unknown token '@@'"
    status=1
  done < <(grep -n '@@' "$file" || true)
done
[ "$status" -eq 0 ] && echo "lint: clean"
exit "$status"
`;

// Cases are "name function a b expected", one per line, so a failure has a file and line.
const CASES = `adds_numbers add 2 3 5
subtracts_numbers sub 7 4 3
multiplies_numbers mul 6 7 42
divides_numbers div 9 3 3
squares_numbers square 5 0 25
cubes_numbers cube 3 0 27
`;

const TEST_RUNNER = `#!/usr/bin/env bash
set -u
source src/calc.sh
for i in $(seq 1 ${TEST_NOISE_LINES}); do echo "test noise::case_$i ... ok"; done
# A flaky case fails on the first attempt of a run and passes on any re-run.
if [ -f tests/flaky ] && [ "\${GITHUB_RUN_ATTEMPT:-1}" = "1" ]; then
  echo "FAIL timing_sensitive (tests/flaky:1): deadline exceeded"
  exit 1
fi
failed=0
line=0
while read -r name fn a b expected; do
  line=$((line + 1))
  [ -z "$name" ] && continue
  got=$("$fn" "$a" "$b" 2>&1)
  if [ "$got" = "$expected" ]; then echo "test $name ... ok"
  else echo "FAIL $name (tests/cases.txt:$line): expected $expected, got $got"; failed=$((failed + 1)); fi
done < tests/cases.txt
echo "test result: $failed failed"
[ "$failed" -eq 0 ]
`;

/** The fixture's workflow. Docs-only pushes start no run (paths-ignore). */
export function workflow({ timeoutMinutes = 2 } = {}) {
  return `name: Fixture CI
on:
  push:
    paths-ignore: ["docs/**", "**.md"]
  workflow_dispatch:
jobs:
  lint:
    runs-on: ${RUNNER_LABEL}
    steps:
      - uses: actions/checkout@v4
      - name: Lint
        run: bash tools/lint.sh
  test:
    runs-on: ${RUNNER_LABEL}
    timeout-minutes: ${timeoutMinutes}
    steps:
      - uses: actions/checkout@v4
      - name: Unit tests
        run: bash tests/run.sh
      - name: Slow step
        run: if [ -f tests/slow ]; then sleep "$(cat tests/slow)"; fi
`;
}

/** The fixture's files at its initial commit, path to contents. */
export function baseFiles() {
  return {
    "README.md": "# calc\n\nA dummy project for rbit's agent-loop benchmark.\n",
    "docs/guide.md": "# Guide\n\nCall `add 2 3`.\n",
    "src/calc.sh": LIBRARY,
    "tools/lint.sh": LINT,
    "tests/cases.txt": CASES,
    "tests/run.sh": TEST_RUNNER,
    ".rbit/workflows/ci.yml": workflow(),
  };
}

/** A git checkout of the fixture, driven with an explicit identity and no prompts. */
export class FixtureRepo {
  constructor(dir, { remote, token }) {
    this.dir = dir;
    this.remote = remote;
    const basic = Buffer.from(`agent:${token}`).toString("base64");
    // The token reaches git through the environment only: never a URL or a command line.
    this.env = {
      ...process.env, GIT_TERMINAL_PROMPT: "0", GIT_AUTHOR_NAME: "Agent Loop", GIT_AUTHOR_EMAIL: "agent@example.test",
      GIT_COMMITTER_NAME: "Agent Loop", GIT_COMMITTER_EMAIL: "agent@example.test",
      GIT_CONFIG_COUNT: "3", GIT_CONFIG_KEY_0: "credential.helper", GIT_CONFIG_VALUE_0: "",
      GIT_CONFIG_KEY_1: "http.extraHeader", GIT_CONFIG_VALUE_1: `Authorization: Basic ${basic}`,
      GIT_CONFIG_KEY_2: "init.defaultBranch", GIT_CONFIG_VALUE_2: "main",
    };
  }

  git(...args) {
    return execFileSync("git", args, { cwd: this.dir, env: this.env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  }

  write(files) {
    for (const [path, contents] of Object.entries(files)) {
      mkdirSync(dirname(join(this.dir, path)), { recursive: true });
      writeFileSync(join(this.dir, path), contents);
    }
  }

  read(path) {
    return readFileSync(join(this.dir, path), "utf8");
  }

  init() {
    mkdirSync(this.dir, { recursive: true });
    this.git("init", "--quiet");
    this.write(baseFiles());
    this.commit("Initial fixture");
    return this.git("rev-parse", "HEAD");
  }

  commit(message) {
    this.git("add", "--all");
    this.git("commit", "--quiet", "--allow-empty", "-m", message);
    return this.git("rev-parse", "HEAD");
  }

  /** Starts `branch` from `main`, discarding local changes. */
  branch(branch) {
    this.git("checkout", "--quiet", "-B", branch, "main");
  }

  push(branch, { force = true } = {}) {
    this.git("push", "--quiet", ...(force ? ["--force"] : []), this.remote, `HEAD:refs/heads/${branch}`);
  }
}

/** The 1-based line of the first occurrence of `needle` in `text`. */
export function lineOf(text, needle) {
  const index = text.split("\n").findIndex((line) => line.includes(needle));
  if (index < 0) throw new Error(`fixture text has no ${JSON.stringify(needle)}`);
  return index + 1;
}
