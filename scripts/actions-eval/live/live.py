#!/usr/bin/env python3
"""The live Actions eval: cases run on a real rbit instance with real runners.

    python live.py [--split train|test|all] [--out DIR] [--regrade DIR] [case ...]

Each case is a directory of `cases/` with `case.json` and the files of one or more
repositories. The harness has two phases, kept apart so a grader can be re-run on the same
output:

1. observe: create fresh private repositories, push their files, trigger, wait until every
   run is finished and nothing new starts, then record the runs, jobs, job logs, artifacts
   and the HTTP responses the case asks for in `<out>/<case>/observed.json`.
2. grade: a pure function of `observed.json` and the case's `expect` list. Every case is
   graded twice and the two verdicts must agree; a disagreement is a grader bug.

Environment: RBIT_URL (default http://localhost:3000) and RBIT_TOKEN, an admin-scope token
(or RBIT_TOKEN_FILE, a file whose first `rbit_...` word is the token). Repositories are
created under the token's owner. The instance needs a runner with a `self-hosted` host label
and an `ubuntu-latest` container label; the container security cases need a Linux runner
with an `eval-linux` container label and an `eval-linux-host` host label on the same
runner (on another system their runs wait, and are reported as plumbing). Shell checks run
RBIT_CLI (the `rbit` binary) and RBIT_RUNNER_EXE (`rbit-runner`). Only the standard library
is used.

`case.json`:

    {
      "feature": "zip",                 # the item it measures; every feature has cases in
      "split": "train",                 #   both splits
      "why": "what makes it hard",      # optional
      "repos": {"main": {"files": "repo", "public": false}},
      "setup": [{"http": {...}}],       # optional requests before the push
      "trigger": "push" | {"dispatch": "ci.yml", "repo": "main", "ref": "main", "inputs": {}},
      "wait": {"runs": 1, "settle": 15, "timeout": 900},
      "observe": [{"id": "x", "http": {...}} | {"id": "x", "artifact": {...}} | {"id": "x", "shell": "..."}],
      "expect": [...]
    }

An `http` request is {"method", "path", "auth": "token"|"none", "body", "api": true}; with
`api: false` the path is outside `/api/v1`. Strings may use {owner}, {repo} (the `main`
repository's name), {repo:NAME} and {url}.
"""

from __future__ import annotations

import argparse
import base64
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CASES = HERE / "cases"
URL = os.environ.get("RBIT_URL", "http://localhost:3000").rstrip("/")
# The token's owner; asked from the instance unless RBIT_USER names it.
USER = os.environ.get("RBIT_USER", "")


def token() -> str:
    value = os.environ.get("RBIT_TOKEN")
    if value:
        return value
    path = os.environ.get("RBIT_TOKEN_FILE")
    if path:
        found = re.search(r"rbit_[A-Za-z0-9]+", Path(path).read_text())
        if found:
            return found.group(0)
    sys.exit("set RBIT_TOKEN or RBIT_TOKEN_FILE")


TOKEN = ""

# ------------------------------------------------------------------------------ HTTP


def request(method: str, path: str, *, auth: str = "token", body=None, api: bool = True, raw: bool = False, headers=None):
    """(status, headers as a lower-cased dict, body as JSON, text or bytes)."""
    url = f"{URL}/api/v1{path}" if api else f"{URL}{path}"
    data = None
    sent = {"Accept": "application/json"}
    if body is not None:
        data = json.dumps(body).encode()
        sent["Content-Type"] = "application/json"
    if auth == "token":
        sent["Authorization"] = f"Bearer {TOKEN}"
    sent.update(headers or {})
    # Rate limiting is the instance protecting itself, not an answer: wait and ask again.
    for attempt in range(6):
        req = urllib.request.Request(url, data=data, method=method, headers=sent)
        try:
            with urllib.request.urlopen(req, timeout=60) as response:
                status, got, content = response.status, response.headers, response.read()
        except urllib.error.HTTPError as error:
            status, got, content = error.code, error.headers, error.read()
        if status != 429:
            break
        retry = got.get("Retry-After", "")
        time.sleep(min(int(retry) if retry.isdigit() else 2 ** attempt, 30))
    lowered = {key.lower(): value for key, value in got.items()}
    if raw:
        return status, lowered, content
    kind = lowered.get("content-type", "")
    if "json" in kind:
        try:
            return status, lowered, json.loads(content or b"null")
        except ValueError:
            pass
    return status, lowered, content.decode("utf-8", "replace")


def substitute(value, names: dict):
    if isinstance(value, str):
        for key, real in names.items():
            value = value.replace("{" + key + "}", real)
        return value
    if isinstance(value, list):
        return [substitute(item, names) for item in value]
    if isinstance(value, dict):
        return {key: substitute(item, names) for key, item in value.items()}
    return value


# ------------------------------------------------------------------------------ observe


def git(*args, cwd):
    basic = base64.b64encode(f"{USER}:{TOKEN}".encode()).decode()
    command = ["git", "-c", f"http.extraHeader=Authorization: Basic {basic}", "-c", "user.name=eval", "-c", "user.email=eval@example.com", *args]
    done = subprocess.run(command, cwd=cwd, capture_output=True, text=True)
    if done.returncode:
        raise RuntimeError(f"git {' '.join(args)}: {done.stderr.strip()}")
    return done.stdout


def create_repo(name: str, public: bool):
    status, _, body = request("POST", "/user/repos", body={"name": name, "private": not public, "auto_init": False})
    if status not in (200, 201):
        raise RuntimeError(f"cannot create {name}: {status} {body}")


def push(case_dir: Path, files: str, name: str, work: Path, names: dict):
    repo = work / name
    shutil.copytree(case_dir / files, repo)
    # Placeholders in the repository's files ({owner}, {repo}, {repo:NAME}) name this run's
    # repositories.
    for path in repo.rglob("*"):
        if path.is_file():
            try:
                text = path.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                continue
            filled = substitute(text, names)
            if filled != text:
                path.write_text(filled, encoding="utf-8", newline=chr(10))
    git("init", "-q", "-b", "main", cwd=repo)
    git("add", "-A", cwd=repo)
    git("commit", "-qm", "eval case", cwd=repo)
    git("push", "-q", f"{URL}/{USER}/{name}.git", "main", cwd=repo)


def runs_of(name: str):
    status, _, body = request("GET", f"/repos/{USER}/{name}/actions/runs?per_page=100")
    return body.get("workflow_runs", []) if status == 200 and isinstance(body, dict) else []


def wait_for_runs(names: list[str], wait: dict) -> str:
    """"settled" when every run of `names` finished, at least `runs` exist, and nothing
    changed for `settle` seconds; otherwise why it stopped waiting: no run started within
    `start` seconds, a run stayed queued for `queued` seconds (no runner takes it), or the
    `timeout` passed. The harness never waits longer than that."""
    want, settle = wait.get("runs", 1), wait.get("settle", 10)
    began = time.time()
    deadline = began + wait.get("timeout", 300)
    quiet_since, last, queued_since = None, None, {}
    while time.time() < deadline:
        runs = [run for name in names for run in runs_of(name)]
        state = sorted((run["id"], run["status"]) for run in runs)
        done = len(runs) >= want and all(run["status"] == "completed" for run in runs)
        if done and state == last:
            quiet_since = quiet_since or time.time()
            if time.time() - quiet_since >= settle:
                return "settled"
        else:
            quiet_since = None
        if len(runs) < want and time.time() - began > wait.get("start", 60) and all(run["status"] == "completed" for run in runs):
            return f"only {len(runs)} of {want} runs started within {wait.get('start', 60)}s"
        now = time.time()
        for run in runs:
            if run["status"] == "queued":
                since = queued_since.setdefault(run["id"], now)
                if now - since > wait.get("queued", 180):
                    return f"run {run['id']} stayed queued for {wait.get('queued', 180)}s: no runner took it"
            else:
                queued_since.pop(run["id"], None)
        last = state
        time.sleep(5)
    return f"timed out after {wait.get('timeout', 300)}s"


def cancel_unfinished(names):
    """Cancels what a case left running, so a stuck case costs the runners nothing."""
    for name in names:
        for run in runs_of(name):
            if run["status"] != "completed":
                request("POST", f"/repos/{USER}/{name}/actions/runs/{run['id']}/cancel")


def archive_entries(content: bytes, kind: str) -> dict:
    """Paths to text (or "<binary>") of a zip or tar.gz."""
    files = {}
    if kind == "zip":
        with zipfile.ZipFile(io.BytesIO(content)) as archive:
            for info in archive.infolist():
                if not info.is_dir():
                    files[info.filename] = text_of(archive.read(info))
                    mode = info.external_attr >> 16
                    if mode & 0o6000:
                        files[info.filename + "#setid"] = "yes"
    else:
        with tarfile.open(fileobj=io.BytesIO(content), mode="r:gz") as archive:
            for member in archive.getmembers():
                if member.isfile():
                    files[member.name.removeprefix("./")] = text_of(archive.extractfile(member).read())
                    if member.mode & 0o6000:
                        files[member.name.removeprefix("./") + "#setid"] = "yes"
    return files


def text_of(data: bytes) -> str:
    try:
        return data.decode("utf-8")[:20000]
    except UnicodeDecodeError:
        return "<binary>"


def observe(case_name: str, case: dict, out: Path) -> dict:
    case_dir = CASES / case_name
    stamp = time.strftime("%m%d%H%M%S")
    # Shell checks run the client and runner under test: RBIT_CLI and RBIT_RUNNER_EXE name
    # them, and each case gets a scratch directory.
    scratch = Path(tempfile.mkdtemp(prefix=f"rbit-eval-{case_name}-"))
    names = {
        "owner": USER,
        "url": URL,
        "cli": os.environ.get("RBIT_CLI", "rbit"),
        "runner": os.environ.get("RBIT_RUNNER_EXE", "rbit-runner"),
        "tmp": str(scratch),
        "case": str(case_dir),
    }
    repos = {}
    for key, spec in case.get("repos", {}).items():
        repos[key] = f"ev-{case_name}-{key}-{stamp}"[:90]
        names[f"repo:{key}"] = repos[key]
    names["repo"] = repos.get("main", "")
    observed = {"case": case_name, "repos": repos, "names": names, "runs": [], "observations": {}, "errors": []}
    work = Path(tempfile.mkdtemp(prefix="rbit-eval-"))
    try:
        for key, spec in case.get("repos", {}).items():
            create_repo(repos[key], spec.get("public", False))
        for step in substitute(case.get("setup", []), names):
            if "http" in step:
                spec = step["http"]
                status, _, body = request(spec.get("method", "GET"), spec["path"], auth=spec.get("auth", "token"), body=spec.get("body"), api=spec.get("api", True))
                if status >= 300 and not step.get("may_fail"):
                    observed["errors"].append(f"setup {spec['path']}: {status} {str(body)[:300]}")
            if "secret" in step:
                spec = step["secret"]
                request("PUT", f"/repos/{USER}/{spec['repo']}/actions/secrets/{spec['name']}", body={"value": spec["value"]})
        # Repositories other than `main` first: `main`'s push is what starts runs that use them.
        for key in sorted(case.get("repos", {}), key=lambda key: key == "main"):
            push(case_dir, case["repos"][key]["files"], repos[key], work, names)
        trigger = substitute(case.get("trigger", "push"), names)
        if isinstance(trigger, dict) and "dispatch" in trigger:
            repo = repos[trigger.get("repo", "main")]
            status, _, body = request("POST", f"/repos/{USER}/{repo}/actions/workflows/{trigger['dispatch']}/dispatches", body={"ref": trigger.get("ref", "main"), "inputs": trigger.get("inputs", {})})
            if status != 204:
                observed["errors"].append(f"dispatch: {status} {body}")
        wait = case.get("wait", {"runs": 1})
        if wait.get("runs", 1) > 0 or wait.get("settle"):
            observed["settled"] = wait_for_runs(list(repos.values()), wait)
            if observed["settled"] != "settled":
                # Runs that never started are what a missing feature looks like; a run no
                # runner takes, or one that never ends, is the environment's problem.
                kind = "plumbing" if "queued" in observed["settled"] or "timed out" in observed["settled"] else "waited"
                observed["errors"].append(f"{kind}: {observed['settled']}")
                cancel_unfinished(repos.values())
        for key, repo in repos.items():
            for run in runs_of(repo):
                run["repo_key"] = key
                status, _, jobs = request("GET", f"/repos/{USER}/{repo}/actions/runs/{run['id']}/jobs?per_page=100")
                run["jobs"] = jobs.get("jobs", []) if isinstance(jobs, dict) else []
                for job in run["jobs"]:
                    _, _, log = request("GET", f"/repos/{USER}/{repo}/actions/jobs/{job['id']}/logs")
                    job["log"] = log if isinstance(log, str) else json.dumps(log)
                _, _, page = request("GET", f"/{USER}/{repo}/actions/runs/{run['run_number']}", api=False, headers={"Accept": "text/html"})
                run["page"] = page if isinstance(page, str) else ""
                _, _, artifacts = request("GET", f"/repos/{USER}/{repo}/actions/runs/{run['id']}/artifacts")
                run["artifacts"] = artifacts.get("artifacts", []) if isinstance(artifacts, dict) else []
                observed["runs"].append(run)
        for item in substitute(case.get("observe", []), names):
            observed["observations"][item["id"]] = observe_one(item, observed, repos)
    except Exception as error:  # noqa: BLE001 - recorded, graded as a failure
        import traceback

        where = [frame for frame in traceback.extract_tb(error.__traceback__) if frame.filename == __file__][-1]
        observed["errors"].append(f"plumbing: {type(error).__name__}: {error} (at {where.name}:{where.lineno})")
    finally:
        shutil.rmtree(work, ignore_errors=True)
        shutil.rmtree(scratch, ignore_errors=True)
    print(f"observed {case_name}: {observed.get('settled', 'no wait')}, {len(observed['runs'])} runs", flush=True)
    (out / case_name).mkdir(parents=True, exist_ok=True)
    (out / case_name / "observed.json").write_text(json.dumps(observed, indent=1))
    return observed


def observe_one(item: dict, observed: dict, repos: dict) -> dict:
    if "http" in item:
        spec = item["http"]
        status, headers, body = request(spec.get("method", "GET"), spec["path"], auth=spec.get("auth", "token"), body=spec.get("body"), api=spec.get("api", True), raw=True, headers=spec.get("headers"))
        text = text_of(body)
        parsed = None
        if "json" in headers.get("content-type", ""):
            try:
                parsed = json.loads(body or b"null")
            except ValueError:
                pass
        return {"status": status, "headers": headers, "body": text, "json": parsed}
    if "artifact" in item:
        spec = item["artifact"]
        repo = repos[spec.get("repo", "main")]
        runs = [run for run in observed["runs"] if run["repo_key"] == spec.get("repo", "main") and run.get("path", "").endswith(spec.get("workflow", ""))]
        for run in runs:
            for artifact in run["artifacts"]:
                if artifact["name"] == spec["name"]:
                    kind = spec.get("format", "zip")
                    status, headers, content = request("GET", f"/repos/{USER}/{repo}/actions/artifacts/{artifact['id']}/{kind}", raw=True)
                    files = archive_entries(content, kind) if status == 200 else {}
                    return {"status": status, "headers": headers, "files": files, "meta": artifact}
        return {"status": 0, "files": {}, "error": "no such artifact"}
    if "shell" in item:
        done = subprocess.run(item["shell"], shell=True, capture_output=True, text=True, timeout=item.get("timeout", 600), env={**os.environ, "RBIT_TOKEN": TOKEN})
        return {"exit": done.returncode, "stdout": done.stdout[-20000:], "stderr": done.stderr[-20000:]}
    return {"error": "unknown observation"}


# ------------------------------------------------------------------------------ grade


def lookup(value, dotted: str):
    for part in dotted.split("."):
        if isinstance(value, list) and part.isdigit():
            value = value[int(part)] if int(part) < len(value) else None
        elif isinstance(value, dict):
            value = value.get(part)
        else:
            return None
    return value


def grade(case: dict, observed: dict) -> list[str]:
    """The problems; none means the case passed. Pure: same input, same answer."""
    problems = list(observed.get("errors", []))
    runs = observed.get("runs", [])
    case = substitute(case, observed.get("names", {}))

    def runs_matching(spec):
        return [
            run
            for run in runs
            if run.get("repo_key") == spec.get("repo", "main")
            and (not spec.get("workflow") or (run.get("path") or "").endswith("/" + spec["workflow"]))
            and (not spec.get("event") or run.get("event") == spec["event"])
        ]

    for expect in case.get("expect", []):
        if "run" in expect:
            spec = expect["run"]
            matching = runs_matching(spec)
            if not matching:
                problems.append(f"no run of {spec.get('workflow')}")
            for run in matching:
                if "conclusion" in spec and run.get("conclusion") != spec["conclusion"]:
                    problems.append(f"run {spec.get('workflow')} #{run.get('run_number')}: {run.get('conclusion')}, expected {spec['conclusion']}")
                for needle in spec.get("page_contains", []):
                    if needle not in run.get("page", ""):
                        problems.append(f"run page of {spec.get('workflow')} lacks {needle!r}")
            if "count" in spec and len(matching) != spec["count"]:
                problems.append(f"{len(matching)} runs of {spec.get('workflow')}, expected {spec['count']}")
        elif "no_run" in expect:
            if runs_matching(expect["no_run"]):
                problems.append(f"{expect['no_run'].get('workflow')} ran but must not")
        elif "job" in expect:
            spec = expect["job"]
            jobs = [job for run in runs_matching(spec) for job in run.get("jobs", []) if re.fullmatch(spec.get("name", ".*"), job.get("name", ""))]
            if not jobs:
                problems.append(f"no job {spec.get('name')} in {spec.get('workflow')}")
            for job in jobs:
                if "conclusion" in spec and job.get("conclusion") != spec["conclusion"]:
                    problems.append(f"job {job.get('name')}: {job.get('conclusion')}, expected {spec['conclusion']}")
                for needle in spec.get("contains", []):
                    if needle not in job.get("log", ""):
                        problems.append(f"job {job.get('name')} log lacks {needle!r}")
                for needle in spec.get("lacks", []):
                    if needle in job.get("log", ""):
                        problems.append(f"job {job.get('name')} log has {needle!r}")
                for pattern in spec.get("matches", []):
                    if not re.search(pattern, job.get("log", "")):
                        problems.append(f"job {job.get('name')} log has no match for {pattern!r}")
        elif "obs" in expect:
            seen = observed.get("observations", {}).get(expect["obs"])
            if seen is None:
                problems.append(f"no observation {expect['obs']}")
                continue
            label = expect["obs"]
            if "status" in expect and seen.get("status") != expect["status"]:
                problems.append(f"{label}: status {seen.get('status')}, expected {expect['status']}")
            if "exit" in expect and seen.get("exit") != expect["exit"]:
                problems.append(f"{label}: exit {seen.get('exit')}, expected {expect['exit']} ({(seen.get('stderr') or '')[-300:]})")
            for name, wanted in expect.get("headers", {}).items():
                got = (seen.get("headers") or {}).get(name.lower())
                if wanted is None and got is not None:
                    problems.append(f"{label}: header {name} present")
                elif wanted is not None and (got is None or wanted.lower() not in got.lower()):
                    problems.append(f"{label}: header {name} is {got!r}, expected {wanted!r}")
            text = seen.get("body", "") + seen.get("stdout", "")
            for needle in expect.get("contains", []):
                if needle not in text:
                    problems.append(f"{label}: lacks {needle!r}")
            for needle in expect.get("lacks", []):
                if needle in text:
                    problems.append(f"{label}: has {needle!r}")
            for dotted, wanted in expect.get("json", {}).items():
                got = lookup(seen.get("json"), dotted)
                if got != wanted:
                    problems.append(f"{label}: {dotted} is {got!r}, expected {wanted!r}")
            files = seen.get("files", {})
            for name in expect.get("files", []):
                if name not in files:
                    problems.append(f"{label}: no file {name} (has {sorted(files)[:12]})")
            for name in expect.get("no_files", []):
                if name in files:
                    problems.append(f"{label}: has file {name}")
            if "exact_files" in expect and sorted(files) != sorted(expect["exact_files"]):
                problems.append(f"{label}: files {sorted(files)}, expected {sorted(expect['exact_files'])}")
            for name, needle in expect.get("file_contains", {}).items():
                if needle not in files.get(name, ""):
                    problems.append(f"{label}: {name} lacks {needle!r}")
            for name, needle in expect.get("file_lacks", {}).items():
                if needle in files.get(name, ""):
                    problems.append(f"{label}: {name} has {needle!r}")
            if expect.get("no_setid") and any(name.endswith("#setid") for name in files):
                problems.append(f"{label}: an archived file is set-user-ID or set-group-ID")
    return problems


# ------------------------------------------------------------------------------ main


def load(name: str) -> dict:
    return json.loads((CASES / name / "case.json").read_text())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--split", default="all", choices=["train", "test", "all"])
    parser.add_argument("--out", default=None)
    parser.add_argument("--regrade", default=None, help="grade the observed.json files of an earlier --out")
    parser.add_argument("--jobs", type=int, default=4, help="cases observed at once")
    parser.add_argument("cases", nargs="*")
    args = parser.parse_args()
    names = args.cases or sorted(path.name for path in CASES.iterdir() if (path / "case.json").exists())
    names = [name for name in names if args.split == "all" or load(name)["split"] == args.split]
    # Cases for one system (`"platform": "windows"`) are left out elsewhere, and said so.
    here = "windows" if os.name == "nt" else sys.platform
    elsewhere = [name for name in names if load(name).get("platform", here) != here]
    if elsewhere:
        print(f"not on this system: {', '.join(elsewhere)}")
    names = [name for name in names if name not in elsewhere]
    check_splits()
    out = Path(args.regrade or args.out or tempfile.mkdtemp(prefix="rbit-eval-out-"))
    if not args.regrade:
        global TOKEN, USER
        TOKEN = token()
        if not USER:
            status, _, me = request("GET", "/user")
            if status != 200:
                sys.exit(f"the token does not work at {URL}: {status}")
            USER = me["login"]
        from concurrent.futures import ThreadPoolExecutor

        with ThreadPoolExecutor(max_workers=args.jobs) as pool:
            list(pool.map(lambda name: observe(name, load(name), out), names))
    results = {}
    for name in names:
        path = out / name / "observed.json"
        if not path.exists():
            continue
        observed = json.loads(path.read_text())
        first, second = grade(load(name), observed), grade(load(name), json.loads(path.read_text()))
        if first != second:
            first = first + ["grader is unstable: two gradings disagree"]
        results[name] = first
    by = {"train": [0, 0], "test": [0, 0]}
    for name, problems in results.items():
        case = load(name)
        by[case["split"]][0 if not problems else 1] += 1
        print(f"{'pass' if not problems else 'FAIL'}  {name} ({case['feature']}, {case['split']})")
        for problem in problems[:8]:
            print(f"        {problem}")
    summary = {split: {"passed": counts[0], "failed": counts[1]} for split, counts in by.items()}
    features = {}
    for name, problems in results.items():
        feature = load(name)["feature"]
        features.setdefault(feature, [0, 0])[0 if not problems else 1] += 1
    (out / "summary.json").write_text(json.dumps({"splits": summary, "features": features, "results": results}, indent=1))
    for split, counts in by.items():
        total = sum(counts)
        if total:
            print(f"{split}: {counts[0]}/{total} passed")
    plumbing = sorted(name for name, problems in results.items() if any(problem.startswith("plumbing:") for problem in problems))
    if plumbing:
        print(f"plumbing failures (the environment, not rbit; run them again): {', '.join(plumbing)}")
    print(f"observations in {out}")


def check_splits():
    """Every feature must have cases in both splits, or a round can never improve both."""
    seen = {}
    for path in CASES.iterdir():
        if (path / "case.json").exists():
            case = json.loads((path / "case.json").read_text())
            seen.setdefault(case["feature"], set()).add(case["split"])
    lopsided = sorted(feature for feature, splits in seen.items() if splits != {"train", "test"})
    if lopsided:
        print(f"warning: features without both splits: {', '.join(lopsided)}", file=sys.stderr)


if __name__ == "__main__":
    main()
