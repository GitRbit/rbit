# rbit project handoff

This page is a short orientation for contributors working from `main` as of
2026-11-13. It replaces the 2026-10-25 handoff, whose branch, CI and
pre-release instructions no longer described the public project.

## Current baseline

`main` declares **0.2.2**; **0.2.1** is the latest published release shown on rbit.sh.
The [changelog](../CHANGELOG.md) records the prepared 0.2.2 changes and upgrade notes
for published releases. The [README](../README.md)
describes the product, the [roadmap](../ROADMAP.md) separates available work from
ideas, and the [architecture guide](ARCHITECTURE.md) maps the current crates and
runtime boundaries. The public repository is
[GitRbit/rbit](https://github.com/GitRbit/rbit).

rbit now includes Git hosting, collaboration, Actions with its own separate runner,
wikis, localized UI, the command-line client and agent-facing CI and merge workflows.
Start with the corresponding guide rather than an old milestone contract:

| Work | Current guide |
| --- | --- |
| Build, test, contribute and release | [CONTRIBUTING.md](../CONTRIBUTING.md) |
| Deploy, back up, restore or upgrade | [deployment.md](deployment.md) |
| Configure Actions and runners | [actions.md](actions.md) |
| Use the CLI or agent API | [cli.md](cli.md), [agent-guide.md](agent-guide.md) |
| Understand source boundaries | [ARCHITECTURE.md](ARCHITECTURE.md) |

The [ground-up rewrite plan](design/ground-up-rewrite-plan.md),
[acceptance ledger](design/rewrite/feature-ledger.json) and
[progress record](design/rewrite/progress.md) describe the modularization and its
validation history. Their pinned 0.1.5 baseline and 0.2.0 target are historical
contracts, not the version to build or publish from this checkout. Other files under
`design/` are similarly useful for decision history; check the implementation and
current guides before treating a proposal as shipped behavior.

## Working on a change

1. Start from an up-to-date `main` in an isolated worktree. Keep unrelated changes
   out of the branch and use a focused commit for each coherent change.
2. Check the feature's current implementation and tests. New behavior may live in a
   feature crate while compatibility adapters remain in the original crates.
3. Update the public guide and changelog when behavior or operator requirements
   change. Do not present work on a draft branch or in a design contract as released.
4. Run the applicable checks in [CONTRIBUTING.md](../CONTRIBUTING.md) and use hosted
   `CI required` for the final branch revision. The [CI check guide](ci-required-check.md)
   explains that aggregate; a missing or skipped result is not a pass.

The release checklist in [CONTRIBUTING.md](../CONTRIBUTING.md#preparing-a-release)
is the current publication gate. Keep the database, bare repositories and required
secret keys together when planning a restore; see
[backup and restore](deployment.md#backups) before changing storage or migrations.
