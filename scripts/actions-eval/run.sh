#!/usr/bin/env bash
# The third-party actions eval: real workflows that use real actions, run with
# `rbit-runner exec` and graded by their result and their log.
#
#   scripts/actions-eval/run.sh [--split train|test|all] [--backend host|container] [case…]
#
# Each case is a directory of `cases/`: the files of a repository (with
# `.github/workflows/ci.yml`) and `expect.txt`:
#
#   result: success            # or failure
#   split: train               # or test: improve against train, report test
#   contains: v20.             # a line of the log must contain this (any number)
#   lacks: ##[debug]           # no line may contain this (any number)
#   skip: windows-host         # not on a Windows host (also: host, container)
#
# Cases need the network (actions come from GitHub, tools from their vendors). Set
# RBIT_RUNNER to the binary to test; it defaults to target/debug/rbit-runner.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runner="${RBIT_RUNNER:-$root/target/debug/rbit-runner}"
[[ -x "$runner" || -x "$runner.exe" ]] || { echo "build rbit-runner first (RBIT_RUNNER=$runner)" >&2; exit 2; }
runner="$(cd "$(dirname "$runner")" && pwd)/$(basename "$runner")"
split=all
backend=host
cases=()
while (($#)); do
  case "$1" in
    --split) split="$2"; shift 2 ;;
    --backend) backend="$2"; shift 2 ;;
    *) cases+=("$1"); shift ;;
  esac
done
((${#cases[@]})) || mapfile -t cases < <(cd "$here/cases" && ls -d */ | tr -d /)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# A runner.toml to run with (its labels' images, `[actions]`); the default runs every
# label in docker.io/library/ubuntu:24.04, which lacks much of what GitHub's machines have.
[[ -n "${EVAL_RUNNER_TOML:-}" ]] && cp "$EVAL_RUNNER_TOML" "$work/runner.toml"
platform=host
[[ "$backend" == host && "$(uname -s)" == MINGW* ]] && platform=windows-host
[[ "$backend" == container ]] && platform=container
passed=0
failed=0
skipped=0
failures=()
for name in "${cases[@]}"; do
  expect="$here/cases/$name/expect.txt"
  case_split="$(sed -n 's/^split: *//p' "$expect")"
  [[ "$split" == all || "$split" == "$case_split" ]] || continue
  skip=false
  for token in $(sed -n 's/^skip: *//p' "$expect" | sed 's/#.*//'); do
    [[ "$token" == "$platform" ]] && skip=true
  done
  if $skip; then
    skipped=$((skipped + 1))
    echo "skip  $name ($case_split)"
    continue
  fi
  repo="$work/$name"
  cp -r "$here/cases/$name" "$repo"
  rm "$repo/expect.txt"
  (cd "$repo" && git init -q && git add -A && git -c user.name=eval -c user.email=eval@example.com commit -qm case) >/dev/null
  log="$work/$name.log"
  (cd "$repo" && timeout 900 "$runner" -c "$work/runner.toml" exec --backend "$backend" .github/workflows/ci.yml) >"$log" 2>&1
  if grep -q "^  success " "$log" && ! grep -q "^  failure \|^  cancelled " "$log"; then actual=success; else actual=failure; fi
  problems=()
  wanted="$(sed -n 's/^result: *//p' "$expect")"
  [[ "$actual" == "$wanted" ]] || problems+=("result $actual, expected $wanted")
  while IFS= read -r needle; do
    grep -qF -- "$needle" "$log" || problems+=("missing: $needle")
  done < <(sed -n 's/^contains: //p' "$expect")
  while IFS= read -r needle; do
    ! grep -qF -- "$needle" "$log" || problems+=("present: $needle")
  done < <(sed -n 's/^lacks: //p' "$expect")
  if ((${#problems[@]})); then
    failed=$((failed + 1))
    failures+=("$name")
    echo "FAIL  $name ($case_split)"
    printf '        %s\n' "${problems[@]}"
    cp "$log" "${EVAL_LOGS:-$work}/$name.log" 2>/dev/null || true
  else
    passed=$((passed + 1))
    echo "pass  $name ($case_split)"
  fi
done
echo "$passed passed, $failed failed, $skipped skipped (split: $split, backend: $backend)"
((failed == 0))
