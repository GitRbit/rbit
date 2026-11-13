# Agent loop: superseded runs

Status: built. Work package 2.1. An agent pushes often; without help every push runs the
whole suite, and on a busy runner the newest result arrives last. A workflow could ask for
`concurrency` with `cancel-in-progress`, but nothing said "superseded by", only "cancelled".

## As built

- A repository setting, off by default: `GET`/`PUT …/actions/supersede`
  (`{"enabled": bool}`; administrators, `admin` scope). Migration
  `0076_actions_supersede.sql`.
- When on, a `push` run on a branch other than the default branch whose workflow declares
  no `concurrency` joins the implicit group `rbit-supersede/<workflow path>/<ref>` with
  `cancel-in-progress`. The existing group logic (one decision at a time) cancels the
  older unfinished runs. The default branch is never superseded.
- Every run cancelled because a newer run entered its group (the setting, or a workflow's
  own `cancel-in-progress`) gets a supersession row naming the newer commit; its tasks that
  were running get the termination reason `superseded`. Runs carry `superseded_by` in the
  API. `rbit status` and the watch read `superseded` when every run of the commit was
  superseded, with `superseded_by`.

## Tests

End to end (`agent_supersede.rs`): with the setting off two pushes leave two queued runs;
on, the older run reads cancelled with `superseded_by` the newer commit, the newest stays
queued, the watch of the older commit reads `superseded`; pushes to the default branch keep
every run.

## Left out

A setting page in the web interface (the API is the interface for now). Pull request runs
already supersede through the GitHub-shaped `concurrency` that most workflows declare.
