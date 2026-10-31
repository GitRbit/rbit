// Track identities survive lane compaction, so highlighting follows a line even
// when branches merge or move horizontally. Delegation supports boosted pages.
document.addEventListener("click", (event) => {
  if (!(event.target instanceof Element)) return;
  const button = event.target.closest("[data-graph-track], [data-graph-clear]");
  const graph = button?.closest("[data-commit-graph]");
  if (!graph) return;
  const requested = button.dataset.graphTrack;
  const lane = requested === graph.dataset.graphSelected ? undefined : requested;
  if (lane === undefined) delete graph.dataset.graphSelected;
  else graph.dataset.graphSelected = lane;
  graph.querySelectorAll("[data-graph-lane], [data-graph-row]").forEach((element) => {
    element.classList.toggle("graph-highlight", (element.dataset.graphLane ?? element.dataset.graphRow) === lane);
  });
  graph.querySelectorAll("[data-graph-track]").forEach((track) => {
    track.setAttribute("aria-pressed", String(track.dataset.graphTrack === lane));
  });
  graph.querySelector("[data-graph-clear]").hidden = lane === undefined;
  graph.querySelector("[data-graph-status]").textContent = lane === undefined
    ? (globalThis.rbitMessages?.text("browser-graph-help") ?? "Follow colored lines to trace history. Select a track to highlight its commits.")
    : (globalThis.rbitMessages?.text("browser-graph-selected", { track: globalThis.rbitMessages?.number(Number(lane) + 1) ?? String(Number(lane) + 1) }) ?? `Highlighting track ${Number(lane) + 1}. Select it again to show all history.`);
});
