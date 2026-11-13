# Agent loop: a runner that keeps up (cache notices, prompt-free push, queue position)

Status: built. Work packages 2.4, 2.5 and 2.6 of the agent loop (2.1 to 2.3 have their own
contracts: `agent-supersede.md`, `agent-applicable-workflows.md`, `agent-exclusive-labels.md`).

## 2.4 Cache notices

- Product: every cache step says what it bought as a `##[notice]` line: `cache hit` or
  `cache partial hit` with the key, files, bytes and seconds; `cache miss`; `cache saved`
  with the bytes. Notices are stored as the job's annotations, so `GET …/jobs/{id}/
  annotations` shows them; nothing new to configure, and it works on container runners.
- rbit's own CI builds cold: every job compiles from nothing, with no build directory kept
  between jobs and no build cache, so a result never depends on what an earlier job left
  behind. A persistent `CARGO_BUILD_BUILD_DIR` on the runner was tried and removed on
  2026-11-12 at the owner's request; quick iteration uses scratch runs of a few jobs
  (`rbit ci run --jobs`) instead.

## 2.5 Push without a prompt

`rbit auth setup-git --hostname URL` installs `rbit auth git-credential` as Git's credential
helper for that URL (other helpers for it reset). `get` answers from `RBIT_TOKEN`,
`GH_TOKEN` or the stored login; without one it answers `quit=1`, so Git stops at once
instead of prompting. A refused token fails the push with `Authentication failed`. The
large-push corruption seen once on a self-hosted instance was not reproduced; it stays an
open issue on the instance.

## 2.6 Queue position

A waiting job carries `queue`: `position` among waiting jobs with the same label set
(oldest first), `ahead`, and `expected_start_seconds` = ahead × the mean run time of the
last 50 finished jobs with that label set (one runner assumed). `rbit status` and the watch
show the position of the commit's front job.

## Tests

Unit: the credential helper's request parsing and answers. Runner: cache messages in the
job tests. End to end: queue position 2 and `ahead` 1 for a branch's job behind `main`'s.

## Left out

The expected start does not count the runners serving a label set.
