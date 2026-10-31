// Render only Markdown fences explicitly marked as Mermaid by the sanitizer.
const selector = 'pre > code.language-mermaid';
const mounted = new WeakSet();
// Each diagram frame by its window, to answer its size messages without a page scan.
const frames = new WeakMap();

// The element htmx swapped in (or the content an event names); the document otherwise.
const rootOf = (event) => {
  const root = event?.detail?.root ?? event?.detail?.elt ?? event?.target;
  return typeof root?.querySelectorAll === 'function' ? root : document;
};
/** Elements in `root` matching `selector`, `root` included. */
const within = (root, selector) => {
  const found = [...root.querySelectorAll(selector)];
  if (root.matches?.(selector)) found.unshift(root);
  return found;
};

function enhanceDiagrams(event) {
  for (const code of within(rootOf(event), selector)) {
    const pre = code.parentElement;
    if (mounted.has(pre)) continue;
    mounted.add(pre);
    // htmx history restores serialized markup with fresh DOM nodes. Replace its
    // old frame so it receives the source again when its new document loads.
    if (pre.parentElement.matches('details.mermaid-source')) {
      const oldDetails = pre.parentElement;
      const oldFrame = oldDetails.previousElementSibling;
      if (oldFrame?.matches('iframe.mermaid-frame')) oldFrame.remove();
      oldDetails.before(pre);
      oldDetails.remove();
    }
    const frame = document.createElement('iframe');
    frame.title = (globalThis.rbitMessages?.text("browser-mermaid-diagram") ?? (globalThis.rbitMessages?.text("browser-mermaid-diagram") ?? "Mermaid diagram"));
    frame.className = 'mermaid-frame';
    frame.setAttribute('sandbox', 'allow-scripts');
    frame.setAttribute('height', '300');
    frame.src = '/assets/mermaid-frame.html';
    frame.addEventListener('load', () => {
      frames.set(frame.contentWindow, frame);
      frame.contentWindow.postMessage({ type: 'rbit:mermaid', source: code.textContent }, '*');
    });
    // Keep the source available without JavaScript, on errors, and as a disclosure.
    const details = document.createElement('details');
    details.className = 'mermaid-source';
    const summary = document.createElement('summary');
    summary.textContent = (globalThis.rbitMessages?.text("browser-diagram-source") ?? (globalThis.rbitMessages?.text("browser-diagram-source") ?? "Diagram source"));
    pre.before(frame, details);
    details.append(summary, pre);
    if (frame.contentWindow) frames.set(frame.contentWindow, frame);
  }
}

addEventListener('message', (event) => {
  if (event.data?.type !== 'rbit:mermaid:size') return;
  const frame = event.source ? frames.get(event.source) : undefined;
  if (!frame) return;
  const height = Number(event.data.height);
  // Include the frame's two one-pixel borders so its content does not scroll.
  if (Number.isFinite(height)) frame.setAttribute('height', String(Math.max(100, Math.min(2000, Math.ceil(height) + 2))));
  if (event.data.error) frame.nextElementSibling.open = true;
});

enhanceDiagrams();
document.addEventListener('htmx:afterSettle', enhanceDiagrams);
// The language browser inserts previews without an htmx swap.
document.addEventListener('rbit:content-loaded', enhanceDiagrams);
document.addEventListener('htmx:historyRestore', enhanceDiagrams);
