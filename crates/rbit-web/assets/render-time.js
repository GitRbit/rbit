// Keep cached full-page footers and HTMX swaps on the actual response timing.
const installed = new WeakSet();

function formatDuration(ms) {
  if (ms < 1) return `${Math.round(ms * 1000)} µs`;
  if (ms < 1000) return `${ms.toFixed(1)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function installRenderTiming(document, performance = globalThis.performance) {
  if (installed.has(document)) return;
  installed.add(document);
  let renderTime = null;
  const show = (root = document) => {
    if (renderTime === null) return;
    for (const element of root.querySelectorAll("[data-render-time]")) element.textContent = renderTime;
  };
  const update = duration => {
    if (typeof duration !== "number" || !Number.isFinite(duration) || duration < 0) return;
    renderTime = formatDuration(duration);
    show();
  };

  // A revalidated navigation can reuse HTML containing an earlier footer. Its
  // navigation entry exposes the new Server-Timing header without another fetch.
  try {
    const navigation = performance?.getEntriesByType?.("navigation")?.[0];
    const metric = navigation?.serverTiming?.find(entry => entry.name === "render");
    if (metric) update(metric.duration);
  } catch { /* Keep server-rendered text when navigation timing is unavailable. */ }

  document.addEventListener("htmx:afterRequest", event => {
    if (!event.detail.successful) return;
    const timing = event.detail.xhr?.getResponseHeader("Server-Timing");
    const match = timing && /(?:^|,)\s*render;dur=(\d+(?:\.\d+)?)(?=\s*(?:;|,|$))/.exec(timing);
    if (match) update(Number(match[1]));
  });
  // A full-page swap can bring a new footer; apply only to the swapped subtree.
  document.addEventListener("htmx:afterSettle", event => {
    if (event.detail.elt?.querySelectorAll) show(event.detail.elt);
  });
}
