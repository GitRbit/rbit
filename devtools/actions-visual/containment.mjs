import { allowedRequest } from "./policy.mjs";
export async function contain(context, allow = allowedRequest, allowedPost, faults = []) {
  // Only stable categories are retained; URLs, page text and stacks are never logged.
  await context.route("**/*", async route => {
    const request = route.request();
    // Only the synthetic language form may mutate. Read the two validation
    // headers explicitly; never enumerate or retain cookies/authorization headers.
    const form = request.method() === "POST" ? { body: request.postData?.() ?? null,
      contentType: await request.headerValue?.("content-type") ?? null,
      origin: await request.headerValue?.("origin") ?? null } : undefined;
    if (allow(request.url(), request.method(), form)) {
      if (request.method() !== "POST") return route.continue();
      if (allowedPost) return allowedPost(route, request);
    }
    faults.push("unexpected-request");
    return route.abort("blockedbyclient");
  });
  await context.routeWebSocket("**/*", socket => {
    faults.push("unexpected-websocket"); socket.close();
  });
  context.on("serviceworker", () => faults.push("unexpected-service-worker"));
  context.on("page", page => {
    page.on("popup", popup => { faults.push("unexpected-popup"); void popup.close(); });
    page.on("pageerror", () => faults.push("page-error"));
    page.on("console", message => { if (message.type() === "error") faults.push("console-error"); });
  });
  await context.addInitScript(() => {
    window.__visualCspViolations = 0;
    document.addEventListener("securitypolicyviolation", () => window.__visualCspViolations++);
  });
  return faults;
}
