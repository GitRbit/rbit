// Preserve native PRG while refusing unverified POST replay/redirect responses.
// Playwright1.65 routes only the initial request, so GET hops are audited, not
// claimed to be preemptively blocked. The network namespace contains real egress.
import { productionOrigin, productionLanguage } from "./production-policy.mjs";
const postUrl = productionOrigin + productionLanguage;
const savedUrl = postUrl + "?saved=1";

export function languageTransport(context, faults) {
  const posts = new WeakSet(), chain = new WeakSet(), expectedGets = new Set();
  let submitted = 0, postResponses = 0, completedGets = 0;
  const fault = () => faults.push("unexpected-language-redirect");
  context.on("request", request => {
    const previous = request.redirectedFrom();
    if (!previous || (!posts.has(previous) && !chain.has(previous))) return;
    chain.add(request);
    if (!posts.has(previous) || request.method() !== "GET" || request.url() !== savedUrl || request.postData() !== null) {
      fault(); return;
    }
    expectedGets.add(request);
  });
  context.on("response", response => {
    const request = response.request();
    if (posts.has(request)) {
      if (response.status() !== 303 || response.url() !== postUrl) fault();
      else postResponses++;
    } else if (expectedGets.delete(request)) {
      if (response.status() !== 200 || response.url() !== savedUrl) fault();
      else completedGets++;
    }
  });
  return {
    async post(route, request) {
      if (request.method() !== "POST" || request.url() !== postUrl || posts.has(request)) {
        faults.push("language-post-refused"); await route.abort("blockedbyclient"); return;
      }
      let response, fulfilled = false;
      try {
        posts.add(request); submitted++;
        // Never follow or automatically retry a mutation in the API transport.
        response = await route.fetch({ maxRedirects: 0, maxRetries: 0, timeout: 10000 });
        const locations = response.headersArray().filter(header => header.name.toLowerCase() === "location");
        if (response.status() !== 303 || response.url() !== postUrl || locations.length !== 1
          || locations[0].value !== productionLanguage + "?saved=1") throw Error("LANGUAGE_POST_RESPONSE_REFUSED");
        // Forward the exact verified303, including its actual cookie headers.
        // The browser, including no-JS mode, performs its native POST/Redirect/GET.
        await route.fulfill({ response });
        fulfilled = true;
      } catch {
        faults.push("language-post-response-refused");
        if (!fulfilled) {
          try { await route.abort("blockedbyclient"); }
          catch { faults.push("language-post-abort-failed"); }
        }
      } finally {
        if (response) {
          try { await response.dispose(); }
          catch { faults.push("language-post-dispose-failed"); }
        }
      }
    },
    complete() {
      if (faults.length || expectedGets.size || submitted !== postResponses || submitted !== completedGets) {
        throw Error("PRODUCTION_LANGUAGE_CHAIN_UNPROVED");
      }
    },
  };
}
