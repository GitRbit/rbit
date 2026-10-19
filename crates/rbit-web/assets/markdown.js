// Render only Markdown fences explicitly marked as Mermaid by the sanitizer.
const selector = 'pre > code.language-mermaid';
const mounted = new WeakSet();

function enhanceDiagrams() {
  for (const code of document.querySelectorAll(selector)) {
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
    frame.title = 'Mermaid diagram';
    frame.className = 'mermaid-frame';
    frame.setAttribute('sandbox', 'allow-scripts');
    frame.setAttribute('height', '300');
    frame.src = '/assets/mermaid-frame.html';
    frame.addEventListener('load', () => {
      frame.contentWindow.postMessage({ type: 'rbit:mermaid', source: code.textContent }, '*');
    });
    // Keep the source available without JavaScript, on errors, and as a disclosure.
    const details = document.createElement('details');
    details.className = 'mermaid-source';
    const summary = document.createElement('summary');
    summary.textContent = 'Diagram source';
    pre.before(frame, details);
    details.append(summary, pre);
  }
}

addEventListener('message', (event) => {
  if (event.data?.type !== 'rbit:mermaid:size') return;
  for (const frame of document.querySelectorAll('iframe.mermaid-frame')) {
    if (frame.contentWindow !== event.source) continue;
    const height = Number(event.data.height);
    // Include the frame's two one-pixel borders so its content does not scroll.
    if (Number.isFinite(height)) frame.setAttribute('height', String(Math.max(100, Math.min(2000, Math.ceil(height) + 2))));
    if (event.data.error) frame.nextElementSibling.open = true;
  }
});

enhanceDiagrams();
document.addEventListener('htmx:afterSettle', enhanceDiagrams);
// The language browser inserts previews without an htmx swap.
document.addEventListener('rbit:content-loaded', enhanceDiagrams);
document.addEventListener('htmx:historyRestore', enhanceDiagrams);
