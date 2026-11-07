import { h, icon, status, intent, link, hidden, action, writer, empty } from "./ui.mjs";
import { runs } from "./fixtures.mjs";

export function overview(ctx) {
  const sidebar = `<aside class="workflow-sidebar">
    <h2 class="eyebrow">Workflows</h2>
    <nav aria-label="Workflows" class="workflow-links">
    ${workflowLinks(ctx)}</nav>
    <div class="sidebar-note">
    <p class="eyebrow">Workflow files</p>
    <p>Configured in <code dir="ltr">.rbit/workflows/</code>
    </p>
    <a href="${h(intent(ctx, "workflow-files"))}">View workflow files ${icon("chevron")}</a>
    </div>
    </aside>`;
  const heading = ctx.workflow === "0" ? "All workflow runs" : ["", "Continuous integration", "Release", "Security"][Number(ctx.workflow)];
  const filters = `<form class="filters" action="/overview" method="get" role="search">${hidden(ctx, ["branch", "actor", "event", "status"])}
    <label>
    <span class="sr-only">Branch</span>
    <input class="input" type="search" name="branch" value="${h(ctx.branch)}" placeholder="Branch" autocomplete="off">
    </label>
    <label>
    <span class="sr-only">Actor</span>
    <input class="input" type="search" name="actor" value="${h(ctx.actor)}" placeholder="Actor" autocomplete="off">
    </label>
    <label>
    <span class="sr-only">Event</span>
    <select class="input" name="event">${filterOptions(["Any event", "push", "pull request", "manual", "schedule"], ctx.event)}</select>
    </label>
    <label>
    <span class="sr-only">Status</span>
    <select class="input" name="status">${filterOptions(["Any status", "failure", "running", "success", "approval", "cancelled"], ctx.status)}</select>
    </label>
    <button class="btn">Filter</button>
    <a class="btn btn-ghost" href="${h(link(ctx, "overview", { branch: "", actor: "", event: "", status: "" }))}">Clear</a>
    </form>`;
  const workflowName = ["", "Continuous integration", "Release", "Security"][Number(ctx.workflow)];
  const filtered = runs.filter((run) =>
      (!workflowName || run.workflow === workflowName)
      && (!ctx.branch || run.branch.includes(ctx.branch))
      && (!ctx.actor || run.actor.includes(ctx.actor))
      && (!ctx.event || run.event === ctx.event)
      && (!ctx.status || run.state === ctx.status))
    .map((run) => ctx.page === "2" ? { ...run, number: run.number - 6, time: "Yesterday" } : run);
  const rows = filtered.map((run) => `<li class="run-row">${status(run.state, false)}<div class="run-content">
    <a class="run-title" href="${h(link(ctx, "run", { state: run.state === "failure" ? "failed" : run.state, number: String(run.number) }))}">${h(ctx.long === "1" ? `${run.title} — maintaining consistent behavior across supported platforms and long translated labels` : run.title)}</a>
    <p class="meta">
    <span>${h(run.workflow)} #${run.number}</span>
    <span>${h(run.event)} by ${h(run.actor)}</span>
    <span class="branch">${icon("branch")}<bdi>${h(run.branch)}</bdi>
    </span>
    <code dir="ltr">${h(run.sha)}</code>
    </p>
    </div>
    <div class="run-meta">${status(run.state)}<time>${h(run.time)}</time>
    <span class="duration">${h(run.duration)}</span>
    </div>
    </li>`).join("");
  const dispatch = writer(ctx) && ctx.workflow !== "0" ? `<details class="box dispatch">
    <summary>${icon("play")} Run workflow</summary>
    <div class="panel-body">
    <form action="/overview" method="get" class="inline-form">${hidden(ctx, ["ref"])}<label>Use workflow from<input class="input mono" name="ref" value="${h(ctx.ref)}" list="workflow-refs" required>
    </label>
    <datalist id="workflow-refs">
    <option value="main">
    <option value="v0.3.1">
    </datalist>
    <button class="btn">Change</button>
    </form>
    <form action="/intent" method="get" data-production-method="post">${hidden(ctx)}<input type="hidden" name="return" value="overview">
    <label class="label">Deployment environment<select class="input" name="inputs[environment]">
    <option>staging</option>
    <option>production</option>
    </select>
    </label>
    <label class="label">Release label <span class="muted">(optional)</span>
    <input class="input" type="text" name="inputs[label]" placeholder="Release candidate">
    </label>
    <label class="label">Batch size<input class="input" type="number" step="any" name="inputs[batch]" value="1" required>
    </label>
    <input type="hidden" name="inputs[smoke]" value="false">
    <label class="checkbox">
    <input type="checkbox" name="inputs[smoke]" value="true" checked>Run smoke checks</label>
    <button class="btn btn-primary" name="intent" value="dispatch">Run workflow</button>
    </form>
    </div>
    </details>` : "";
  const body = resultsBody(ctx, rows, filtered);
  return `<div class="actions-layout">${sidebar}<section class="runs-main">
    <div class="section-heading">
    <div>
    <p class="eyebrow">Actions</p>
    <h1>${h(heading)}</h1>
    <p class="muted">Every workflow, from commit to completion.</p>
    </div>${writer(ctx) && ctx.workflow !== "0" ? action(ctx, "toggle-workflow", ctx.state === "disabled" ? "Enable workflow" : "Disable workflow") : `<a class="btn" href="${h(intent(ctx, "workflow-files"))}">${icon("file")} Workflow files</a>`}</div>
    ${ctx.state === "disabled" ? '<p class="notice">This workflow is disabled. No event starts a run until a writer enables it.</p>' : ""}${dispatch}${filters}
    <div class="list-heading">
    <h2>Recent runs</h2>
    <span class="muted">Latest 6 of 26</span>
    </div>${body}<nav class="pagination" aria-label="Pagination">
    <a class="btn" href="${h(link(ctx, "overview", { page: "2" }))}" rel="next">Older runs ${icon("chevron")}</a>
    </nav>
    </section>
    </div>`;
}

function workflowLinks(ctx) {
  return ["All workflows", "Continuous integration", "Release", "Security"]
    .map((name, index) => `
      <a href="${h(link(ctx, "overview", { workflow: String(index) }))}"
        ${ctx.workflow === String(index) ? 'aria-current="page"' : ""}>
        ${icon(index === 0 ? "play" : "file")}<span>${h(name)}</span>
        ${index === 0 ? '<span class="count">26</span>' : ""}
      </a>`).join("");
}

function filterOptions(values, selected) {
  return values.map((value, index) => `
    <option value="${index ? h(value) : ""}" ${selected === value ? "selected" : ""}>
      ${h(value)}
    </option>`).join("");
}

function resultsBody(ctx, rows, filtered) {
  if (ctx.state === "error") {
    const retry = `<a class="btn" href="${h(link(ctx, "overview", { state: "failed" }))}">${icon("refresh")} Retry</a>`;
    return `<div class="box">${empty(
      "Workflow runs could not be loaded",
      "Your filters have been kept. Retry this request to load the latest runs.", retry,
    )}</div>`;
  }
  if (ctx.state !== "empty" && filtered.length) return `<ul class="run-list box">${rows}</ul>`;
  const hasFilters = !!(ctx.branch || ctx.actor || ctx.event || ctx.status);
  const clear = `<a class="btn"
    href="${h(link(ctx, "overview", { state: "failed", branch: "", actor: "", status: "", event: "" }))}">
    ${hasFilters ? "Clear filters" : "Refresh runs"}
  </a>`;
  return `<div class="box">${empty(
    hasFilters ? "No matching workflow runs" : "No workflow runs yet",
    hasFilters ? "Try another branch or clear your filters."
      : "Add a workflow under .rbit/workflows and push it. Runs appear here once they are triggered.",
    clear,
  )}</div>`;
}
