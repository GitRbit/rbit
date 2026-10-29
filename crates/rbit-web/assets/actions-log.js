// Actions job pages (templates/actions/job.html) refresh their steps every few seconds
// while the job runs, and each refresh opens the running step, or else the first failed
// one. Every page works without this file; with it, a step the viewer opened or closed
// stays that way, and the step being watched stays open once it is done (when the job
// ends, too) unless the job moved on to another running step.

const JOB = "#job";

/** The viewer's choices on the job page they are on: open or not, by step number. */
let choices = { path: null, steps: new Map() };
/** The steps open just before a refresh of the job, or `null`. */
let openBefore = null;

function chosen() {
  if (choices.path !== location.pathname) choices = { path: location.pathname, steps: new Map() };
  return choices.steps;
}

const stepsOf = (job) => (job ? [...job.querySelectorAll("details[data-step]")] : []);

// A click on a step's summary (or Enter or Space on it) is the viewer's choice; a refresh
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

// The refreshed job replaced the old one: look it up again.
document.addEventListener("htmx:afterSwap", () => {
  const before = openBefore;
  openBefore = null;
  if (!before) return;
  const steps = stepsOf(document.querySelector(JOB));
  const running = steps.find((step) => step.dataset.state === "running");
  const choice = chosen();
  for (const step of steps) {
    const number = step.dataset.step;
    if (choice.has(number)) step.open = choice.get(number);
    else if (before.has(number) && (!running || running === step)) step.open = true;
  }
});
