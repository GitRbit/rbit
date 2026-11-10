// The job page's log toolbar (templates/actions/job/toolbar.html), which only scripts show:
// search across every step's log with next and previous (Enter, Shift+Enter), each line's
// time (remembered in this browser), and opening or closing every step and group at once.
// Clicking a line number links to the line and also copies that link. Without scripts the
// raw log and its download remain, and the browser's find searches the open steps.

const JOB = "#job";
/** The most matches marked at once; a search for "e" in a long log stays responsive. */
const MAX_MATCHES = 5000;
const TIMES = "rbit.actions.log-times";

let query = "";
let matches = [];
let current = -1;
let pending = null;
/** The search box had the focus before a reload of the job. */
let typing = false;

const job = () => document.querySelector(JOB);
const tools = () => job()?.querySelector("[data-log-tools]") ?? null;

function remembered() {
  try {
    return localStorage.getItem(TIMES) === "1";
  } catch {
    return false;
  }
}

function showTimes(on) {
  const root = job();
  if (!root) return;
  root.toggleAttribute("data-times", on);
  const box = root.querySelector("[data-log-times]");
  if (box) box.checked = on;
}

function clear(root) {
  for (const mark of root.querySelectorAll("mark.log-match")) mark.replaceWith(mark.textContent);
  for (const text of root.querySelectorAll(".log-text")) text.normalize();
  matches = [];
  current = -1;
}

/** Marks every match of `needle` in the log text under `root`, in document order. */
function mark(root, needle) {
  const found = [];
  const lower = needle.toLowerCase();
  for (const text of root.querySelectorAll(".log-text")) {
    if (found.length >= MAX_MATCHES) break;
    if (!text.textContent.toLowerCase().includes(lower)) continue;
    const walker = document.createTreeWalker(text, NodeFilter.SHOW_TEXT);
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    for (const node of nodes) {
      const value = node.nodeValue;
      const folded = value.toLowerCase();
      let index = folded.indexOf(lower);
      if (index < 0 || folded.length !== value.length) continue;
      const pieces = document.createDocumentFragment();
      let last = 0;
      while (index >= 0 && found.length < MAX_MATCHES) {
        pieces.append(value.slice(last, index));
        const hit = document.createElement("mark");
        hit.className = "log-match";
        hit.textContent = value.slice(index, index + needle.length);
        pieces.append(hit);
        found.push(hit);
        last = index + needle.length;
        index = folded.indexOf(lower, last);
      }
      pieces.append(value.slice(last));
      node.replaceWith(pieces);
    }
  }
  return found;
}

function count() {
  const box = tools();
  const output = box?.querySelector("[data-log-count]");
  if (!output) return;
  if (!query) output.textContent = "";
  else if (!matches.length) output.textContent = box.dataset.noMatches ?? "0";
  else output.textContent = `${current < 0 ? 0 : current + 1}/${matches.length}${matches.length >= MAX_MATCHES ? "+" : ""}`;
}

function search(text) {
  const root = job();
  if (!root) return;
  clear(root);
  query = text;
  if (query) matches = mark(root, query);
  count();
}

function step(by) {
  if (!matches.length) return;
  matches[current]?.removeAttribute("data-current");
  current = (current + by + matches.length) % matches.length;
  const hit = matches[current];
  hit.setAttribute("data-current", "");
  for (let details = hit.closest("details"); details; details = details.parentElement?.closest("details")) details.open = true;
  hit.scrollIntoView({ block: "center" });
  count();
}

document.addEventListener("input", (event) => {
  const input = event.target;
  if (!(input instanceof HTMLInputElement) || !input.matches(`${JOB} [data-log-search]`)) return;
  clearTimeout(pending);
  pending = setTimeout(() => search(input.value), 150);
});

document.addEventListener("keydown", (event) => {
  const input = event.target;
  if (event.key !== "Enter" || !(input instanceof HTMLInputElement) || !input.matches(`${JOB} [data-log-search]`)) return;
  event.preventDefault();
  if (input.value !== query) search(input.value);
  step(event.shiftKey ? -1 : 1);
});

document.addEventListener("change", (event) => {
  const box = event.target;
  if (!(box instanceof HTMLInputElement) || !box.matches(`${JOB} [data-log-times]`)) return;
  showTimes(box.checked);
  try {
    localStorage.setItem(TIMES, box.checked ? "1" : "0");
  } catch {
    // Times stay on for this page only.
  }
});

document.addEventListener("click", async (event) => {
  if (!(event.target instanceof Element)) return;
  const button = event.target.closest(`${JOB} [data-log-match], ${JOB} [data-log-expand]`);
  if (button?.dataset.logMatch) {
    const input = tools()?.querySelector("[data-log-search]");
    if (input && input.value !== query) search(input.value);
    step(button.dataset.logMatch === "previous" ? -1 : 1);
    return;
  }
  if (button?.dataset.logExpand) {
    const open = button.dataset.logExpand === "open";
    for (const details of job()?.querySelectorAll("details[data-step], details.log-group") ?? []) details.open = open;
    return;
  }
  const number = event.target.closest(`${JOB} a.log-num`);
  if (!number || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey) return;
  const link = new URL(number.getAttribute("href"), location.href).href;
  const status = tools()?.querySelector("[data-log-status]");
  try {
    await navigator.clipboard.writeText(link);
    if (status) {
      status.textContent = "";
      status.textContent = status.dataset.copied ?? "";
    }
  } catch {
    // The address bar has the link: clicking a number still goes to the line.
  }
});

/** Shows the toolbar and keeps the viewer's search and times across reloads. */
function enhance() {
  const box = tools();
  if (!box) return;
  box.hidden = false;
  showTimes(remembered());
  const input = box.querySelector("[data-log-search]");
  if (input && query) {
    input.value = query;
    search(query);
  }
  if (input && typing) input.focus();
  typing = false;
}

// New live lines are searched too.
document.addEventListener("actions-log:append", () => {
  if (query) search(query);
});
document.addEventListener("htmx:beforeSwap", () => {
  typing = document.activeElement?.matches?.("[data-log-search]") ?? false;
});
document.addEventListener("htmx:afterSwap", enhance);
if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", enhance);
else enhance();
