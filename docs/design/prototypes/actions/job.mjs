import { h, icon, link, empty } from "./ui.mjs";
import { jobs, selectedRun } from "./fixtures.mjs";
import { jobBreadcrumb, jobNavigation, jobHeader, jobFailure, proposalNote } from "./job-header.mjs";
import { jobSteps } from "./job-steps.mjs";

function jobModel(ctx) {
  const selected = /^[0-5]$/.test(ctx.job) ? Number(ctx.job) : 2;
  const chosen = jobs[selected];
  const idle = ["empty", "queued", "approval"].includes(ctx.state);
  const active = ["running", "disconnected"].includes(ctx.state);
  return {
    selected, chosen, idle, active,
    entry: selectedRun(ctx),
    state: idle ? "queued" : active ? "running" : chosen.state,
  };
}

export function jobPage(ctx) {
  if (ctx.state === "denied") {
    const back = `<a class="btn" href="${h(link(ctx, "overview"))}">Back to workflow runs</a>`;
    return `<div class="box">${empty("Job not found", "This job is unavailable or you do not have access to it.", back, true)}</div>`;
  }
  const model = jobModel(ctx);
  const navigation = jobNavigation(ctx, model);
  return `
    ${jobBreadcrumb(ctx, model)}
    <div class="job-layout">
      <aside class="job-sidebar">${navigation}</aside>
      <section class="job-main">
        <details class="mobile-jobs">
          <summary>Jobs in this run ${icon("chevron")}</summary>
          ${navigation}
        </details>
        ${jobHeader(ctx, model)}
        ${jobFailure(ctx, model)}
        ${proposalNote(ctx, model)}
        ${jobSteps(ctx, model)}
      </section>
    </div>`;
}
