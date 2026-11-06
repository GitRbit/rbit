import { h, icon, link, hidden, empty } from "./ui.mjs";
import { logLines } from "./fixtures.mjs";

function highlight(text, query) {
  const index = query ? text.toLowerCase().indexOf(query.toLowerCase()) : -1;
  if (index < 0) return h(text);
  return `${h(text.slice(0, index))}<mark>${h(text.slice(index, index + query.length))}</mark>${h(text.slice(index + query.length))}`;
}

function toolbar(ctx) {
  return `
    <div class="log-toolbar">
      <form action="/job" method="get" role="search">
        ${hidden(ctx, ["q"])}
        <label class="log-search">
          ${icon("search")}<span class="sr-only">Search this log window</span>
          <input class="input" name="q" value="${h(ctx.q)}"
            placeholder="Search this log window" autocomplete="off">
        </label>
        <button class="btn">Find</button>
      </form>
      <div class="button-row">
        <button type="button" class="btn btn-ghost js-only" data-match="previous"
          aria-controls="log-window" aria-label="Previous search match">↑</button>
        <button type="button" class="btn btn-ghost js-only" data-match="next"
          aria-controls="log-window" aria-label="Next search match">↓</button>
        <button type="button" class="btn js-only" data-copy
          aria-controls="log-window">Copy window</button>
        <button type="button" class="btn js-only" data-follow
          aria-controls="log-window" aria-pressed="false">Follow tail</button>
      </div>
    </div>`;
}

function windowNavigation(ctx) {
  const earlier = ctx.earlier === "1";
  return `
    <div class="log-window-note">
      <span data-line-window>Lines <bdi>${earlier ? "49,841–49,920" : "49,921–50,000"}</bdi> of <bdi>50,000</bdi> · earlier lines omitted</span>
      <a href="${h(link(ctx, "job", { earlier: earlier ? "" : "1" }))}">
        ${earlier ? "Return to latest lines" : "Show earlier lines"}
      </a>
      <a href="/raw-log.txt" download="windows-tests.log">Download raw</a>
    </div>`;
}

function connectionNotice(ctx) {
  if (ctx.state !== "disconnected") return "";
  return `
    <p class="connection-notice" role="status">
      ${icon("queued")} Connection lost. Showing the last received output.
      <a href="${h(link(ctx, "job", { state: "running" }))}">Retry</a>
    </p>`;
}

function lineRow(line, query) {
  return `
    <li id="L${line.number}" class="log-line ${line.failure ? "error-line" : ""}">
      <a href="#L${line.number}" class="line-number"
        aria-label="Line ${line.number}">${line.number}</a>
      <code>${highlight(line.text, query)}</code>
    </li>`;
}

function logWindow(ctx, lines) {
  return `
    <div class="log-scroll" id="log-window" role="region"
      aria-label="Run workspace tests log, 80 visible lines" tabindex="0" dir="ltr">
      <ol class="log-lines" start="${lines[0].number}">
        ${lines.map((line) => lineRow(line, ctx.q)).join("")}
      </ol>
    </div>`;
}

function footer(ctx, matches) {
  const message = ctx.q
    ? `${matches} matching lines in this window`
    : ctx.state === "running" ? "Live output · follow is paused" : "End of log · exit code 101";
  return `
    <div class="log-footer">
      <span data-log-feedback role="status" aria-live="polite">${message}</span>
      <span class="muted">UTF-8 · timestamps hidden</span>
    </div>`;
}

export function jobLog(ctx) {
  if (ctx.state === "expired") {
    return `<div class="log-placeholder">${empty(
      "This log has expired",
      "Log retention removed the stored output. The job status and timings remain available.",
    )}</div>`;
  }
  const lines = logLines(ctx.state, ctx.earlier === "1");
  const matches = ctx.q ? lines.filter((line) => line.text.toLowerCase().includes(ctx.q.toLowerCase())).length : 0;
  const truncated = ctx.state === "truncated" ? `
    <p class="notice">This job exceeded the log retention limit. Later lines were dropped; downloading the raw log cannot recover them.</p>` : "";
  return `${toolbar(ctx)}${windowNavigation(ctx)}${connectionNotice(ctx)}${logWindow(ctx, lines)}${footer(ctx, matches)}${truncated}`;
}
