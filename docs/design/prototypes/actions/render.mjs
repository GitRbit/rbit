import { h, icon, intent, link, hidden, empty, defaults } from "./ui.mjs";
import { overview } from "./overview.mjs";
import { runPage } from "./run.mjs";
import { jobPage } from "./job.mjs";

const states = ["failed", "success", "cancelled", "running", "approval", "deployment", "loading", "empty", "error", "disabled", "denied", "disconnected", "truncated", "expired", "queued"];
export function context(path, query) {
  const screen = path === "/" ? "overview" : path.slice(1);
  if (!["overview", "run", "job"].includes(screen)) throw new Error("Unknown prototype screen");
  const result = { screen, ...defaults };
  for (const key of Object.keys(result)) if (key !== "screen" && query.has(key)) result[key] = query.get(key).slice(0, 160);
  if (!states.includes(result.state) || !["system", "light", "dark", "dna-day", "dna-dark"].includes(result.theme) || !["ltr", "rtl"].includes(result.dir)
    || !["writer", "reader", "reviewer"].includes(result.viewer) || !["0", "1", "2", "3"].includes(result.workflow) || !["0", "1"].includes(result.long)) throw new Error("Unknown prototype variant");
  if (result.state === "disabled" && result.workflow === "0") result.workflow = "1";
  return result;
}

function reviewBar(ctx) {
  return `<details class="review-tools">
    <summary>Design preview <span class="muted">· synthetic fixtures · no backend requests</span>
    </summary>
    <form method="get" action="/${h(ctx.screen)}">${hidden(ctx, ["state", "theme", "dir", "viewer", "long"])}
    <label>State<select class="input" name="state">${states.map((state) => `<option ${state === ctx.state ? "selected" : ""}>${state}</option>`).join("")}</select>
    </label>
    <label>Theme<select class="input" name="theme">${["dna-day", "dna-dark", "light", "dark", "system"].map((theme) => `<option ${ctx.theme === theme ? "selected" : ""}>${theme}</option>`).join("")}</select>
    </label>
    <label>Layout<select class="input" name="dir">
    <option value="ltr">LTR</option>
    <option value="rtl" ${ctx.dir === "rtl" ? "selected" : ""}>RTL diagnostic</option>
    </select>
    </label>
    <label>Permissions<select class="input" name="viewer">${["writer", "reader", "reviewer"].map((viewer) => `<option ${ctx.viewer === viewer ? "selected" : ""}>${viewer}</option>`).join("")}</select>
    </label>
    <label>Labels<select class="input" name="long">
    <option value="0">Normal</option>
    <option value="1" ${ctx.long === "1" ? "selected" : ""}>Long diagnostic</option>
    </select>
    </label>
    <button class="btn">Apply preview</button>
    </form>
    </details>`;
}
export function render(ctx, feedback = "") {
  const main = mainContent(ctx);
  return `<!doctype html>
    <html lang="en" dir="${ctx.dir}" data-theme="${ctx.theme}">
    <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1">
    <title>Actions · rbit design preview</title>
    <link rel="stylesheet" href="/assets/app.css">
    <link rel="stylesheet" href="/prototype.css">
    <script src="/prototype.js" defer>
    </script>
    </head>
    <body>
    <a class="skip-link" href="#main">Skip to Actions content</a>
    <header class="site-header">
    <a href="${h(link(ctx, "overview"))}" class="brand">
    <img src="/assets/logo-384.avif" alt="" width="25" height="25">
    <span>rbit</span>
    </a>
    <div class="repository-crumb">
    <a href="${h(link(ctx, "overview"))}">GitRbit</a>
    <span>/</span>
    <strong>rbit</strong>
    <span class="badge">Public</span>
    </div>
    <nav aria-label="Global" class="global-nav">
    <a href="${h(intent(ctx, "explore"))}">Explore</a>
    <a href="${h(intent(ctx, "notifications"))}" aria-label="Notifications">${icon("queued")}</a>
    <span class="avatar" title="Signed in as Sam">S</span>
    </nav>
    </header>
    <div class="repository-nav">
    <nav aria-label="Repository">
    <a href="${h(intent(ctx, "code"))}">${icon("code")} Code</a>
    <a href="${h(intent(ctx, "issues"))}">Issues <span class="count">12</span>
    </a>
    <a href="${h(intent(ctx, "pulls"))}">Pull requests <span class="count">4</span>
    </a>
    <a href="${h(link(ctx, "overview"))}" aria-current="page">${icon("play")} Actions</a>
    <a href="${h(intent(ctx, "settings"))}">Settings</a>
    </nav>
    </div>
    <div class="review-wrap">${reviewBar(ctx)}${screenNavigation(ctx)}
    </div>
    <main id="main" tabindex="-1">${feedback ? `<div class="notice" role="status">${h(feedback)} No request was sent to a repository or runner.</div>` : ""}${main}</main>
    <footer class="page-footer">
    <span>rbit</span>
    <span>Review-only prototype · ${ctx.dir === "rtl" ? "RTL is a layout diagnostic, not a translated locale" : "fixtures are synthetic"}</span>
    </footer>
    </body>
    </html>`;
}

function mainContent(ctx) {
  if (ctx.state === "loading") return `<section class="box" aria-busy="true" aria-label="Loading ${h(ctx.screen)}">
    <div class="empty">
    <h1>Loading ${h(ctx.screen === "overview" ? "workflow runs" : ctx.screen === "run" ? "run details" : "job log")}…</h1>
    <p>Waiting for the next response.</p>
    <a class="btn" href="${h(link(ctx, ctx.screen, { state: "failed" }))}">Refresh</a>
    </div>
    </section>`;
  if (ctx.screen === "overview" && ctx.state === "denied") {
    return `<div class="box">${empty(
      "Actions not found", "This repository's Actions are unavailable or you do not have access.", "", true,
    )}</div>`;
  }
  if (ctx.screen === "overview") return overview(ctx);
  if (ctx.screen === "run") return runPage(ctx);
  return jobPage(ctx);
}

function screenNavigation(ctx) {
  const labels = { overview: "Overview", run: "Run overview", job: "Job detail" };
  const links = Object.entries(labels).map(([screen, label]) => `
    <a href="${h(link(ctx, screen))}" ${ctx.screen === screen ? 'aria-current="page"' : ""}>
      ${label}
    </a>`).join("");
  return `<nav class="screen-switch" aria-label="Prototype screens">${links}</nav>`;
}
