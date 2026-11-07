//! Original parser diagnostics fixtures; assertions and source samples retained.
use super::super::{MAX_DISPATCH_INPUTS, parse};
use super::support::{CI, assert_errors, job_err, parse_err};
use crate::error::{ErrorKind, Location};
use crate::limits::{Deadline, Limits};

#[test]
fn errors_point_at_the_problem() {
    let cases: &[(&str, &str, (u32, u32))] = &[
        ("jobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "no `on` section", (1, 1)),
        ("on: push\n", "no `jobs` section", (1, 1)),
        ("on: push\njobs: {}\n", "at least one job", (2, 1)),
        ("- a\n", "must be a mapping", (1, 1)),
        ("on: push\njobs:\n  1st: {runs-on: x, steps: [{run: x}]}\n", "job id `1st`", (3, 3)),
        ("on: push\njobs:\n  a b: {runs-on: x, steps: [{run: x}]}\n", "job id `a b`", (3, 3)),
        ("on: push\njobs:\n  a: {steps: []}\n", "needs `runs-on`", (3, 3)),
        ("on: push\njobs:\n  a: {runs-on: []}\n", "at least one label", (3, 16)),
        ("on: push\njobs:\n  a: {runs-on: {group: big}}\n", "runner groups", (3, 16)),
        ("on: push\njobs:\n  a: {uses: ./x.txt}\n", "must be `./path/to/workflow.yml` or `owner/repo/path/to/workflow.yml@ref`", (3, 7)),
        ("on: push\njobs:\n  a: {steps: [{run: x}], runs-on: x, needs: b}\n", "needs `b`, which does not exist", (3, 3)),
        ("on: push\njobs:\n  a: {steps: [{run: x}], runs-on: x, needs: a}\n", "needs itself", (3, 3)),
        (
            "on: push\njobs:\n  a: {steps: [{run: x}], runs-on: x, needs: b}\n  b: {steps: [{run: x}], runs-on: x, needs: c}\n  c: {steps: [{run: x}], runs-on: x, needs: a}\n",
            "cycle",
            (3, 3),
        ),
        (
            "on:\n  push:\n    branches: [a]\n    branches-ignore: [b]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "both `branches`",
            (3, 5),
        ),
        ("on:\n  push:\n    branches: ['[]']\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "invalid pattern", (3, 15)),
        ("on:\n  push:\n    types: [created]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "does not support `types`", (3, 5)),
        ("on:\n  pull_request:\n    types: [opend]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`opend` is not", (3, 12)),
        ("on:\n  schedule: '* * * * *'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "list of `cron`", (2, 13)),
        ("on:\n  schedule:\n    - crn: x\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`cron` string", (3, 7)),
        ("on:\n  schedule:\n    - cron: '61 * * * *'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "between 0 and 59", (3, 13)),
        (
            "on:\n  schedule:\n    - cron: '* * * * *'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "more often than every 5 minutes",
            (3, 13),
        ),
        ("on: push\nconcurrency: {cancel-in-progress: true}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "needs a `group`", (2, 14)),
        (
            "on: push\nconcurrency: {group: g, cancel-in-progress: maybe}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "true, false or an expression",
            (2, 45),
        ),
        ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: linux}\n", "must be a mapping of lists, or an expression", (5, 24)),
        (
            "on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: ['${{ matrix.x }}']}}\n",
            "the `matrix` context is not available here",
            (5, 30),
        ),
        ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: linux}}\n", "must be a list", (5, 29)),
        (
            "on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: [a], exclude: [{arch: x}]}}\n",
            "`arch` is not a matrix dimension",
            (5, 45),
        ),
        ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {}}\n", "a dimension or an `include`", (5, 24)),
        (
            "on:\n  workflow_dispatch:\n    inputs:\n      x: {type: choice}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "needs `options`",
            (4, 10),
        ),
        (
            "on:\n  workflow_dispatch:\n    inputs:\n      x: {type: choice, options: [a], default: b}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "one of its options",
            (4, 10),
        ),
        (
            "on:\n  workflow_dispatch:\n    inputs:\n      x: {type: file}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "not an input type",
            (4, 10),
        ),
        ("on:\n  workflow_dispatch:\n    inputs:\n      'a b': {}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "input names", (4, 7)),
    ];
    for (source, message, (line, column)) in cases {
        let error = parse_err(source);
        assert!(error.message.contains(message), "{source}: {error}");
        assert_eq!(error.at, Some(Location { line: *line, column: *column }), "{source}: {error}");
        assert!(matches!(error.kind, ErrorKind::Parse), "{source}");
    }
}

#[test]
fn limits_are_enforced() {
    let limits = Limits { max_bytes: 64, ..Limits::default() };
    let error = parse(&[b' '; 65], &limits, &Deadline::NONE).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Limit);
    assert_eq!(parse(b"\xff", &Limits::default(), &Deadline::NONE).unwrap_err().kind, ErrorKind::Parse);
    let jobs = (0..=Limits::default().max_jobs)
        .map(|index| format!("  j{index}: {{runs-on: x, steps: [{{run: x}}]}}\n"))
        .collect::<Vec<_>>()
        .concat();
    assert_eq!(parse_err(&format!("on: push\njobs:\n{jobs}")).kind, ErrorKind::Limit);
    let inputs = (0..=MAX_DISPATCH_INPUTS).map(|index| format!("      i{index}: {{}}\n")).collect::<Vec<_>>().concat();
    let source = format!("on:\n  workflow_dispatch:\n    inputs:\n{inputs}jobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n");
    assert_eq!(parse_err(&source).kind, ErrorKind::Limit);
    let expired = Deadline::at(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1)).unwrap());
    assert_eq!(parse(CI.as_bytes(), &Limits::default(), &expired).unwrap_err().kind, ErrorKind::Timeout);
}

#[test]
#[expect(clippy::too_many_lines, reason = "every level's unknown and unsupported keys, in order")]
fn unknown_and_unsupported_keys_are_errors() {
    let workflow_level: &[(&str, &str, (u32, u32))] = &[
        ("nmae: x\non: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "a workflow does not support `nmae`", (1, 1)),
        (
            "on: {workflow_call: {inputs: {a: {type: text}}}}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "`on.workflow_call.inputs.a.type` must be boolean, number or string",
            (1, 34),
        ),
        ("on: [push, pushh]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`on.pushh` is not an event", (1, 5)),
        (
            "on: push\ndefaults: {run: {shel: bash}}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "`defaults.run` does not support `shel`",
            (2, 18),
        ),
        ("on: push\ndefaults: {shell: bash}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`defaults` does not support `shell`", (2, 12)),
        (
            "on: push\nconcurrency: {group: g, cancel: true}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
            "does not support `cancel`",
            (2, 25),
        ),
        ("on: push\npermissions: {content: read}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`content` is not a permission", (2, 15)),
    ];
    for (source, message, (line, column)) in workflow_level {
        let error = parse_err(source);
        assert!(error.message.contains(message), "{source}: {error}");
        assert_eq!(error.at, Some(Location { line: *line, column: *column }), "{source}: {error}");
    }
    let job_level: &[(&str, &str, (u32, u32))] = &[
        ("    runs_on: x\n    steps: [{run: x}]\n", "`jobs.a` does not support `runs_on`; it supports name, needs, if", (4, 5)),
        ("    runs-on: x\n    environment: {url: x}\n    steps: [{run: x}]\n", "`jobs.a.environment` needs a `name`", (5, 18)),
        ("    runs-on: x\n    environment: ''\n    steps: [{run: x}]\n", "`jobs.a.environment` must name an environment", (5, 18)),
        (
            "    runs-on: x\n    environment: {name: p, reviewers: x}\n    steps: [{run: x}]\n",
            "`jobs.a.environment` does not support `reviewers`",
            (5, 28),
        ),
        ("    runs-on: x\n    environment: ${{ steps.a.outputs.b }}\n    steps: [{run: x}]\n", "steps", (5, 18)),
        (
            "    uses: o/r/.github/workflows/w.yml@v1\n    runs-on: x\n",
            "`jobs.a.runs-on` is not supported by rbit: the called workflow's jobs",
            (5, 5),
        ),
        ("    uses: ./w.yml\n    concurrency: g\n", "`jobs.a.concurrency` is not supported by rbit on a job that calls a workflow", (5, 5)),
        ("    uses: ./w.yml\n    with: {a: '${{ steps.x.outputs.y }}'}\n", "the `steps` context is not available here", (5, 15)),
        (
            "    runs-on: x\n    secrets: inherit\n    steps: [{run: x}]\n",
            "`jobs.a.secrets` is not supported by rbit: only a job that calls a workflow",
            (5, 5),
        ),
        (
            "    runs-on: x\n    with: {A: b}\n    steps: [{run: x}]\n",
            "`jobs.a.with` is not supported by rbit: only a job that calls a workflow",
            (5, 5),
        ),
        (
            "    runs-on: x\n    strategy: {fail_fast: false}\n    steps: [{run: x}]\n",
            "`jobs.a.strategy` does not support `fail_fast`",
            (5, 16),
        ),
        (
            "    runs-on: x\n    container: {image: a, options: --privileged}\n    steps: [{run: x}]\n",
            "`jobs.a.container.options`: `--privileged` is not an option rbit passes",
            (5, 36),
        ),
        (
            "    runs-on: x\n    container: {image: a, volumes: [/x]}\n    steps: [{run: x}]\n",
            "`jobs.a.container.volumes`: `/x` must be `name:/path`",
            (5, 36),
        ),
        (
            "    runs-on: x\n    container: {image: a, credentials: {user: u}}\n    steps: [{run: x}]\n",
            "`jobs.a.container.credentials` does not support `user`",
            (5, 41),
        ),
        ("    runs-on: x\n    container: {image: a, credentials: {username: u}}\n    steps: [{run: x}]\n", "needs `password`", (5, 40)),
        ("    runs-on: x\n    container: {env: {}}\n    steps: [{run: x}]\n", "`jobs.a.container` needs an `image`", (5, 16)),
        (
            "    runs-on: x\n    services: {db: {image: pg, options: --x}}\n    steps: [{run: x}]\n",
            "`jobs.a.services.db.options`: `--x` is not an option rbit passes",
            (5, 41),
        ),
        (
            "    runs-on: x\n    services: {db: {image: pg, volumes: [/x]}}\n    steps: [{run: x}]\n",
            "`jobs.a.services.db.volumes`: `/x` must be `name:/path`",
            (5, 41),
        ),
        (
            "    runs-on: x\n    services: {db: {image: pg, ports: [5432]}}\n    steps: [{run: x}]\n",
            "`jobs.a.services.db.ports` is not supported by rbit",
            (5, 32),
        ),
        ("    runs-on: x\n    services: {db: {env: {}}}\n    steps: [{run: x}]\n", "`jobs.a.services.db` needs an `image`", (5, 20)),
        ("    runs-on: x\n    services: {1db: {image: pg}}\n    steps: [{run: x}]\n", "a service id must start", (5, 16)),
        (
            "    runs-on: x\n    defaults: {run: {shell: bash, cwd: x}}\n    steps: [{run: x}]\n",
            "`jobs.a.defaults.run` does not support `cwd`",
            (5, 35),
        ),
        ("    runs-on: {labels: [x], size: big}\n    steps: [{run: x}]\n", "`jobs.a.runs-on` does not support `size`", (4, 28)),
        ("    runs-on: x\n    steps:\n      - run: x\n        uses_: y\n", "`jobs.a.steps[0]` does not support `uses_`", (7, 9)),
        ("    runs-on: x\n    env: {A=B: x}\n    steps: [{run: x}]\n", "a variable name", (5, 11)),
        ("    runs-on: x\n    env: '${{ fromJSON(vars.ENV) }}'\n    steps: [{run: x}]\n", "`jobs.a.env` must be a mapping", (5, 10)),
        ("    runs-on: x\n    outputs: {1x: y}\n    steps: [{run: x}]\n", "output name `1x`", (5, 15)),
        ("    runs-on: x\n    needs: [b, b]\n    steps: [{run: x}]\n  b: {runs-on: x, steps: [{run: x}]}\n", "names `b` twice", (5, 12)),
    ];
    assert_errors(job_level.iter().map(|(job, message, at)| (*job, job_err(job), *message, *at)));
}
