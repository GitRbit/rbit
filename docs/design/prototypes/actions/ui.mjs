// Review-only HTML helpers. Every fixture/query string crosses the escaping boundary.
export const h = (value) => String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
export const defaults = Object.freeze({
  state: "failed", theme: "dna-day", dir: "ltr", viewer: "writer",
  workflow: "0", ref: "main", long: "0", branch: "", actor: "",
  event: "", status: "", q: "", number: "842", job: "2", page: "1", earlier: "",
});
const compact = (ctx) => Object.fromEntries(Object.entries(ctx).filter(([key, value]) => key !== "screen" && value !== "" && value !== defaults[key]));
export function link(ctx, screen, patch = {}) {
  const query = new URLSearchParams(compact({ ...ctx, ...patch }));
  query.delete("screen"); query.delete("intent"); query.delete("return");
  return `/${screen}?${query}`;
}
export function hidden(ctx, exclude = []) {
  return Object.entries(compact(ctx)).filter(([key]) => !["screen", "intent", "return", ...exclude].includes(key))
    .map(([key, value]) => `<input type="hidden" name="${h(key)}" value="${h(value)}">`).join("");
}
export function intent(ctx, name) { return `/intent?${new URLSearchParams({ ...compact(ctx), intent: name, return: ctx.screen })}`; }
const shapes = {
  check: '<path d="m9 12 2 2 4-4"/><circle cx="12" cy="12" r="9"/>',
  failure: '<circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/>',
  running: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3"/>',
  queued: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  skipped: '<circle cx="12" cy="12" r="9"/><path d="m6 18 12-12"/>',
  approval: '<path d="m12 3 8 4v6c0 5-8 8-8 8s-8-3-8-8V7Z"/><path d="M12 8v5m0 3h.01"/>',
  branch: '<circle cx="6" cy="5" r="3"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="19" r="3"/><path d="M6 8v8m0-3c8 0 12-1 12-4"/>',
  code: '<path d="m8 7-5 5 5 5m8-10 5 5-5 5m-3-14-2 18"/>',
  refresh: '<path d="M20 7v5h-5M4 17v-5h5"/><path d="M6 6a8 8 0 0 1 14 6M4 12a8 8 0 0 0 14 6"/>',
  download: '<path d="M12 3v12m-5-5 5 5 5-5M4 17v4h16v-4"/>',
  file: '<path d="M14 2H6v20h12V6Zm0 0v5h5M8 12h8m-8 4h6"/>',
  search: '<circle cx="10" cy="10" r="7"/><path d="m15 15 6 6"/>',
  play: '<path d="m8 4 12 8-12 8Z"/>',
  chevron: '<path d="m9 5 7 7-7 7"/>',
};
export function icon(name) {
  return `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    ${shapes[name] ?? shapes.file}
  </svg>`;
}
export function status(state, text = true) {
  const labels = { success: "Passed", failure: "Failed", running: "In progress", queued: "Queued", waiting: "Waiting", skipped: "Skipped", approval: "Awaiting approval", cancelled: "Cancelled" };
  return `<span class="status ${h(state)}">${icon(state === "success" ? "check" : state === "cancelled" ? "skipped" : state)}${text ? h(labels[state] ?? state) : `<span class="sr-only">${h(labels[state] ?? state)}</span>`}</span>`;
}
export function action(ctx, intent, label, primary = false) {
  return `<form action="/intent" method="get" data-production-method="post">
    ${hidden(ctx)}
    <input type="hidden" name="return" value="${h(ctx.screen)}">
    <button class="btn ${primary ? "btn-primary" : ""}" name="intent" value="${h(intent)}">
      ${h(label)}
    </button>
  </form>`;
}
export const writer = (ctx) => ctx.viewer === "writer";
export function empty(title, text, button = "", pageHeading = false) {
  const tag = pageHeading ? "h1" : "h2";
  return `<div class="empty">
    ${icon("file")}<${tag}>${h(title)}</${tag}>
    <p>${h(text)}</p>${button}
  </div>`;
}
