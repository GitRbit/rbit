// Actions job pages (templates/actions/job.html). Every page works without this file:
// steps and log groups are native details, line numbers are plain links, and a reload
// shows the latest log. With it:
//
// * A running job's log is tailed. The running step's new lines are fetched from
//   `…/logs/live` (routes/actions/job_live.rs) and appended where the console ends, inside
//   the group still being written; the page follows them while the viewer keeps the end
//   of that step in view. When a step starts or ends, or the job ends, the job section is
//   reloaded instead, and polling stops with the job.
// * Steps keep what the viewer opened or closed across those reloads, and the step being
//   watched stays open once it is done unless the job moved on to another running step.
// * Running durations tick, and a linked line (#step-3-line-12) opens its step and group.

const JOB = "#job";
/** How long to wait between asks: idle, after new lines, after a failure, while hidden. */
const WAIT = { idle: 1500, busy: 250, failed: 6000, hidden: 10000 };
/** The most rows one console keeps; the oldest go first. */
const MAX_ROWS = 20000;

/** The viewer's choices on the job page they are on: open or not, by step. */
let choices = { path: null, steps: new Map() };
/** The steps open just before a reload of the job, or `null`. */
let openBefore = null;

function chosen() {
  if (choices.path !== location.pathname) choices = { path: location.pathname, steps: new Map() };
  return choices.steps;
}

const stepsOf = (job) => (job ? [...job.querySelectorAll("details[data-step]")] : []);

// A click on a step's summary (or Enter or Space on it) is the viewer's choice; a reload
// setting `open` is not, so `toggle` cannot tell them apart.
document.addEventListener("click", (event) => {
  if (event.defaultPrevented || !(event.target instanceof Element)) return;
  const summary = event.target.closest(`${JOB} details[data-step] > summary`);
  if (!summary) return;
  const step = summary.parentElement;
  chosen().set(step.dataset.step, !step.open);
});

document.addEventListener("htmx:beforeSwap", (event) => {
  const job = event.target instanceof Element && event.target.matches(JOB) ? event.target : null;
  openBefore = job ? new Set(stepsOf(job).filter((step) => step.open).map((step) => step.dataset.step)) : null;
});

// The reloaded job replaced the old one: look it up again.
document.addEventListener("htmx:afterSwap", () => {
  const before = openBefore;
  openBefore = null;
  if (before) {
    const steps = stepsOf(document.querySelector(JOB));
    const running = steps.find((step) => step.dataset.state === "running");
    const choice = chosen();
    for (const step of steps) {
      const key = step.dataset.step;
      if (choice.has(key)) step.open = choice.get(key);
      else if (before.has(key) && (!running || running === step)) step.open = true;
    }
  }
  start();
});

// ------------------------------------------------------------------ live tail

let timer = null;
let asking = false;
/** Whether the page keeps the end of the running step in view. */
let following = true;

const liveJob = () => document.querySelector?.(`${JOB}[data-live-url]`) ?? null;
const tailOf = (job) => job.querySelector(".log-console[data-next]");

function schedule(delay) {
  clearTimeout(timer);
  timer = setTimeout(ask, document.hidden ? WAIT.hidden : delay);
}

/** Reloads the job section; the swap handlers above keep the steps' state. */
function reload(job) {
  clearTimeout(timer);
  const htmx = globalThis.htmx;
  if (htmx?.ajax) htmx.ajax("GET", job.dataset.selfUrl || location.pathname, { target: JOB, select: JOB, swap: "outerHTML" });
  else location.reload();
}

async function ask() {
  const job = liveJob();
  if (!job || asking) return;
  const tail = tailOf(job);
  const url = new URL(job.dataset.liveUrl, location.href);
  url.searchParams.set("from", tail?.dataset.next ?? "0");
  asking = true;
  let delay = WAIT.idle;
  try {
    const response = await fetch(url, { headers: { Accept: "application/json" }, cache: "no-store", credentials: "same-origin" });
    if (!response.ok) throw new Error(`live log: ${response.status}`);
    const answer = await response.json();
    if (liveJob() !== job) {
      delay = 0;
    } else if (answer.state !== job.dataset.liveState || !answer.active) {
      delay = null;
      reload(job);
    } else if (tail && answer.target === tail.dataset.log) {
      if (answer.html) {
        append(tail, answer.html);
        delay = WAIT.busy;
      }
      tail.dataset.next = String(answer.next);
    }
  } catch {
    delay = WAIT.failed;
  } finally {
    asking = false;
  }
  // Another job page may have replaced this one meanwhile: it is asked for at once.
  if (delay !== null && liveJob()) schedule(delay);
}

/** The bottom of `tail` is in view (or close): the page is following it. */
function atEnd(tail) {
  return tail.getBoundingClientRect().bottom <= innerHeight + 80;
}

/** Appends server-rendered rows to `tail`, continuing the group it left open. */
function append(tail, html) {
  const follow = following && tail.closest("details")?.open !== false;
  const template = document.createElement("template");
  template.innerHTML = html;
  tail.parentElement?.querySelector("[data-log-empty]")?.remove();
  let group = tail.lastElementChild?.matches("details[data-unended]") ? tail.lastElementChild : null;
  const end = () => {
    group?.removeAttribute("data-unended");
    group = null;
  };
  for (const node of [...template.content.children]) {
    if (node.matches("[data-endgroup]")) {
      end();
    } else if (node.matches("details.log-group")) {
      end();
      tail.append(node);
      if (node.hasAttribute("data-unended")) group = node;
    } else {
      (group?.querySelector(":scope > .log-group-body") ?? tail).append(node);
    }
  }
  while (tail.childElementCount > MAX_ROWS) tail.firstElementChild.remove();
  if (follow) tail.lastElementChild?.scrollIntoView({ block: "end" });
  document.dispatchEvent(new CustomEvent("actions-log:append"));
}

addEventListener("scroll", () => {
  const job = liveJob();
  const tail = job && tailOf(job);
  if (tail) following = atEnd(tail);
}, { passive: true });

document.addEventListener("visibilitychange", () => {
  if (!document.hidden && liveJob()) schedule(0);
});

// ------------------------------------------------------------------ durations and links

/** `seconds` in narrow units in the page's language: `1h 2m`, `1m 3s`, `12s`. */
function took(seconds, language) {
  const unit = (value, name) => {
    try {
      return new Intl.NumberFormat(language, { style: "unit", unit: name, unitDisplay: "narrow" }).format(value);
    } catch {
      return `${value}${name[0]}`;
    }
  };
  const [hours, minutes, rest] = [Math.floor(seconds / 3600), Math.floor((seconds % 3600) / 60), seconds % 60];
  if (hours) return `${unit(hours, "hour")} ${unit(minutes, "minute")}`;
  if (minutes) return `${unit(minutes, "minute")} ${unit(rest, "second")}`;
  return unit(rest, "second");
}

let skew = null;
setInterval(() => {
  const job = document.querySelector?.(JOB);
  if (!job?.dataset.now) return;
  skew ??= Date.now() / 1000 - Number(job.dataset.now);
  const now = Date.now() / 1000 - skew;
  for (const clock of job.querySelectorAll("[data-started]")) {
    clock.textContent = took(Math.max(0, Math.floor(now - Number(clock.dataset.started))), job.lang || undefined);
  }
}, 1000);

/** Opens the step and group of the linked line and brings it into view. */
function reveal() {
  const id = decodeURIComponent(location.hash.slice(1));
  if (!/^step-[a-z0-9]+-line-\d+$/.test(id)) return false;
  const row = document.getElementById(id);
  if (!row) return false;
  for (let details = row.closest("details"); details; details = details.parentElement?.closest("details")) details.open = true;
  following = false;
  row.scrollIntoView({ block: "center" });
  return true;
}

addEventListener("hashchange", reveal);

/** Starts tailing a running job's page (once), or follows its running step. */
function start() {
  skew = null;
  const job = liveJob();
  if (!job) return;
  const tail = tailOf(job);
  if (!reveal() && following && tail?.closest("details")?.open) tail.lastElementChild?.scrollIntoView({ block: "end" });
  if (!asking) schedule(WAIT.idle);
}

if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
else start();
