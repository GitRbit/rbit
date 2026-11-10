// The run layout the Actions run and job pages share (templates/actions/run/sidebar.html).
// The run's navigation folds into a closed disclosure on phones and is always open beside
// the page from `md` up.
const narrow = window.matchMedia("(width < 48rem)");
function fitNavigation() {
  for (const details of document.querySelectorAll("details[data-run-nav]")) {
    if (!narrow.matches) details.open = true;
    else if (!details.dataset.fitted) details.open = false;
    details.dataset.fitted = "1";
  }
}
narrow.addEventListener("change", () => {
  for (const details of document.querySelectorAll("details[data-run-nav]")) delete details.dataset.fitted;
  fitNavigation();
});
if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", fitNavigation);
else fitNavigation();
document.addEventListener("htmx:load", fitNavigation);

// Running durations (`data-since`, Unix seconds) tick every second between polls, from
// the server's clock and in the page's language: `data-run-clock` holds the server time
// and the shapes of `1h 2m`, `3m 4s` and `5s` with 11, 22, 33, 44 and 55 for the numbers.
const skews = new WeakMap();
function elapsedText(clock, seconds) {
  const hours = Math.floor(seconds / 3600), minutes = Math.floor((seconds % 3600) / 60), rest = seconds % 60;
  const fill = (pattern, marks) => pattern?.replace(/11|22|33|44|55/g, mark => String(marks[mark] ?? mark));
  if (hours) return fill(clock.dataset.clockHm, { 11: hours, 22: minutes });
  if (minutes) return fill(clock.dataset.clockMs, { 33: minutes, 44: rest });
  return fill(clock.dataset.clockS, { 55: rest });
}
function tick() {
  const clock = document.querySelector("[data-run-clock]");
  if (!clock) return;
  const now = Date.now() / 1000;
  if (!skews.has(clock)) skews.set(clock, Number(clock.dataset.runClock) - now);
  const serverNow = now + skews.get(clock);
  for (const element of document.querySelectorAll("[data-since]")) {
    const since = Number(element.dataset.since);
    if (!Number.isFinite(since)) continue;
    const text = elapsedText(clock, Math.max(0, Math.floor(serverNow - since)));
    if (text && element.textContent !== text) element.textContent = text;
  }
}
setInterval(tick, 1000);
