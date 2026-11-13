//! Review without a diff (agent plan steps 5.1 to 5.3): the diff manifest of a pull request
//! and its change classes, one file or hunk on request, the review inbox, and resolving a
//! thread from the API.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rbit_core::{
    IssueNumber, TokenScope,
    pull::{ReviewCommentId, ReviewSide},
};
use rbit_git::diff::{Hunk, LineKind};
use rbit_services::pulls::{
    PullContext,
    inbox::{Anchor, ManifestFile},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Get, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::ApiJson,
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    let repo = |suffix: &str| format!("/repos/{{owner}}/{{repo}}{suffix}");
    vec![
        route(Get, repo("/pulls/{number}/manifest"), manifest),
        route(Get, repo("/pulls/{number}/inbox"), inbox),
        route(Put, repo("/pulls/{number}/comments/{id}/resolution"), resolve),
    ]
}

#[derive(Debug, Deserialize)]
pub struct PullPath {
    owner: String,
    repo: String,
    number: String,
}

async fn open_pull(state: &AppState, caller: &Caller, owner: &str, repo: &str, number: &str) -> ApiResult<PullContext> {
    let repo = caller.open_repo(state, owner, repo).await?;
    let number = IssueNumber::parse(number).map_err(|_| ApiError::not_found())?;
    Ok(state.services.pulls.open(caller.user(), repo, number).await?)
}

// ------------------------------------------------------------------ change classes (5.3)

/// Extensions of code and scripts: such a file under `docs/` (a `conf.py`, a build script)
/// changes what runs, so it is not documentation.
const CODE_EXTENSIONS: [&str; 27] = [
    ".py", ".sh", ".bash", ".zsh", ".ps1", ".bat", ".cmd", ".js", ".mjs", ".cjs", ".ts", ".mts", ".cts", ".jsx", ".tsx", ".rs", ".go",
    ".rb", ".pl", ".php", ".java", ".kt", ".c", ".h", ".cpp", ".cs", ".lua",
];

/// Whether `path` is documentation: under a `docs/` directory and not code or a script (nor
/// a `Makefile`), or a markup file. Plain text elsewhere is often data or a fixture, so it
/// is not.
fn is_docs(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let code = CODE_EXTENSIONS.iter().any(|end| name.ends_with(end)) || name == "makefile";
    let markup = [".md", ".markdown", ".rst", ".adoc"].iter().any(|end| name.ends_with(end));
    markup || ((lower.starts_with("docs/") || lower.contains("/docs/")) && !code)
}

/// Directories whose files are CI definitions: rbit's, and the ones it also discovers or
/// other forges read.
const WORKFLOW_DIRS: [&str; 4] = [".rbit/workflows/", ".github/workflows/", ".forgejo/workflows/", ".gitea/workflows/"];

/// Lock and manifest files of common package managers.
const DEPENDENCY_FILES: [&str; 22] = [
    "Cargo.lock",
    "Cargo.toml",
    "package.json",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "go.mod",
    "go.sum",
    "requirements.txt",
    "poetry.lock",
    "pyproject.toml",
    "Pipfile",
    "Pipfile.lock",
    "uv.lock",
    "Gemfile",
    "Gemfile.lock",
    "composer.json",
    "composer.lock",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "gradle.lockfile",
];

/// Whether `path` is a test.
fn is_test(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower.starts_with("tests/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("__tests__/")
        || name.starts_with("test_")
        || [".test.", ".spec.", "_test.", "_spec."].iter().any(|part| name.contains(part))
}

/// The change classes of `files`: `docs_only`, `tests_only`, `workflows` (CI definitions)
/// and `dependencies` (lock and manifest files). Empty for no files.
fn classes(files: &[ManifestFile]) -> Vec<&'static str> {
    let paths: Vec<&str> = files.iter().flat_map(|file| std::iter::once(file.path.as_str()).chain(file.old_path.as_deref())).collect();
    if paths.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    if paths.iter().all(|path| is_docs(path)) {
        found.push("docs_only");
    }
    if paths.iter().all(|path| is_test(path)) {
        found.push("tests_only");
    }
    if paths.iter().any(|path| WORKFLOW_DIRS.iter().any(|dir| path.starts_with(dir))) {
        found.push("workflows");
    }
    if paths.iter().any(|path| DEPENDENCY_FILES.contains(&path.rsplit('/').next().unwrap_or(path))) {
        found.push("dependencies");
    }
    found
}

// ------------------------------------------------------------------ manifest (5.1)

#[derive(Debug, Default, Deserialize)]
pub struct ManifestQuery {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    hunk: Option<usize>,
}

/// A hunk as unified-diff text.
fn hunk_text(hunk: &Hunk) -> String {
    let mut text = hunk.header.clone();
    for line in &hunk.lines {
        let sign = match line.kind {
            LineKind::Context => ' ',
            LineKind::Added => '+',
            LineKind::Removed => '-',
        };
        text.push('\n');
        text.push(sign);
        text.push_str(&line.text);
    }
    text
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/manifest[?path=<file>[&hunk=<n>]]`
pub async fn manifest(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<PullPath>,
    Query(query): Query<ManifestQuery>,
) -> ApiResult<Json<Value>> {
    let ctx = open_pull(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let pulls = &state.services.pulls;
    if let Some(file) = query.path.filter(|file| !file.is_empty()) {
        let hunks = pulls.file_hunks(&ctx, &file).await?.ok_or_else(ApiError::not_found)?;
        let chosen: Vec<Value> = match query.hunk {
            Some(index) => vec![json!({ "index": index, "text": hunk_text(hunks.get(index).ok_or_else(ApiError::not_found)?) })],
            None => hunks.iter().enumerate().map(|(index, hunk)| json!({ "index": index, "text": hunk_text(hunk) })).collect(),
        };
        return Ok(Json(json!({ "schema": "rbit.hunks/1", "path": file, "hunks": hunks.len(), "items": chosen })));
    }
    let found = pulls.manifest(&ctx).await?;
    let files: Vec<Value> = found
        .files
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "old_path": file.old_path,
                "status": file.status,
                "additions": file.additions,
                "deletions": file.deletions,
                "stats_known": file.stats_known,
                "binary": file.binary,
                "old_blob": file.old_blob,
                "new_blob": file.new_blob,
            })
        })
        .collect();
    Ok(Json(json!({
        "schema": "rbit.manifest/1",
        "base": found.base,
        "head": found.head,
        "classes": classes(&found.files),
        "files": files,
    })))
}

// ------------------------------------------------------------------ inbox (5.2)

/// `GET /repos/{owner}/{repo}/pulls/{number}/inbox`: the unresolved threads, oldest first.
pub async fn inbox(State(state): State<AppState>, caller: Caller, Path(path): Path<PullPath>) -> ApiResult<Json<Value>> {
    let ctx = open_pull(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let threads = state.services.pulls.inbox(caller.user(), &ctx).await?;
    let items: Vec<Value> = threads
        .iter()
        .map(|thread| {
            let root = &thread.root;
            let (anchor, line, head_path) = match &thread.anchor {
                Anchor::Current => ("current", Some(root.line as usize), Some(&root.path)),
                Anchor::Moved { line, path } => ("moved", Some(*line), Some(path.as_ref().unwrap_or(&root.path))),
                Anchor::Ambiguous => ("ambiguous", None, None),
            };
            json!({
                "id": root.id.0,
                "path": root.path,
                "side": if root.side == ReviewSide::Left { "LEFT" } else { "RIGHT" },
                "line": root.line,
                "commit": root.commit_sha,
                "anchor": anchor,
                "head_line": line,
                // Where the line is in the head: `path`, or the file's new name after a rename.
                "head_path": head_path,
                "author": thread.author,
                "says": rbit_services::pulls::inbox::excerpt(&root.body),
                "replies": thread.replies,
                "last_reply": thread.last.as_ref().map(|(author, text)| json!({ "author": author, "says": text })),
            })
        })
        .collect();
    Ok(Json(json!({ "schema": "rbit.inbox/1", "number": ctx.issue().number.get(), "head": ctx.pull.head_sha, "threads": items })))
}

#[derive(Debug, Deserialize)]
pub struct CommentPath {
    owner: String,
    repo: String,
    number: String,
    id: i64,
}

#[derive(Debug, Deserialize)]
pub struct Resolution {
    resolved: bool,
}

/// `PUT /repos/{owner}/{repo}/pulls/{number}/comments/{id}/resolution {"resolved": true}`
pub async fn resolve(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<CommentPath>,
    ApiJson(request): ApiJson<Resolution>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    caller.require_right(rbit_services::tokens::Right::Approve)?;
    let user = caller.require(TokenScope::Write)?;
    let ctx = open_pull(&state, &caller, &path.owner, &path.repo, &path.number).await?;
    let comment = state.services.pulls.set_thread_resolved(user, &ctx, ReviewCommentId(path.id), request.resolved).await?;
    Ok((StatusCode::OK, Json(json!({ "schema": "rbit.resolution/1", "id": comment.id.0, "resolved": comment.resolved_at.is_some() }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> ManifestFile {
        ManifestFile {
            path: path.to_owned(),
            old_path: None,
            status: "modified",
            additions: 1,
            deletions: 0,
            stats_known: true,
            binary: false,
            old_blob: None,
            new_blob: None,
        }
    }

    #[test]
    fn changes_are_classed_by_their_paths() {
        assert_eq!(classes(&[file("docs/guide.md"), file("README.md")]), ["docs_only"]);
        assert_eq!(classes(&[file("crates/a/tests/x.rs"), file("src/thing_test.go")]), ["tests_only"]);
        assert_eq!(classes(&[file(".rbit/workflows/ci.yml"), file("Cargo.lock")]), ["workflows", "dependencies"]);
        assert_eq!(classes(&[file("src/main.rs"), file("docs/a.md")]), Vec::<&str>::new());
        assert_eq!(classes(&[]), Vec::<&str>::new());
        assert_eq!(classes(&[file("src/fixture.txt")]), Vec::<&str>::new(), "text outside docs/ may be data");
        assert_eq!(classes(&[file("docs/notes.txt")]), ["docs_only"]);
    }

    #[test]
    fn code_under_docs_other_forges_workflows_and_more_lockfiles_are_classed() {
        let none = Vec::<&str>::new();
        for code in ["docs/conf.py", "docs/build.sh", "docs/theme/app.js", "docs/gen.ts", "site/docs/gen.rs", "docs/Makefile"] {
            assert_eq!(classes(&[file("docs/guide.md"), file(code)]), none, "{code} is not documentation");
        }
        assert_eq!(classes(&[file("docs/images/flow.svg"), file("docs/guide.md")]), ["docs_only"]);
        for workflow in [".forgejo/workflows/ci.yml", ".gitea/workflows/ci.yaml", ".github/workflows/x.yml"] {
            assert_eq!(classes(&[file(workflow)]), ["workflows"], "{workflow}");
        }
        for lock in [
            "yarn.lock",
            "web/pnpm-lock.yaml",
            "pyproject.toml",
            "Pipfile.lock",
            "uv.lock",
            "Gemfile.lock",
            "composer.lock",
            "pom.xml",
            "app/build.gradle",
            "build.gradle.kts",
        ] {
            assert_eq!(classes(&[file(lock)]), ["dependencies"], "{lock}");
        }
    }
}
