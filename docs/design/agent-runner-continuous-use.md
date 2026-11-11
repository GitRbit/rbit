# Agent loop: a runner that keeps up (warm builds, prompt-free push, queue position)

Status: built. Work packages 2.4, 2.5 and 2.6 of the agent loop (2.1 to 2.3 have their own
contracts: `agent-supersede.md`, `agent-applicable-workflows.md`, `agent-exclusive-labels.md`).

## 2.4 Warm builds

- Product: every cache step says what it bought as a `##[notice]` line: `cache hit` or
  `cache partial hit` with the key, files, bytes and seconds; `cache miss`; `cache saved`
  with the bytes. Notices are stored as the job's annotations, so `GET …/jobs/{id}/
  annotations` shows them; nothing new to configure, and it works on container runners.
- rbit's own CI: measured cold on one 16-core host runner (run 11): contracts 3.7 min,
  clippy 2.9, tests 6.6, release 9.6 — every job compiles from nothing because a host
  runner deletes the job directory. The cheapest fix that works is outside the product: the
  self-hosted runner's environment sets `CARGO_BUILD_BUILD_DIR` to a directory that
  persists, so cargo's intermediate outputs stay warm while each job's final binaries still
  land in its own `target/`. The workflow does not change and still runs cold anywhere else.
  Uploading a multi-gigabyte target directory to the server's cache per job was rejected:
  it would cost more transfer than it saves on one machine.

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

The expected start does not count the runners serving a label set; pruning the warm build
directory is the runner operator's job.
