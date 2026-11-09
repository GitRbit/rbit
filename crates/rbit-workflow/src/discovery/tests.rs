//! Discovery precedence, filters, budgets and names, over an in-memory source.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::*;
use crate::error::ErrorKind;
use crate::limits::{Deadline, Limits};
use crate::plan::{GroupContext, NoCalls, plan_run};
use crate::trigger::Event;

/// Directories by path (entry name, is a file), files by path; `Err` lists fail.
#[derive(Default)]
struct Tree {
    dirs: BTreeMap<&'static str, Vec<(String, bool)>>,
    files: BTreeMap<String, Vec<u8>>,
    failing: Option<&'static str>,
    listed: Vec<&'static str>,
    read: Vec<String>,
    truncated: Vec<usize>,
}

impl Tree {
    fn with(mut self, dir: &'static str, entries: &[(&str, bool)]) -> Self {
        self.dirs.insert(dir, entries.iter().map(|(name, file)| ((*name).to_owned(), *file)).collect());
        self
    }

    fn file(mut self, path: &str, source: &str) -> Self {
        self.files.insert(path.to_owned(), source.as_bytes().to_vec());
        self
    }
}

impl WorkflowSource for Tree {
    type Error = &'static str;

    fn list(&mut self, dir: &'static str) -> Result<Listing, Self::Error> {
        self.listed.push(dir);
        if self.failing == Some(dir) {
            return Err("broken");
        }
        Ok(match self.dirs.get(dir) {
            Some(entries) => {
                Listing::Entries(entries.iter().map(|(name, is_file)| DirEntry { name: name.clone(), is_file: *is_file }).collect())
            }
            None => Listing::Missing,
        })
    }

    fn read(&mut self, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Unreadable> {
        self.read.push(path.to_owned());
        let data = self.files.get(path).ok_or(Unreadable)?;
        Ok((data.len() <= max_bytes).then(|| data.clone()))
    }

    fn truncated(&mut self, selected: &Selected) {
        self.truncated.push(selected.available);
    }
}

const PUSH: &str = "on: push\njobs:\n  build:\n    runs-on: x\n    steps: [{run: make}]\n";

#[test]
fn the_first_existing_directory_wins_even_without_workflows() {
    let mut tree = Tree::default().with(".forgejo/workflows", &[("README.md", true)]).with(".github/workflows", &[("ci.yml", true)]);
    assert_eq!(paths(&mut tree), Ok(Vec::new()));
    assert_eq!(tree.listed, [".rbit/workflows", ".forgejo/workflows"], "the GitHub directory is never listed");

    let mut tree = Tree::default().with(".rbit/workflows", &[("a.yml", true)]).with(".forgejo/workflows", &[("b.yml", true)]);
    assert_eq!(paths(&mut tree), Ok(vec![".rbit/workflows/a.yml".to_owned()]));
    assert_eq!(tree.listed, [".rbit/workflows"]);

    let mut tree = Tree::default();
    assert_eq!(paths(&mut tree), Ok(Vec::new()));
    assert_eq!(tree.listed, WORKFLOW_DIRS);
}

#[test]
fn list_errors_stop_discovery() {
    let mut tree = Tree { failing: Some(".forgejo/workflows"), ..Tree::default() }.with(".github/workflows", &[("ci.yml", true)]);
    assert_eq!(paths(&mut tree), Err("broken"));
    assert_eq!(tree.listed, [".rbit/workflows", ".forgejo/workflows"]);
}

#[test]
fn only_yaml_files_count_sorted_by_name() {
    let entries =
        [("z.yaml", true), ("a.yml", true), ("dir.yml", false), (".yml", true), ("CI.YML", true), ("notes.txt", true), ("b.yml.bak", true)];
    let mut tree = Tree::default().with(".github/workflows", &entries);
    assert_eq!(paths(&mut tree), Ok(vec![".github/workflows/a.yml".to_owned(), ".github/workflows/z.yaml".to_owned()]));
    assert_eq!(tree.truncated, Vec::<usize>::new());
    for (name, workflow) in [("ci.yml", true), ("ci.yaml", true), (".yml", false), ("ci.YML", false), ("ci", false), ("ci.yml.txt", false)]
    {
        assert_eq!(is_workflow_file_name(name), workflow, "{name}");
    }
}

#[test]
fn at_most_the_first_files_by_name_are_read() {
    let names: Vec<String> = (0..MAX_WORKFLOW_FILES + 6).rev().map(|n| format!("w{n:03}.yml")).collect();
    let entries: Vec<(&str, bool)> = names.iter().map(|name| (name.as_str(), true)).collect();
    let mut tree = Tree::default().with(".rbit/workflows", &entries);
    let found = paths(&mut tree).unwrap();
    assert_eq!(found.len(), MAX_WORKFLOW_FILES);
    assert_eq!((found[0].as_str(), found[MAX_WORKFLOW_FILES - 1].as_str()), (".rbit/workflows/w000.yml", ".rbit/workflows/w063.yml"));
    assert_eq!(tree.truncated, [MAX_WORKFLOW_FILES + 6]);
}

#[test]
fn the_step_api_settles_like_the_source_walk() {
    let mut discovery = Discovery::new();
    assert_eq!(discovery.next_dir(), Some(".rbit/workflows"));
    assert_eq!(discovery.record(Listing::Missing), None);
    assert_eq!(discovery.next_dir(), Some(".forgejo/workflows"));
    let entries = vec![DirEntry { name: "ci.yml".into(), is_file: true }];
    let selected = discovery.record(Listing::Entries(entries)).unwrap();
    assert_eq!(
        (selected.dir, selected.paths.as_slice(), selected.available),
        (".forgejo/workflows", &[".forgejo/workflows/ci.yml".to_owned()][..], 1)
    );
    assert_eq!(discovery.next_dir(), None);
    assert_eq!(discovery.record(Listing::Missing), None);
}

#[test]
fn reads_keep_their_budget_and_error_text() {
    let limits = Limits { max_bytes: 64, ..Limits::default() };
    let mut tree = Tree::default().file("big.yml", &"#".repeat(65)).file("bad.yml", "jobs: [");
    let unreadable = load(&mut tree, "gone.yml", &limits, &Deadline::NONE).unwrap_err();
    assert_eq!((unreadable.kind, unreadable.to_string()), (ErrorKind::Parse, "the file could not be read".to_owned()));
    let large = load(&mut tree, "big.yml", &limits, &Deadline::NONE).unwrap_err();
    assert_eq!((large.kind, large.to_string()), (ErrorKind::Limit, "the file is larger than 64 bytes".to_owned()));
    assert_eq!(load(&mut tree, "bad.yml", &limits, &Deadline::NONE).unwrap_err().kind, ErrorKind::Parse);

    let expired = Deadline::at(Instant::now().checked_sub(Duration::from_secs(1)).unwrap());
    tree.read.clear();
    assert_eq!(load(&mut tree, "big.yml", &limits, &expired).unwrap_err().kind, ErrorKind::Timeout);
    assert_eq!(tree.read, Vec::<String>::new(), "the deadline is checked before reading");
}

#[test]
fn discover_parses_every_file_and_discover_one_reads_only_its_own() {
    let mut tree = Tree::default()
        .with(".rbit/workflows", &[("a.yml", true), ("b.yml", true)])
        .file(".rbit/workflows/a.yml", &format!("name: '  '\n{PUSH}"))
        .file(".rbit/workflows/b.yml", "on: [");
    let files = discover(&mut tree, &Limits::default(), &Deadline::NONE).unwrap();
    assert_eq!(files.iter().map(WorkflowFile::name).collect::<Vec<_>>(), [".rbit/workflows/a.yml", ".rbit/workflows/b.yml"]);
    assert!(files[0].parsed.is_ok() && files[1].parsed.is_err());

    tree.read.clear();
    let wanted = WorkflowFileName::parse("b.yml").unwrap();
    let found = discover_one(&mut tree, |path| wanted.matches(path), &Limits::default(), &Deadline::NONE).unwrap().unwrap();
    assert_eq!((found.path.as_str(), tree.read.as_slice()), (".rbit/workflows/b.yml", &[".rbit/workflows/b.yml".to_owned()][..]));
    let error = found.parsed.unwrap_err();
    assert!(describe(&found.path, &error).starts_with(".rbit/workflows/b.yml: line "), "{error}");
    let missing = WorkflowFileName::parse("c.yml").unwrap();
    assert!(discover_one(&mut tree, |path| missing.matches(path), &Limits::default(), &Deadline::NONE).unwrap().is_none());
}

#[test]
fn workflows_are_named_by_file_name_or_path() {
    let name = WorkflowFileName::parse("ci.yml").unwrap();
    assert!(name.matches(".forgejo/workflows/ci.yml") && name.matches(".github/workflows/ci.yml"));
    assert!(!name.matches(".forgejo/workflows/other.yml"));
    let path = WorkflowFileName::parse(" .github/workflows/ci.yaml ").unwrap();
    assert!(path.matches(".github/workflows/ci.yaml") && !path.matches(".forgejo/workflows/ci.yaml"));
    assert_eq!((path.shown(), name.shown()), (".github/workflows/ci.yaml".to_owned(), "ci.yml".to_owned()));
    for refused in ["", "ci", "../ci.yml", "a/ci.yml", ".forgejo/workflows/", "x\\ci.yml", "README.md", ".github/workflows/.."] {
        assert!(WorkflowFileName::parse(refused).is_none(), "{refused}");
    }
}

#[test]
fn events_plan_matching_workflows_and_report_broken_ones() {
    let mut tree = Tree::default()
        .with(".rbit/workflows", &[("a.yml", true), ("b.yml", true), ("c.yml", true)])
        .file(".rbit/workflows/a.yml", &format!("name: Build\n{PUSH}"))
        .file(".rbit/workflows/b.yml", "on: [")
        .file(".rbit/workflows/c.yml", "on: workflow_dispatch\njobs:\n  j:\n    runs-on: x\n    steps: [{run: x}]\n");
    let files = discover(&mut tree, &Limits::default(), &Deadline::NONE).unwrap();
    let event = Event::Push { reference: "refs/heads/main", deleted: false };
    let group = GroupContext { reference: "refs/heads/main".into(), ..GroupContext::default() };
    let out = evaluate(&files, &event, &mut None::<Vec<String>>, &group, &Limits::default(), &Deadline::NONE, &mut NoCalls);
    assert_eq!(out.iter().map(|evaluated| evaluated.name.as_str()).collect::<Vec<_>>(), ["Build", ".rbit/workflows/b.yml"]);
    assert_eq!(out[0].outcome.as_ref().unwrap().jobs.len(), 1);
    assert_eq!(out[1].outcome.as_ref().unwrap_err().kind, ErrorKind::Parse);
}

#[test]
fn groups_unique_to_one_run_are_no_groups() {
    let plan = |concurrency: &str| {
        let source = format!(
            "on: push\n{concurrency}\njobs:\n  j:\n    runs-on: x\n    concurrency: job-${{{{ github.run_number }}}}\n    steps: [{{run: x}}]\n  k:\n    runs-on: x\n    concurrency: deploy\n    steps: [{{run: x}}]\n"
        );
        let workflow = crate::parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap();
        let group = GroupContext { workflow: "ignored".into(), run_id: 9, ..GroupContext::default() };
        plan_run(&workflow, "CI", ".rbit/workflows/ci.yml", &group, &Limits::default(), &Deadline::NONE, &mut NoCalls).unwrap()
    };
    let shared = plan("concurrency: ci-${{ github.workflow }}");
    assert_eq!(shared.concurrency, Some(("ci-CI".to_owned(), false)));
    let jobs: Vec<_> = shared.jobs.iter().map(|job| job.concurrency.as_ref().map(|c| c.group.clone())).collect();
    assert_eq!(jobs, [None, Some("deploy".to_owned())]);
    assert_eq!(plan("concurrency: ci-${{ github.run_id }}").concurrency, None);
    assert_eq!(plan("concurrency: ci-${{ github.run_number }}").concurrency, None);
}
