// Keep sticky bars flush at fractional zoom/device scales and after navigation.
const root = document.documentElement;
let observed = [];
const observer = new ResizeObserver(measureBars);

function bars() {
  return [document.querySelector('body > .site-header'), document.querySelector('body > .site-subnav')];
}

function measureBars() {
  const heights = bars().map(bar => bar?.isConnected ? bar.getBoundingClientRect().height : null);
  for (const [index, property] of ['--bar-height', '--subnav-height'].entries()) {
    if (heights[index] === null) root.style.removeProperty(property);
    else root.style.setProperty(property, `${heights[index]}px`);
  }
}

function trackBars() {
  const next = bars().filter(Boolean);
  for (const bar of observed) if (!next.includes(bar)) observer.unobserve(bar);
  for (const bar of next) if (!observed.includes(bar)) observer.observe(bar);
  observed = next;
  measureBars();
}

trackBars();
document.addEventListener('htmx:afterSettle', trackBars);
document.addEventListener('htmx:historyRestore', trackBars);
