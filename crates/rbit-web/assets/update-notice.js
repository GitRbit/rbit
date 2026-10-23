// Keep the footer current across partial swaps, preloads and older history snapshots.
const HEADER = 'Rbit-Update-Available';
const RELEASE = /^https:\/\/github\.com\/GitRbit\/rbit\/releases\/tag\/[A-Za-z0-9.+-]{1,128}$/;

function snapshot(raw) {
  if (typeof raw !== 'string') return null;
  const match = /^([0-9]{1,39});(.+)$/.exec(raw);
  if (!match) return null;
  const value = match[2];
  if (value !== 'none' && value !== 'pending' && (!RELEASE.test(value) || new URL(value).href !== value)) return null;
  return { revision: BigInt(match[1]), value, raw };
}

export function installUpdateNotice(document) {
  let latest = null;
  let release = null;
  const observe = raw => {
    const incoming = snapshot(raw);
    if (!incoming || (latest && incoming.revision <= latest.revision)) return;
    latest = incoming;
    if (incoming.value !== 'pending') release = incoming.value === 'none' ? null : incoming.value;
  };
  const show = () => {
    const target = document.querySelector('[data-update-notice]');
    if (!target) return;
    // A newly swapped full page or restored history can be newer than module memory.
    observe(target.getAttribute('data-update-notice'));
    if (!latest) return;
    target.setAttribute('data-update-notice', latest.raw);
    if (!release) { target.replaceChildren(); return; }
    if (target.querySelector('a')?.href === release) return;
    const link = document.createElement('a');
    link.href = release;
    link.className = 'link whitespace-nowrap';
    link.textContent = 'Update available';
    link.setAttribute('hx-boost', 'false');
    link.rel = 'noreferrer';
    target.replaceChildren(link);
  };
  show();
  document.addEventListener('htmx:afterRequest', event => {
    if (!event.detail.successful) return;
    observe(event.detail.xhr?.getResponseHeader(HEADER));
    show();
  });
  for (const name of ['htmx:afterSettle', 'htmx:historyRestore']) document.addEventListener(name, show);
}
