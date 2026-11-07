import { h, status, link, icon, empty } from "./ui.mjs";
import { steps } from "./fixtures.mjs";
import { jobLog } from "./job-log.mjs";

function stepRow(ctx, model, step, index) {
  const body = index === 4 ? jobLog(ctx) : `
    <p class="step-note">${h(step.name)} completed successfully.
      <a href="/raw-log.txt">View raw output</a>
    </p>`;
  return `
    <li>
      <details data-step="${index + 1}" ${index === 4 ? "open" : ""}>
        <summary>
          ${status(index === 4 ? model.state : step.state, false)}
          <span class="step-title">${h(step.name)}</span>
          <span class="duration">${h(step.duration)}</span>
          <span class="disclosure" aria-hidden="true">⌄</span>
        </summary>
        <div class="step-body">${body}</div>
      </details>
    </li>`;
}

function completedJob(ctx, { chosen, state }) {
  if (state === "skipped") {
    const button = `<a class="btn" href="${h(link(ctx, "job", { job: "2" }))}">Open failed dependency</a>`;
    return `<div class="box">${empty(
      "This job was skipped",
      "The Windows test job did not pass, so its dependent package job did not start.", button,
    )}</div>`;
  }
  return `
    <ol class="box steps">
      <li><details open>
        <summary>${status("success")}
          <span class="step-title">${h(chosen.name)}</span>
          <span class="duration">${h(chosen.duration)}</span>
        </summary>
        <p class="step-note">This job completed successfully.
          <a href="${h(link(ctx, "job", { job: "2" }))}">Open the Windows failure fixture</a> for the long-log workspace.
        </p>
      </details></li>
    </ol>`;
}

export function jobSteps(ctx, model) {
  if (ctx.state === "error") {
    const controls = `
      <a class="btn" href="${h(link(ctx, "job", { state: "failed" }))}">Retry log</a>
      <a class="btn" href="/raw-log.txt">Raw log</a>`;
    return `<div class="box">${empty("The log could not be loaded", "The job state is still available. Retry to fetch this log window.", controls)}</div>`;
  }
  if (model.idle) {
    const refresh = `<a class="btn" href="${h(link(ctx, "job", { state: "running" }))}">${icon("refresh")} Refresh</a>`;
    return `<div class="box">${empty("Waiting for a runner", "Steps and logs will appear when a matching runner takes this job.", refresh)}</div>`;
  }
  if (!model.active && model.selected !== 2) return completedJob(ctx, model);
  return `<ol class="box steps">${steps.map((step, index) => stepRow(ctx, model, step, index)).join("")}</ol>`;
}
