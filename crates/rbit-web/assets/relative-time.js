// Ages in pinned fragments. The file table's last commits are cached for a year and shared
// by readers of every language, so the server sends them as a plain day ("2026-11-08") with
// the exact moment in `datetime`. This rewrites each `<time data-relative>` as an age in the
// page's language ("3 hours ago", "vor 2 Jahren"), measured from the viewer's own clock so it
// never goes stale. The units match the server's (`TimeMessage::past`): months of 30 days,
// years of 365. Without this script, or without `Intl.RelativeTimeFormat`, the day stays.
const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const MONTH = 30 * DAY;
const YEAR = 365 * DAY;

/** `[count, unit]` for a moment `seconds` ago, or `null` for under a minute. */
export function age(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  if (s < MINUTE) return null;
  if (s < HOUR) return [Math.floor(s / MINUTE), "minute"];
  if (s < DAY) return [Math.floor(s / HOUR), "hour"];
  if (s < MONTH) return [Math.floor(s / DAY), "day"];
  if (s < YEAR) return [Math.floor(s / MONTH), "month"];
  return [Math.floor(s / YEAR), "year"];
}

const formatters = new Map();
function formatter(lang, numeric) {
  const key = `${lang}|${numeric}`;
  if (!formatters.has(key)) formatters.set(key, new Intl.RelativeTimeFormat(lang || undefined, { numeric }));
  return formatters.get(key);
}

/** The age of `iso` at `now` (milliseconds) in `lang`, or `null` when it cannot be told. */
export function relativeText(iso, now, lang) {
  const then = Date.parse(iso);
  if (!Number.isFinite(then) || typeof Intl === "undefined" || typeof Intl.RelativeTimeFormat !== "function") return null;
  try {
    const parts = age((now - then) / 1000);
    // "now", "jetzt", "今": the nearest Intl has to the server's "just now".
    if (!parts) return formatter(lang, "auto").format(0, "second");
    const [count, unit] = parts;
    return formatter(lang, "always").format(-count, unit);
  } catch {
    return null;
  }
}

/** Rewrites every `<time data-relative>` in `root` (itself included). */
export function renderRelativeTimes(root, now = Date.now()) {
  if (!root?.querySelectorAll) return;
  const lang = root.ownerDocument?.documentElement?.lang ?? root.documentElement?.lang ?? "";
  const times = [...root.querySelectorAll("time[data-relative]")];
  if (root.matches?.("time[data-relative]")) times.push(root);
  for (const time of times) {
    const text = relativeText(time.getAttribute("datetime"), now, lang);
    if (text && time.textContent !== text) time.textContent = text;
  }
}

if (typeof document !== "undefined") {
  renderRelativeTimes(document);
  // Every swap announces its new content, out-of-band cells and history restores included.
  document.addEventListener("htmx:load", (event) => renderRelativeTimes(event.detail?.elt ?? document));
  // A page left open keeps its ages current.
  setInterval(() => renderRelativeTimes(document), 60_000);
}
