// Pure parsing: raw server-rendered footer and response-header totals are different spans.
export function renderHeaderMs(value) {
  if (typeof value !== "string") return null;
  const matches = [...value.matchAll(/(?:^|,)\s*render;dur=(\d+(?:\.\d+)?)(?=\s*(?:;|,|$))/g)];
  if (matches.length !== 1 || [...value.matchAll(/(?:^|,)\s*render(?=;|,|$)/g)].length !== 1) return null;
  const duration = Number(matches[0][1]);
  return Number.isFinite(duration) ? duration : null;
}

export function footerSsrMs(tail) {
  const matches = [...tail.matchAll(/<span\b[^>]*\bdata-render-time(?:=(?:"[^"]*"|'[^']*'))?[^>]*>\s*(\d+(?:\.\d+)?)\s*(µs|μs|&micro;s|&#181;s|ms|s)\s*<\/span>/g)];
  if (matches.length !== 1) return null;
  const value = Number(matches[0][1]);
  const unit = matches[0][2];
  const result = unit === "s" ? value * 1000 : unit === "ms" ? value : value / 1000;
  return Number.isFinite(result) ? result : null;
}
