// One in-flight request per mounted dashboard. Navigation, hiding the tab, and
// pausing all cancel the request; retries back off without discarding the last data.
const initialized = new WeakSet();

function initialize() {
  for (const panel of document.querySelectorAll("[data-admin-monitoring]")) {
    if (initialized.has(panel)) continue;
    initialized.add(panel);
    const button = panel.querySelector("[data-monitoring-toggle]");
    const status = panel.querySelector("[data-monitoring-status]");
    const content = panel.querySelector("[data-monitoring-content]");
    let paused = false;
    let stopped = false;
    let timer;
    let controller;
    let delay = 5000;
    const setStatus = text => { if (status.textContent !== text) status.textContent = text; };

    function cancel() {
      clearTimeout(timer);
      controller?.abort();
      controller = undefined;
    }
    function stop() {
      stopped = true;
      cancel();
      document.removeEventListener("visibilitychange", visibility);
      document.removeEventListener("htmx:beforeSwap", beforeSwap);
      window.removeEventListener("pagehide", stop);
      button.removeEventListener("click", toggle);
      initialized.delete(panel);
    }
    function schedule(wait = delay) {
      clearTimeout(timer);
      if (!stopped && !paused && !document.hidden) timer = setTimeout(poll, wait);
    }
    async function poll() {
      if (!panel.isConnected) return stop();
      if (paused || document.hidden || stopped || controller) return;
      const request = new AbortController();
      controller = request;
      // The timeout also bounds a connection that never finishes its body.
      const timeout = setTimeout(() => request.abort(), 10000);
      try {
        const response = await fetch("/admin/monitoring", {
          signal: request.signal, credentials: "same-origin", cache: "no-store",
          headers: { Accept: "text/html" },
        });
        if (response.status === 401 || response.status === 403 || response.status === 404 || response.redirected) {
          setStatus("Access changed · reload to continue");
          button.disabled = true;
          return stop();
        }
        if (!response.ok) throw new Error("Monitoring unavailable");
        const fragment = new DOMParser().parseFromString(await response.text(), "text/html").querySelector("[data-monitoring-snapshot]");
        if (!fragment) throw new Error("Invalid monitoring response");
        if (request.signal.aborted || !panel.isConnected || stopped) return;
        content.replaceChildren(fragment);
        delay = 5000;
        setStatus("Live · every 5s");
      } catch {
        if (paused || document.hidden || stopped || !panel.isConnected) return;
        delay = Math.min(delay * 2, 60000);
        setStatus("Connection lost · retrying");
      } finally {
        clearTimeout(timeout);
        if (controller === request) controller = undefined;
        schedule();
      }
    }
    function visibility() {
      cancel();
      setStatus(paused ? "Paused" : document.hidden ? "Paused · tab hidden" : "Connecting…");
      schedule(0);
    }
    function beforeSwap(event) {
      const target = event.detail?.target;
      if (event.detail?.shouldSwap && target?.contains(panel)) stop();
    }
    function toggle() {
      paused = !paused;
      button.textContent = paused ? "Resume" : "Pause";
      button.setAttribute("aria-pressed", String(paused));
      visibility();
    }
    setStatus(document.hidden ? "Paused · tab hidden" : "Live · every 5s");
    button.disabled = false;
    button.textContent = "Pause";
    button.setAttribute("aria-pressed", "false");
    button.addEventListener("click", toggle);
    document.addEventListener("visibilitychange", visibility);
    document.addEventListener("htmx:beforeSwap", beforeSwap);
    window.addEventListener("pagehide", stop);
    schedule();
  }
}

initialize();
document.addEventListener("htmx:afterSettle", initialize);
document.addEventListener("htmx:historyRestore", initialize);
window.addEventListener("pageshow", initialize);
