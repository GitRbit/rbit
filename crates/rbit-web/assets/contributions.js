// Immediate count tooltips and persistent day selection, including after htmx swaps.
const dayOf = target => target instanceof Element ? target.closest('[data-contribution-day]') : null;
const dateFormatter = new Intl.DateTimeFormat(globalThis.rbitMessages?.locale() ?? globalThis.document?.documentElement?.lang ?? undefined, { dateStyle: 'full', timeZone: 'UTC' });
const dateLabel = value => dateFormatter.format(new Date(`${value}T00:00:00Z`));
const countLabel = count => globalThis.rbitMessages?.plural("browser-commit", count) ?? `${count} commit${count === 1 ? '' : 's'}`;

// The tooltips on show. Hiding runs on every scroll event of every page, so it touches
// only these instead of searching the document (a full walk of a large diff's DOM).
const shown = new Set();

function showDayTooltip(day) {
  // Keep the SVG title as the no-script fallback, without a second delayed tooltip.
  day.querySelector('title')?.remove();
  const calendar = day.closest('[data-contributions]');
  const tooltip = calendar.querySelector('[data-contribution-tooltip]');
  tooltip.textContent = (globalThis.rbitMessages?.text("browser-contribution-day", { count: countLabel(Number(day.dataset.count)), date: dateLabel(day.dataset.contributionDay) }) ?? `${countLabel(Number(day.dataset.count))} on ${dateLabel(day.dataset.contributionDay)}`);
  tooltip.hidden = false;
  shown.add(tooltip);
  const box = day.getBoundingClientRect();
  tooltip.style.left = `${Math.max(8, Math.min(innerWidth - tooltip.offsetWidth - 8, box.left + box.width / 2 - tooltip.offsetWidth / 2))}px`;
  tooltip.style.top = `${box.top >= tooltip.offsetHeight + 12 ? box.top - tooltip.offsetHeight - 8 : box.bottom + 8}px`;
}

function hideTooltips() {
  if (!shown.size) return;
  for (const tooltip of shown) tooltip.hidden = true;
  shown.clear();
}

function selectDay(day) {
  const calendar = day.closest('[data-contributions]');
  const date = day.dataset.contributionDay;
  let monthly = 0;
  // Desktop and mobile calendars contain copies of each date; count each once.
  const seen = new Set();
  for (const item of calendar.querySelectorAll('[data-contribution-day]')) {
    item.setAttribute('aria-pressed', String(item.dataset.contributionDay === date));
    if (!seen.has(item.dataset.contributionDay) && item.dataset.contributionDay.slice(0, 7) === date.slice(0, 7)) monthly += Number(item.dataset.count);
    seen.add(item.dataset.contributionDay);
  }
  const count = Number(day.dataset.count);
  const total = Number(calendar.dataset.total);
  const details = calendar.querySelector('[data-contribution-details]');
  const heading = document.createElement('strong');
  heading.textContent = `${dateLabel(date)} · ${countLabel(count)}`;
  const info = document.createElement('p');
  info.className = 'mt-1 text-xs text-ink-muted';
  const partial = Boolean(calendar.querySelector('[data-partial-activity]'));
  const percent = globalThis.rbitMessages?.number(total ? count / total : 0, { style: "percent", maximumFractionDigits: 1 }) ?? `${total ? (count / total * 100).toFixed(1) : '0'}%`;
  info.textContent = globalThis.rbitMessages?.text(partial ? "browser-contribution-summary-partial" : "browser-contribution-summary", { count: countLabel(monthly), percent })
    ?? `${countLabel(monthly)} in this month · ${total ? (count / total * 100).toFixed(1) : '0'}% of this year's commits. Author dates from 00:00 to 23:59 UTC. Counts follow the scope and visibility described below.${partial ? ' Activity is partial because the history scan limit was reached.' : ''}`;
  details.replaceChildren(heading, info);
}

document.addEventListener('pointerover', event => {
  const day = dayOf(event.target);
  if (day) showDayTooltip(day);
});
document.addEventListener('focusin', event => {
  const day = dayOf(event.target);
  if (day) showDayTooltip(day);
});
document.addEventListener('pointerout', event => { if (dayOf(event.target)) hideTooltips(); });
document.addEventListener('focusout', event => { if (dayOf(event.target)) hideTooltips(); });
document.addEventListener('click', event => {
  const day = dayOf(event.target);
  if (day) selectDay(day);
});
document.addEventListener('keydown', event => {
  if (event.key === 'Escape') hideTooltips();
  const day = dayOf(event.target);
  if (!day) return;
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    selectDay(day);
  } else if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
    event.preventDefault();
    const days = [...day.closest('svg').querySelectorAll('[data-contribution-day]')];
    const offset = { ArrowLeft: -7, ArrowRight: 7, ArrowUp: -1, ArrowDown: 1 }[event.key];
    days[Math.max(0, Math.min(days.length - 1, days.indexOf(day) + offset))]?.focus();
  }
});
addEventListener('scroll', hideTooltips, { passive: true });
addEventListener('resize', hideTooltips);
document.addEventListener('htmx:beforeSwap', hideTooltips);
