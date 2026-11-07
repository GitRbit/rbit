/** Marks the family index entry of the group at the top of the list. */
export function syncIndex(dialog) {
  const list = dialog.querySelector(".tpk-list");
  const top = list.scrollTop + 1;
  let current = null;
  for (const group of list.querySelectorAll(".tpk-group:not([hidden])")) {
    if (group.offsetTop - list.offsetTop > top) break;
    current = group;
  }
  current ??= list.querySelector(".tpk-group:not([hidden])");
  for (const link of dialog.querySelectorAll("[data-tpk-jump]")) {
    const isCurrent = link.dataset.tpkJump === current?.id;
    if (isCurrent && !link.hasAttribute("aria-current")) {
      link.setAttribute("aria-current", "true");
      link.scrollIntoView({ block: "nearest" });
    } else if (!isCurrent) {
      link.removeAttribute("aria-current");
    }
  }
}


/** At most one geometry pass per frame while the picker is open and connected. */
export function watchIndexScroll(dialog) {
  let frame = null;
  const list = dialog.querySelector(".tpk-list");
  list.addEventListener("scroll", () => {
    if (frame !== null || !dialog.open || !dialog.isConnected) return;
    frame = requestAnimationFrame(() => {
      frame = null;
      if (dialog.open && dialog.isConnected) syncIndex(dialog);
    });
  }, { passive: true });
  dialog.addEventListener("close", () => {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
  });
}
