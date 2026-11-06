// Require one natural same-run poll started after the caller's revocation ACK.
import { productionOrigin, productionRun } from "./production-policy.mjs";

export async function postRevocationPoll(page) {
  await page.evaluate(({ origin, path }) => new Promise((resolve, reject) => {
    let owned;
    const sameRun = value => {
      try {
        const url = new URL(value, origin);
        return url.origin === origin && url.pathname === path && !url.username && !url.password;
      } catch { return false; }
    };
    const cleanup = () => {
      clearTimeout(timer);
      for (const [name, handler] of handlers) document.removeEventListener(name, handler);
    };
    const fail = () => { cleanup(); reject(Error("PRODUCTION_REVOKE_POLL_UNPROVED")); };
    const before = event => {
      const detail = event.detail;
      if (!owned && event.target?.id === "run" && detail?.target?.id === "run"
        && detail.requestConfig?.verb?.toLowerCase() === "get" && sameRun(detail.requestConfig.path)
        && detail.xhr) owned = detail.xhr;
    };
    const after = event => {
      if (!owned || event.detail?.xhr !== owned || event.target?.id !== "run") return;
      if (owned.status !== 200 || !sameRun(owned.responseURL)) { fail(); return; }
      cleanup(); resolve();
    };
    const failed = event => { if (owned && event.detail?.xhr === owned) fail(); };
    const handlers = [["htmx:beforeRequest", before], ["htmx:afterSwap", after],
      ["htmx:responseError", failed], ["htmx:sendError", failed]];
    const timer = setTimeout(fail, 10000);
    for (const [name, handler] of handlers) document.addEventListener(name, handler);
  }), { origin: productionOrigin, path: productionRun });
}
