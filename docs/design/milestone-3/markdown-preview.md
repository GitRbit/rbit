# Markdown previews

The Markdown preview stretch item of workstream W is implemented independently of its
other roadmap items. Issue and pull request descriptions, timeline edits and comments,
reviews, line comments and replies offer Write/Preview tabs. Markdown files in the browser
editor use the same enhancement. Without JavaScript, every editor remains a normal form.

`POST /issues/markdown-preview?owner=…&repo=…` renders form field `body` with
`render_issue_markdown`, including the same reference resolution and default-branch
document base as saved discussion content. It first opens the repository for the current
viewer; a hidden or missing repository answers 404. Rendering does not persist drafts or
create references, notifications or events. Responses use `Cache-Control: private, no-store`.

The endpoint lives under the reserved `/issues` namespace so it cannot shadow a user
repository. This is a deliberate change from the original `/markdown/preview` design
placeholder, since `markdown` is a valid account name.

The file editor supplies an additional `revision` query parameter and posts `content` and
the current `path`. Empty and plain-text filenames retain preview context, with controls
shown only while the current filename ends in `.md`, `.markdown`, `.mdown` or `.mkd`.
Renaming a file closes any stale preview and preserves the draft. It uses `render_markdown`, without issue-reference expansion, with
relative links resolved from that file's directory on the selected branch. This activates
the existing `markdown_preview_url` integration hook. Discussion text is bounded at 65,536
bytes; file content uses the configured editor limit. The route also bounds form-encoded
request bytes and file context lengths. Existing page middleware supplies Origin checks,
per-client POST throttling and the strict Content Security Policy. All content uses the
existing final Ammonia sanitizer; there are no inline scripts or styles.

Controls use the existing tab and button design, keyboard navigation, named panels and
live loading/error announcements. Draft text remains in its textarea while viewing the
preview, and remains submittable without switching back. Pending preview requests cannot
be duplicated; switching to Write during a request keeps the source visible. htmx swaps
and history restoration remount working controls.

Router regressions cover sanitized output, references and relative links, file context,
empty content, private repositories, Origin rejection, Unicode byte limits, context
bounds and throttling. Browser behavior tests exercise tabs, request data, loading/error
recovery, pending requests and history restoration. No migration or dependency is needed.
