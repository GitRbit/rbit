// Poll stages reported by the worker; completion always loads fresh repository data.
const polling = new WeakSet();

function initializeMirrorProgress() {
  for (const panel of document.querySelectorAll("[data-mirror-progress]")) {
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
