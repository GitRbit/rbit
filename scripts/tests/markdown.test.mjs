import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/markdown.js', import.meta.url), 'utf8');
function mount() {
  const handlers = {};
  const messages = [];
  const frames = [];
  const created = [];
  const pre = { parentElement: { matches: () => false }, before(...nodes) { frames.push(nodes[0]); } };
  const code = { parentElement: pre, textContent: 'graph TD\n A --> B' };
  const document = {
    querySelectorAll: selector => selector.startsWith('pre') ? [code] : frames,
    addEventListener: (name, callback) => { handlers[name] = callback; },
    createElement: tag => {
      const element = { tag, attributes: {}, contentWindow: { postMessage: (...args) => messages.push(args) },
        setAttribute(key, value) { this.attributes[key] = value; },
        addEventListener(name, callback) { this[name] = callback; },
        append() {} };
      created.push(element);
      return element;
    },
  };
  vm.runInNewContext(source, { document, addEventListener: document.addEventListener });
  frames[0].nextElementSibling = created.find(node => node.tag === 'details');
  return { handlers, messages, frames, code };
}

test('Mermaid rendering sends literal source only to a script-only sandbox and mounts once', () => {
  const page = mount();
  assert.equal(page.frames[0].attributes.sandbox, 'allow-scripts');
  assert.equal(page.frames[0].src, '/assets/mermaid-frame.html');
  page.frames[0].load();
  assert.equal(page.messages[0][0].source, page.code.textContent);
  page.handlers['htmx:afterSettle']();
  page.handlers['htmx:historyRestore']();
  assert.equal(page.frames.length, 1);
});

test('only the diagram frame can resize itself and errors expose the source', () => {
  const page = mount();
  const frame = page.frames[0];
  page.handlers.message({ source: {}, data: { type: 'rbit:mermaid:size', height: 700 } });
  assert.equal(frame.attributes.height, '300');
  page.handlers.message({ source: frame.contentWindow, data: { type: 'rbit:mermaid:size', height: 140.5 } });
  assert.equal(frame.attributes.height, '143', 'intrinsic size includes the frame borders');
  page.handlers.message({ source: frame.contentWindow, data: { type: 'rbit:mermaid:size', height: 9000, error: true } });
  assert.equal(frame.attributes.height, '2000');
  assert.equal(frame.nextElementSibling.open, true);
  page.handlers.message({ source: frame.contentWindow, data: { type: 'rbit:mermaid:size', height: 'NaN' } });
  assert.equal(frame.attributes.height, '2000');
});

test('history restoration replaces the serialized frame and reconnects its source', () => {
  const page = mount();
  let oldFrameRemoved = false;
  let oldDetailsRemoved = false;
  const freshPre = { before: (...nodes) => page.frames.push(nodes[0]) };
  const restoredDetails = {
    matches: selector => selector === 'details.mermaid-source',
    previousElementSibling: { matches: () => true, remove: () => { oldFrameRemoved = true; } },
    before: pre => { pre.parentElement = { matches: () => false }; },
    remove: () => { oldDetailsRemoved = true; },
  };
  freshPre.parentElement = restoredDetails;
  page.code.parentElement = freshPre;
  page.handlers['htmx:historyRestore']();
  assert.equal(oldFrameRemoved, true);
  assert.equal(oldDetailsRemoved, true);
  page.frames[1].load();
  assert.equal(page.messages[0][0].source, page.code.textContent);
});

test('files inserted into the language preview also render their diagrams', () => {
  const page = mount();
  page.code.parentElement = {
    parentElement: { matches: () => false },
    before: (...nodes) => page.frames.push(nodes[0]),
  };
  page.handlers['rbit:content-loaded']();
  assert.equal(page.frames.length, 2);
  page.frames[1].load();
  assert.equal(page.messages[0][0].source, page.code.textContent);
});

const renderer = (await readFile(new URL('../mermaid-frame.js', import.meta.url), 'utf8')).replace("import mermaid from 'mermaid';", '');
test('invalid diagrams fail independently with a readable fallback', async () => {
  let handler;
  let configuration;
  const parent = { postMessage: message => messages.push(message) };
  const messages = [];
  const output = {};
  vm.runInNewContext(renderer, {
    mermaid: { initialize: value => { configuration = value; }, render: async () => { throw new Error('parse'); } },
    parent, addEventListener: (_, callback) => { handler = callback; },
    document: { getElementById: () => output, body: { getBoundingClientRect: () => ({ height: 100 }) } },
    ResizeObserver: class { observe() {} },
  });
  await handler({ source: parent, data: { type: 'rbit:mermaid', source: 'invalid' } });
  assert.equal(configuration.securityLevel, 'strict');
  assert.match(output.textContent, /Unable to render/);
  assert.equal(messages[0].error, true);
});

test('the renderer accepts only its parent, renders once, and enforces the source limit', async () => {
  for (const length of [50000, 50001]) {
    let handler;
    let calls = 0;
    const reports = [];
    const output = {};
    const parent = { postMessage: message => reports.push(message) };
    vm.runInNewContext(renderer, {
      mermaid: { initialize() {}, render: async () => { calls++; return { svg: '<svg></svg>' }; } },
      parent, addEventListener: (_, callback) => { handler = callback; },
      document: { getElementById: () => output, body: { getBoundingClientRect: () => ({ height: 100 }) } },
      ResizeObserver: class { observe() {} },
    });
    const data = { type: 'rbit:mermaid', source: 'a'.repeat(length) };
    await handler({ source: {}, data });
    assert.equal(reports.length, 0, 'unrelated frames cannot supply diagram source');
    await handler({ source: parent, data });
    await handler({ source: parent, data });
    assert.equal(calls, length === 50000 ? 1 : 0);
    assert.equal(reports.length, 1, 'each frame renders or fails once');
    assert.equal(reports[0].error, length > 50000);
    if (length === 50000) assert.equal(output.innerHTML, '<svg></svg>');
  }
});
