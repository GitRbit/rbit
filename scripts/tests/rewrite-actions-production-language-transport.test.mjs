import assert from "node:assert/strict";
import test from "node:test";
import { EventEmitter } from "node:events";
import { languageTransport } from "../../devtools/actions-visual/production-language-transport.mjs";
import { productionOrigin, productionLanguage } from "../../devtools/actions-visual/production-policy.mjs";
const postUrl = productionOrigin + productionLanguage, savedUrl = postUrl + "?saved=1";
const request = (url = postUrl, method = "POST", previous = null) => ({ url: () => url, method: () => method,
  redirectedFrom: () => previous, postData: () => method === "POST" ? "locale=es" : null });
function setup({ status = 303, locations = [productionLanguage + "?saved=1"], fetchFails = false } = {}) {
  const context = new EventEmitter(), faults = [], events = [];
  const owner = languageTransport(context, faults), initial = request();
  const actual = { status: () => status, url: () => postUrl,
    headersArray: () => locations.map(value => ({ name: "Location", value })).concat([{ name: "Set-Cookie", value: "PRIVATE-COOKIE" }]),
    dispose: async () => events.push("disposed") };
  const route = {
    fetch: async options => {
      events.push("fetch");
      assert.deepEqual(options, { maxRedirects: 0, maxRetries: 0, timeout: 10000 });
      if (fetchFails) throw Error("PRIVATE fetch diagnostics");
      return actual;
    },
    fulfill: async options => { assert.equal(options.response, actual, "forward exact verified response/cookies"); events.push("fulfilled"); },
    abort: async code => { assert.equal(code, "blockedbyclient"); events.push("aborted"); },
  };
  return { context, owner, initial, route, faults, events };
}
function browserResponse(context, request, status) {
  context.emit("response", { request: () => request, status: () => status, url: request.url });
}

test("unfollowed real303 is forwarded unchanged and requires the browser's actual fixedGET200", async () => {
  const value = setup();
  await value.owner.post(value.route, value.initial);
  assert.deepEqual(value.events, ["fetch", "fulfilled", "disposed"]);
  assert.throws(() => value.owner.complete(), /PRODUCTION_LANGUAGE_CHAIN_UNPROVED/);
  browserResponse(value.context, value.initial, 303);
  const redirected = request(savedUrl, "GET", value.initial);
  value.context.emit("request", redirected);
  assert.throws(() => value.owner.complete(), /PRODUCTION_LANGUAGE_CHAIN_UNPROVED/);
  browserResponse(value.context, redirected, 200);
  value.owner.complete();
  assert.deepEqual(value.faults, []);
});

test("hostilePOSTredirects are refused before forwarding or following any hop", async () => {
  for (const input of [{ status: 307 }, { status: 308 }, { status: 302 }, { status: 200 },
    { locations: ["https://example.invalid/"] }, { locations: ["/logout"] },
    { locations: [savedUrl] }, { locations: [productionLanguage + "?saved=1\n"] },
    { locations: [] }, { locations: [productionLanguage + "?saved=1", "/private"] }]) {
    const value = setup(input);
    await value.owner.post(value.route, value.initial);
    assert.deepEqual(value.events, ["fetch", "aborted", "disposed"]);
    assert.deepEqual(value.faults, ["language-post-response-refused"]);
    assert.throws(() => value.owner.complete(), /PRODUCTION_LANGUAGE_CHAIN_UNPROVED/);
    assert.ok(!JSON.stringify(value.faults).includes("example.invalid"));
  }
});

test("a failedfetch is not retried, followed, fulfilled or reflected in public diagnostics", async () => {
  const value = setup({ fetchFails: true });
  await value.owner.post(value.route, value.initial);
  assert.deepEqual(value.events, ["fetch", "aborted"]);
  assert.deepEqual(value.faults, ["language-post-response-refused"]);
});

test("unexpected issuedGEThops are observed as faults, never mislabeled preemptively blocked", async () => {
  for (const target of ["https://example.invalid/", productionOrigin + "/logout"]) {
    const value = setup(); await value.owner.post(value.route, value.initial);
    browserResponse(value.context, value.initial, 303);
    const wrong = request(target, "GET", value.initial);
    value.context.emit("request", wrong);
    assert.deepEqual(value.faults, ["unexpected-language-redirect"]);
    assert.throws(() => value.owner.complete(), /PRODUCTION_LANGUAGE_CHAIN_UNPROVED/);
    assert.deepEqual(value.events, ["fetch", "fulfilled", "disposed"], "observer cannot revoke an already issued hop");
  }
  const value = setup(); await value.owner.post(value.route, value.initial);
  browserResponse(value.context, value.initial, 303);
  const first = request(savedUrl, "GET", value.initial);
  value.context.emit("request", first); browserResponse(value.context, first, 302);
  value.context.emit("request", request(productionOrigin + "/private", "GET", first));
  assert.deepEqual(value.faults, ["unexpected-language-redirect", "unexpected-language-redirect"]);
  assert.throws(() => value.owner.complete(), /PRODUCTION_LANGUAGE_CHAIN_UNPROVED/);
});

test("a replayedPOSTroute cannot execute twice and unknown mutation routes refuse beforefetch", async () => {
  const value = setup();
  await value.owner.post(value.route, value.initial);
  await value.owner.post(value.route, value.initial);
  await value.owner.post(value.route, request(productionOrigin + "/logout"));
  assert.deepEqual(value.events, ["fetch", "fulfilled", "disposed", "aborted", "aborted"]);
  assert.deepEqual(value.faults, ["language-post-refused", "language-post-refused"]);
});
