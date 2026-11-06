import { h, icon, status, link, action, writer } from "./ui.mjs";
import { jobs } from "./fixtures.mjs";

export function jobNavigation(ctx, model) {
  const rows = jobs.map((job, index) => `
    <a href="${h(link(ctx, "job", { job: String(index) }))}"
      ${index === model.selected ? 'aria-current="page"' : ""}>
      ${status(index === model.selected ? model.state : job.state, false)}
      <span>${h(job.name)}</span>
    </a>`).join("");
  return `
    <nav aria-label="Jobs in this run">
      <h2 class="eyebrow">Jobs in this run</h2>${rows}
    </nav>`;
}

export function jobBreadcrumb(ctx, { chosen, entry }) {
  return `
    <p class="breadcrumb">
      <a href="${h(link(ctx, "overview"))}">Actions</a><span>/</span>
      <a href="${h(link(ctx, "run"))}">${h(entry.workflow)} #${entry.number}</a>
      <span>/</span><span>${h(chosen.name)}</span>
    </p>`;
}

export function jobHeader(ctx, model) {
  const { chosen, entry, idle, active, state, selected } = model;
  const raw = !idle && state !== "skipped" && selected === 2 ? `
    <a class="btn" href="/raw-log.txt" download="windows-tests.log">
      ${icon("download")} Raw log
    </a>` : "";
  const rerun = writer(ctx) && !active && !idle ? action(ctx, "rerun-job", "Re-run job") : "";
  return `
    <div class="section-heading">
      <div>
        <p class="eyebrow">${h(entry.workflow)} #${entry.number} · attempt 2</p>
        <div class="title-line">${status(state, false)}<h1>${h(chosen.name)}</h1></div>
        <p class="meta">
          ${status(state)}<span class="duration">${idle ? "Not started" : h(chosen.duration)}</span>
          <span>runs on <code dir="ltr">${chosen.name.includes("Windows") ? "windows-latest" : "ubuntu-latest"}</code></span>
        </p>
      </div>
      <div class="button-row">${raw}${rerun}</div>
    </div>`;
}

export function jobFailure(ctx, model) {
  if (model.state !== "failure") return "";
  return `
    <div class="job-failure">
      <span>${icon("failure")} <strong>Run workspace tests</strong> exited with code 101.</span>
      <a href="${h(link(ctx, "job", { earlier: "" }))}#L49984">Jump to failure ↓</a>
    </div>`;
}

export function proposalNote(ctx, model) {
  if (model.selected !== 2 || model.idle) return "";
  const simulate = ctx.state === "running" ? `
    <button class="btn btn-ghost js-only" data-simulate>Simulate new output</button>` : "";
  return `
    <p class="proposed-note">Proposed log workspace · fixed synthetic data, no live runner connection. ${simulate}</p>`;
}
