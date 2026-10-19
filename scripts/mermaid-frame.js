import mermaid from 'mermaid';

mermaid.initialize({
  startOnLoad: false,
  securityLevel: 'strict',
  suppressErrorRendering: true,
  maxTextSize: 50000,
  maxEdges: 500,
  theme: 'neutral',
});
let rendered = false;
addEventListener('message', async (event) => {
  if (event.source !== parent || event.data?.type !== 'rbit:mermaid' || rendered) return;
  rendered = true;
  const output = document.getElementById('diagram');
  let error = false;
  try {
    const source = event.data.source;
    if (typeof source !== 'string' || source.length > 50000) throw new Error('Diagram too large');
    const { svg } = await mermaid.render('rbit-diagram', source);
    output.innerHTML = svg;
  } catch {
    error = true;
    output.textContent = 'Unable to render this Mermaid diagram. See the source below.';
  }
  // scrollHeight includes the current iframe viewport and cannot shrink to a
  // short diagram. The body's intrinsic height includes its padding instead.
  const report = () => parent.postMessage({ type: 'rbit:mermaid:size', height: document.body.getBoundingClientRect().height, error }, '*');
  new ResizeObserver(report).observe(output);
  report();
});
