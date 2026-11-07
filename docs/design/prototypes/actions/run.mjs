import { h, icon, status, intent, link, hidden, action, writer, empty } from "./ui.mjs";
import { jobs, selectedRun } from "./fixtures.mjs";

function approval(ctx) {
  if (!["approval", "deployment"].includes(ctx.state)) return "";
  const fork = `<section class="notice approval-notice" aria-labelledby="trust-heading">${icon("approval")}<div>
    <h2 id="trust-heading">Review this workflow before it runs</h2>
    <p>This pull request comes from an outside author. Approval permits execution with a read-only token and no secrets or variables.</p>
    ${writer(ctx) ? `<form action="/intent" method="get" data-production-method="post">${hidden(ctx)}<input type="hidden" name="return" value="run">
    <label class="checkbox">
    <input type="checkbox" name="trust_author" value="true">Always run this author's pull requests without approval</label>
    <button class="btn btn-primary" name="intent" value="approve-run">Approve and run</button>
    </form>` : '<p class="hint">A repository writer must approve this run.</p>'}</div>
    </section>
    `;
  const deployment = `<section class="box environment">
    <div class="panel-heading">${icon("approval")}<h2>Production deployment needs a review</h2>
    <span class="badge">production</span>
    </div>
    <div class="panel-body">
    <p>The deploy job waits for a required environment reviewer. Workflow approval does not grant deployment approval.</p>
    ${ctx.viewer === "reviewer" ? `<form action="/intent" method="get" data-production-method="post">${hidden(ctx)}<input type="hidden" name="return" value="run">
    <input type="hidden" name="environment" value="production">
    <label class="label" for="review-comment">Comment <span class="muted">(optional)</span>
    </label>
    <textarea class="input" id="review-comment" name="comment" rows="2">
    </textarea>
    <div class="button-row">
    <button class="btn btn-primary" name="intent" value="approve-deployment">Approve and deploy</button>
    <button class="btn" name="intent" value="reject-deployment">Reject</button>
    </div>
    </form>` : '<p class="hint">Only an authorized environment reviewer can approve or reject this deployment.</p>'}</div>
    </section>`;
  return ctx.state === "approval" ? fork : deployment;
}

export function runPage(ctx) {
  if (ctx.state === "denied") return `<div class="box">${empty("Run not found", "This run is unavailable or you do not have access to it.", `<a class="btn" href="${h(link(ctx, "overview"))}">Back to workflow runs</a>`, true)}</div>`;
  const entry = selectedRun(ctx);
  if (["empty", "queued"].includes(ctx.state)) return `<p class="breadcrumb">
    <a href="${h(link(ctx, "overview"))}">Actions</a> / Continuous integration</p>
    <div class="section-heading">
    <div>
    <h1>${h(entry.title)} <span class="muted">#${entry.number}</span>
    </h1>
    <p class="meta">${status("queued")}<span>Waiting to start</span>
    </p>
    </div>${writer(ctx) ? action(ctx, "cancel-run", "Cancel run") : ""}</div>
    <div class="box">${empty("No jobs have started yet", "This run is queued. Refresh to see when a matching runner starts the work.", `<a class="btn" href="${h(link(ctx, "run", { state: "running" }))}">Refresh run</a>`)}</div>`;
  if (ctx.state === "error") return `<p class="breadcrumb">
    <a href="${h(link(ctx, "overview"))}">Actions</a> / Continuous integration</p>
    <div class="section-heading">
    <h1>Workflow could not be planned</h1>
    </div>
    <section class="failure-summary" role="alert">${icon("failure")}<div>
    <h2>Check the workflow configuration</h2>
    <p>
    <code dir="ltr">.rbit/workflows/ci.yml:18</code> · A job refers to a dependency that does not exist.</p>
    <p>No jobs were started. Review the workflow at this run's commit before trying again.</p>
    <a class="btn" href="${h(intent(ctx, "workflow-files"))}">View workflow file</a>
    </div>
    </section>`;
  const running = ctx.state === "running";
  const awaiting = ["approval", "deployment"].includes(ctx.state);
  const state = awaiting ? "approval" : running ? "running"
    : ["success", "cancelled"].includes(ctx.state) ? ctx.state : "failure";
  const controls = writer(ctx) ? running || awaiting ? action(ctx, "cancel-run", "Cancel run") : `${state === "failure" ? action(ctx, "rerun-failed", "Re-run failed", true) : ""}${action(ctx, "rerun-all", "Re-run all")}` : "";
  const list = jobRows(ctx, { awaiting, running, state });
  const graph = dependencyStages(ctx, { awaiting, running, state });
  return `<p class="breadcrumb">
    <a href="${h(link(ctx, "overview"))}">Actions</a>
    <span>/</span>
    <a href="${h(link(ctx, "overview", { workflow: "1" }))}">Continuous integration</a>
    </p>
    <section>
    <div class="section-heading">
    <div>
    <div class="title-line">${status(state, false)}<h1>${h(entry.title)} <span class="muted">#${entry.number}</span>
    </h1>
    </div>
    <p class="meta">${status(state)}<span>${h(entry.event)}</span>
    <span>started by <strong>${h(entry.actor)}</strong>
    </span>
    <time>${h(entry.time)}</time>
    </p>
    </div>
    <div class="button-row">${controls}</div>
    </div>
    <dl class="run-facts">
    <div>
    <dt>Workflow</dt>
    <dd>${h(entry.workflow)}</dd>
    </div>
    <div>
    <dt>Commit</dt>
    <dd>
    <a href="${h(intent(ctx, "view-commit"))}">
    <code dir="ltr">${h(entry.sha)}</code>
    </a> <bdi class="muted">${h(entry.branch)}</bdi>
    </dd>
    </div>
    <div>
    <dt>Duration</dt>
    <dd>${awaiting ? "Not started" : running ? "2m 08s" : "4m 32s"}</dd>
    </div>
    <div>
    <dt>Attempt</dt>
    <dd>2 <span class="muted">of this run</span>
    </dd>
    </div>
    </dl>
    ${approval(ctx)}${state === "failure" ? `<section class="failure-summary" aria-labelledby="failure-heading">${icon("failure")}<div>
    <h2 id="failure-heading">One job needs attention</h2>
    <p>
    <a href="${h(link(ctx, "job"))}">Tests · Windows / stable</a> failed in <strong>Run workspace tests</strong>.</p>
    <p class="muted">A revoked session returned an unexpected response status. Package was skipped because its dependencies did not pass.</p>
    <a class="link" href="${h(link(ctx, "job"))}#L49984">View the failing assertion ${icon("chevron")}</a>
    </div>
    </section>` : ""}
    <div class="list-heading">
    <h2>Workflow jobs</h2>
    <a href="#jobs" class="muted">Jump to accessible job list</a>
    </div>${graph}<div class="list-heading" id="jobs">
    <h2>All jobs <span class="count">6</span>
    </h2>
    <span class="muted">${awaiting ? "Waiting for approval" : running ? "Live status · refresh to update" : "4 passed · 1 failed · 1 skipped"}</span>
    </div>
    <ul class="box job-list">${list}</ul>
    ${awaiting || running || state === "cancelled" ? '<p class="notice">Job summaries and artifacts appear here after their producing jobs upload them.</p></section>' : `<div class="run-bottom">
    <section class="box">
    <div class="panel-heading">${icon("file")}<h2>Job summary</h2>
    </div>
    <div class="panel-body">
    <h3>Test results</h3>
    <p class="muted">${state === "success" ? "All test checks passed." : "The test suite completed with one failing check."}</p>
    <dl class="summary-counts">
    <div>
    <dt>Passed</dt>
    <dd>${state === "success" ? "614" : "613"}</dd>
    </div>
    <div>
    <dt>Failed</dt>
    <dd class="${state === "success" ? "" : "failure"}">${state === "success" ? "0" : "1"}</dd>
    </div>
    <div>
    <dt>Ignored</dt>
    <dd>0</dd>
    </div>
    </dl>
    <p>
    <a href="${h(link(ctx, "job"))}">Open Windows test details ${icon("chevron")}</a>
    </p>
    </div>
    </section>
    <section class="box">
    <div class="panel-heading">${icon("download")}<h2>Artifacts</h2>
    <span class="count">1</span>
    </div>
    <div class="panel-body artifact">
    <div>${icon("file")}<a class="strong" href="${h(intent(ctx, "artifact-zip"))}">test-results-windows.zip</a>
    </div>
    <p class="meta">48 KB · expires in 7 days</p>
    <a class="link" href="${h(intent(ctx, "artifact-tar"))}">Download as tar.gz</a>
    </div>
    </section>
    </div>
    </section>`}`;
}

function jobRows(ctx, { awaiting, running, state }) {
  return jobs.map((job, index) => {
    const look = jobState(job, index, { awaiting, running, state });
    return `<li class="job-row">${status(look, false)}<div>
    <a class="strong" href="${h(link(ctx, "job", { job: String(index), state: running ? "running" : "failed" }))}">${h(job.name)}</a>
    <p class="meta">${h(job.needs)}</p>
    </div>
    <span class="job-result">${status(look)}<span class="duration">${awaiting ? "Not started" : h(job.duration)}</span>
    </span>${writer(ctx) && !running && !awaiting ? action(ctx, "rerun-job", "Re-run") : ""}</li>`;
  }).join("");
}

function dependencyStages(ctx, { awaiting, running, state }) {
  return `<ol class="pipeline" aria-label="Job dependencies">
    <li class="pipeline-stage">
    <span class="eyebrow">01 · Validate</span>
    <a href="${h(link(ctx, "job", { job: "0" }))}">${status(awaiting ? "queued" : "success", false)} Format and lint</a>
    <span class="muted">No dependencies</span>
    </li>
    <li class="pipeline-arrow" aria-hidden="true">→</li>
    <li class="pipeline-stage">
    <span class="eyebrow">02 · Test matrix</span>
    <a href="#jobs">${status(state, false)} Tests <span class="count">4 jobs</span>
    </a>
    <span class="muted">Needs format and lint</span>
    </li>
    <li class="pipeline-arrow" aria-hidden="true">→</li>
    <li class="pipeline-stage">
    <span class="eyebrow">03 · Package</span>
    <a href="${h(link(ctx, "job", { job: "5" }))}">${status(awaiting || running ? "waiting" : state === "success" ? "success" : state === "cancelled" ? "cancelled" : "skipped", false)} Package</a>
    <span class="muted">Needs all test jobs</span>
    </li>
    </ol>`;
}

function jobState(job, index, { awaiting, running, state }) {
  if (awaiting) return "queued";
  if (state === "success") return "success";
  if (state === "cancelled") return index < 2 ? "success" : "cancelled";
  if (!running) return job.state;
  if (index < 2 || index === 3) return "success";
  return index === 2 ? "running" : "waiting";
}
