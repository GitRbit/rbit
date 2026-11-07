import assert from "node:assert/strict";
import test from "node:test";
import { postRevocationPoll } from "../../devtools/actions-visual/production-poll.mjs";
import { productionOrigin, productionRun } from "../../devtools/actions-visual/production-policy.mjs";

async function observed(scenario) {
  const listeners = new Map(), previous = globalThis.document;
  globalThis.document = {
    addEventListener: (name, handler) => listeners.set(name, handler),
    removeEventListener: (name, handler) => { assert.equal(listeners.get(name), handler); listeners.delete(name); },
  };
  const xhr = () => ({ status: 200, responseURL: productionOrigin + productionRun });
  const emit = (name, request, change = {}) => listeners.get(name)?.({ target: { id: "run" },
    detail: { xhr: request, target: { id: "run" }, requestConfig: { verb: "get", path: productionRun } }, ...change });
  try {
    const pending = postRevocationPoll({ evaluate: operation => operation({ origin: productionOrigin, path: productionRun }) });
    await scenario({ pending, emit, xhr, listeners });
    assert.equal(listeners.size, 0, "all success/failure listeners must be removed");
  } finally { globalThis.document = previous; }
}

test("pre-ACK in-flight swaps cannot prove revocation; a new exact GET and its own swap must complete", async () => {
  await observed(async ({ pending, emit, xhr }) => {
    let complete = false; pending.then(() => { complete = true; });
    const old = xhr(); emit("htmx:afterSwap", old);
    await Promise.resolve(); assert.equal(complete, false);
    const foreign = xhr();
    emit("htmx:beforeRequest", foreign, { detail: { xhr: foreign, target: { id: "run" },
      requestConfig: { verb: "get", path: "/alice/rocket/actions/runs/9" } } });
    emit("htmx:afterSwap", foreign);
    await Promise.resolve(); assert.equal(complete, false);
    const current = xhr(); emit("htmx:beforeRequest", current);
    emit("htmx:afterSwap", old);
    await Promise.resolve(); assert.equal(complete, false);
    emit("htmx:afterSwap", current); await pending;
  });
});

test("redirected, failed or wrong-root responses cannot become a positive same-run swap proof", async () => {
  for (const failure of ["redirect", "status", "send-error"]) {
    await observed(async ({ pending, emit, xhr }) => {
      const current = xhr(); emit("htmx:beforeRequest", current);
      emit("htmx:afterSwap", current, { target: { id: "other" } });
      if (failure === "redirect") current.responseURL = "https://elsewhere.test/private";
      if (failure === "status") current.status = 403;
      emit(failure === "send-error" ? "htmx:sendError" : "htmx:afterSwap", current);
      await assert.rejects(pending, /PRODUCTION_REVOKE_POLL_UNPROVED/);
    });
  }
});

test("an absent natural poll fails at the existing ten-second bound and cleans listeners", async () => {
  const previous = globalThis.setTimeout;
  let expire;
  globalThis.setTimeout = (callback, ms) => { assert.equal(ms, 10000); expire = callback; return undefined; };
  try {
    await observed(async ({ pending }) => { expire(); await assert.rejects(pending, /PRODUCTION_REVOKE_POLL_UNPROVED/); });
  } finally { globalThis.setTimeout = previous; }
});
