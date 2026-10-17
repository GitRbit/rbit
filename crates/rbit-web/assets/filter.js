// Instant client-side list filter. Progressive enhancement: without it the full list shows.
//
//   <input type="search" data-filter="LIST_ID" data-filter-count="COUNT_ID">
//   <ul id="LIST_ID">
//     <li data-filter-text="owner/name description">…</li>
//     <li data-filter-empty hidden>No matches.</li>
//   </ul>
//
// Every whitespace-separated word must occur in an item's `data-filter-text` (lower-case).
// The optional count element shows "shown / total" while filtering. Listeners are
// delegated on `document`, so the module works across htmx swaps and loads only once.

function applyFilter(input) {
  const list = document.getElementById(input.dataset.filter);
  if (!list) return;
  const words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
  const items = list.querySelectorAll("[data-filter-text]");
  let shown = 0;
  for (const item of items) {
    const text = item.dataset.filterText;
    const matches = words.every((word) => text.includes(word));
    item.hidden = !matches;
    if (matches) shown += 1;
  }
  list.querySelector("[data-filter-empty]")?.toggleAttribute("hidden", shown > 0);

  const count = input.dataset.filterCount && document.getElementById(input.dataset.filterCount);
  if (count) count.textContent = words.length ? `${shown} / ${items.length}` : `${items.length}`;
}

document.addEventListener("input", (event) => {
  const input = event.target instanceof HTMLInputElement ? event.target : null;
  if (input?.dataset.filter) applyFilter(input);
});

document.addEventListener("keydown", (event) => {
  const input = event.target instanceof HTMLInputElement ? event.target : null;
  if (event.key !== "Escape" || !input?.dataset.filter || !input.value) return;
  input.value = "";
  applyFilter(input);
});

// A restored page (back/forward) may keep the typed text: re-apply it.
document.addEventListener("htmx:historyRestore", () => {
  for (const input of document.querySelectorAll("input[data-filter]")) applyFilter(input);
});
