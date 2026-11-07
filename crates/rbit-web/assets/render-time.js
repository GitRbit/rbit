// Keep cached full-page footers and HTMX swaps on the actual response timing.
const installed = new WeakSet();
const validDuration = value => typeof value === "number" && Number.isFinite(value) && value >= 0;
const headerMetrics = {
  view: /(?:^|,)\s*view;dur=(\d+(?:\.\d+)?)(?=\s*(?:;|,|$))/,
  render: /(?:^|,)\s*render;dur=(\d+(?:\.\d+)?)(?=\s*(?:;|,|$))/,
};

function headerDuration(timing, name) {
  const match = timing && headerMetrics[name].exec(timing);
  const duration = match ? Number(match[1]) : null;
  return validDuration(duration) ? duration : null;
}

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
    if (!validDuration(duration)) return;
    renderTime = formatDuration(duration);
    show();
  };

  // A revalidated navigation can reuse HTML containing an earlier footer. Its
  // navigation entry exposes the new Server-Timing header without another fetch.
  try {
    const navigation = performance?.getEntriesByType?.("navigation")?.[0];
    const metrics = navigation?.serverTiming ?? [];
    const metric = metrics.find(entry => entry?.name === "view" && validDuration(entry.duration))
      ?? metrics.find(entry => entry?.name === "render" && validDuration(entry.duration));
    if (metric) update(metric.duration);
  } catch { /* Keep server-rendered text when navigation timing is unavailable. */ }

  document.addEventListener("htmx:afterRequest", event => {
    if (!event.detail.successful) return;
    const timing = event.detail.xhr?.getResponseHeader("Server-Timing");
    update(headerDuration(timing, "view") ?? headerDuration(timing, "render"));
  });
  // A full-page swap can bring a new footer; apply only to the swapped subtree.
  document.addEventListener("htmx:afterSettle", event => {
    if (event.detail.elt?.querySelectorAll) show(event.detail.elt);
  });
}
