# rbit documentation

These guides describe the current `main` branch. The workspace version is `0.2.2` as of
2026-11-13; check [the changelog](../CHANGELOG.md) and [releases](https://github.com/GitRbit/rbit/releases)
for what has actually shipped. Features on `main` can precede the latest published release.

## Start here

| If you want to… | Read |
| --- | --- |
| See what rbit does and try it | [README](../README.md) |
| Install, configure, back up, restore, or upgrade a server | [Deployment](deployment.md) |
| Set up workflows and runners | [Actions](actions.md) |
| Use rbit from a terminal or a coding agent | [CLI](cli.md) and [agent guide](agent-guide.md) |
| Check available features and future direction | [Roadmap](../ROADMAP.md) |
| Build or contribute | [Contributing](../CONTRIBUTING.md) |
| Understand the code and service boundaries | [Architecture](ARCHITECTURE.md) |
| Report a vulnerability | [Security policy](../SECURITY.md) |

## Focused guides

- [Repository creation](repository-creation.md), [repository configuration](repository-configuration.md),
  [pull request files](pull-request-files.md), [issue search](issue-search.md),
  [personal dashboards](personal-dashboards.md), and [discussions](discussions.md)
- [Live administration monitoring](admin-monitoring.md), [required CI checks](ci-required-check.md),
  and [upgrade rehearsal](upgrade-rehearsal.md)
- [Shared developer worktrees](development-worktrees.md), [CI test shards](ci-test-shards.md),
  and [source provenance](development-source-provenance.md)

## Engineering records

The [design directory](design/) contains plans, acceptance records, measurements, and
branch-specific progress notes. Many record decisions at a particular commit and are
preserved as evidence; their future-tense language is not a statement about current
product availability. Use the current guides above for installation and feature support.
The [rewrite plan](design/ground-up-rewrite-plan.md) and
[rewrite progress](design/rewrite/progress.md) describe that development effort at their
stated checkpoints. The [project handoff](HANDOFF.md) is a current contributor
orientation; [older handoff notes](history/handoff-2026-10-25.md) are retained as history.
