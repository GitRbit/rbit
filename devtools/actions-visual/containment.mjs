import { allowedRequest } from "./policy.mjs";
export async function contain(context) {
  const faults = [];
  // Only stable categories are retained; URLs, page text and stacks are never logged.
  await context.route("**/*", route => {
    const request = route.request();
    if (allowedRequest(request.url(), request.method())) return route.continue();
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
