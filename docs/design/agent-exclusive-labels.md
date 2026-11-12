# Agent loop: exclusive runner labels

Status: built. Work package 2.3. A runner with `capacity` above 1 ran any jobs side by
side, so a benchmark or a timing-sensitive test could not be kept off a busy machine except
by giving it a runner of its own.

## As built

- `runner.toml`: `exclusive = true` on a `[[labels]]` entry. The runner declares the names
  in `Capabilities.exclusive_labels` (optional on the wire; an older server ignores it, an
  older runner declares none). The server checks each is one of the runner's labels and
  keeps them in the runner's stored capabilities (JSON; no migration).
- At claim time (`claim_bridge::claimable_labels`): an idle runner claims with every
  label; a runner running something claims without its exclusive labels, so no job that
  needs one can be matched; a runner running a job that needs an exclusive label claims
  nothing.
- A poll that found nothing waits for the tasks version to move. A task that ends on a
  runner with exclusive labels moves it, so the jobs that runner had to leave out while
  busy are looked at again at once (the end-to-end test caught this: without it a bench
  job queued behind two ordinary ones waited for unrelated new work). A task the sweep
  fails for a silent runner does not move it; the runner's next poll, at most a long-poll
  later, finds the job.
- Docs: `deploy/rbit-runner.example.toml`, `docs/deployment.md`.

## Tests

End to end (`agent_exclusive.rs`): a real runner with capacity 2 and labels
`self-hosted` and exclusive `bench`; a workflow with two ordinary jobs and a bench job, each
sleeping 3 s. The bench job's span never overlaps the others; the two ordinary jobs overlap.

## Left out

A web control: labels are runner configuration, edited in `runner.toml`.
