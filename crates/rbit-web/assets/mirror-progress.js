// Poll stages reported by the worker; completion always loads fresh repository data.
const polling = new WeakSet();

// The element htmx swapped in (or the content an event names); the document otherwise.
const rootOf = (event) => {
  const root = event?.detail?.root ?? event?.detail?.elt ?? event?.target;
  return typeof root?.querySelectorAll === "function" ? root : document;
};
/** Elements in `root` matching `selector`, `root` included. */
const within = (root, selector) => {
  const found = [...root.querySelectorAll(selector)];
  if (root.matches?.(selector)) found.unshift(root);
  return found;
};

function initializeMirrorProgress(event) {
  for (const panel of within(rootOf(event), "[data-mirror-progress]")) {
    if (polling.has(panel)) continue;
    polling.add(panel);
    let delay = 1500;
    const poll = async () => {
      if (!panel.isConnected) return;
      try {
        const response = await fetch(panel.dataset.statusUrl, {
          credentials: "same-origin",
          cache: "no-store",
          headers: { Accept: "application/json" },
        });
        if (!response.ok || response.redirected) throw new Error("Status unavailable");
        const status = await response.json();
        if (!panel.isConnected) return;
        if (status.complete) {
          window.location.replace(panel.dataset.homeUrl);
          return;
        }
        panel.dataset.phase = status.phase;
        panel.querySelector("[data-mirror-phase]").textContent = status.phase;
        panel.querySelector("[data-mirror-message]").textContent = status.message;
        panel.querySelector("[data-mirror-indicator]").hidden = status.phase === "failed";
        delay = status.phase === "failed" ? 10000 : 1500;
      } catch {
        if (!panel.isConnected) return;
        panel.querySelector("[data-mirror-message]").textContent = "Unable to check mirror status. Reconnecting automatically…";
        delay = Math.min(delay * 2, 15000);
      }
      if (panel.isConnected) setTimeout(poll, delay);
    };
    poll();
  }
}

initializeMirrorProgress();
document.addEventListener("htmx:afterSettle", initializeMirrorProgress);
window.addEventListener("pageshow", initializeMirrorProgress);
