import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { createEnhancementScope } from "../../crates/rbit-web/assets/plugin-enhancements.js";

const source = await readFile(new URL("../../crates/rbit-web/assets/actions-log.js", import.meta.url), "utf8");

class Element {}
class Summary extends Element {
  constructor(step) { super(); this.parentElement = step; }
  closest(selector) { return selector.endsWith("> summary") ? this : null; }
}
/** A step of the job page as the server renders it. */
class Step extends Element {
  constructor(number, state, open) { super(); this.dataset = { step: String(number), state }; this.open = open; this.summary = new Summary(this); }
}
class Job extends Element {
  constructor(steps) { super(); this.steps = steps; }
  matches(selector) { return selector === "#job"; }
  querySelectorAll() { return this.steps; }
}
class Link extends Element {
  matches() { return false; }
}

function page(pathname = "/a/b/actions/runs/1/jobs/2") {
  const listeners = new Map();
  const location = { pathname };
  let job = new Job([]);
  const document = {
    addEventListener(type, listener, options) {
      listeners.set(type, listener);
      options?.signal?.addEventListener("abort", () => { if (listeners.get(type) === listener) listeners.delete(type); });
    },
    removeEventListener(type, listener) { if (listeners.get(type) === listener) listeners.delete(type); },
    querySelector: () => job,
  };
  const scope = createEnhancementScope({ document, root: () => job });
  vm.runInNewContext(source.replace("export function mount", "function mount") + "\nmount(document, context);", { document, context: scope.context, location, Element });
  const dispose = () => scope.dispose();
  const fire = (type, event) => listeners.get(type)(event);
  return {
    location,
    dispose,
    listenerCount: () => listeners.size,
    show(steps) { job = new Job(steps); },
    // The browser toggles the step after the click's listeners ran.
    click(step) { fire("click", { target: step.summary, defaultPrevented: false }); step.open = !step.open; },
    refresh(steps) { fire("htmx:beforeSwap", { target: job }); job = new Job(steps); fire("htmx:afterSwap", { target: {} }); return steps; },
    navigate(steps) { fire("htmx:beforeSwap", { target: new Link() }); job = new Job(steps); fire("htmx:afterSwap", { target: {} }); return steps; },
  };
}

const opened = steps => steps.filter(step => step.open).map(step => Number(step.dataset.step));

test("steps keep what the viewer opened or closed across refreshes", () => {
  const job = page();
  const steps = [new Step(0, "success", false), new Step(1, "running", true)];
  job.show(steps);
  job.click(steps[1]);
  job.click(steps[0]);
  assert.deepEqual(opened(job.refresh([new Step(0, "success", false), new Step(1, "running", true)])), [0]);
  assert.deepEqual(opened(job.refresh([new Step(0, "success", false), new Step(1, "failure", true)])), [0], "still closed once it failed");
});

test("the watched step follows the running one and stays open when the job ends", () => {
  const job = page();
  job.show([new Step(0, "running", true), new Step(1, "queued", false), new Step(2, "queued", false)]);
  let steps = job.refresh([new Step(0, "success", false), new Step(1, "queued", false), new Step(2, "queued", false)]);
  assert.deepEqual(opened(steps), [0], "kept open between steps");
  steps = job.refresh([new Step(0, "success", false), new Step(1, "running", true), new Step(2, "queued", false)]);
  assert.deepEqual(opened(steps), [1], "the next running step is followed");
  steps = job.refresh([new Step(0, "success", false), new Step(1, "success", false), new Step(2, "success", false)]);
  assert.deepEqual(opened(steps), [1], "the step last watched stays open");
});

test("a failure the server opens joins the watched step", () => {
  const job = page();
  job.show([new Step(0, "failure", false), new Step(1, "running", true)]);
  assert.deepEqual(opened(job.refresh([new Step(0, "failure", true), new Step(1, "success", false)])), [0, 1]);
});

test("only a refresh of the job carries its steps over", () => {
  const job = page();
  job.show([new Step(0, "running", true)]);
  assert.deepEqual(opened(job.navigate([new Step(0, "success", false)])), []);
});

test("choices belong to the job page they were made on", () => {
  const job = page();
  const step = new Step(0, "success", false);
  job.show([step]);
  job.click(step);
  job.location.pathname = "/a/b/actions/runs/1/jobs/3";
  job.navigate([new Step(0, "success", false)]);
  assert.deepEqual(opened(job.refresh([new Step(0, "success", false)])), []);
});


test("unmount removes all listeners so a new activation does not duplicate behavior", () => {
  const job = page();
  assert.equal(job.listenerCount(), 3);
  job.dispose();
  assert.equal(job.listenerCount(), 0);
  job.dispose();
  assert.equal(job.listenerCount(), 0);
});
