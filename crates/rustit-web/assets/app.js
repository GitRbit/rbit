// Progressive enhancements. Every page works without this file.
// Behaviour is attached with event delegation, so it keeps working after htmx swaps.

const THEME_KEY = "rustit-theme";

function currentTheme() {
  const pinned = document.documentElement.dataset.theme;
  if (pinned) return pinned;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function toggleTheme() {
  const next = currentTheme() === "dark" ? "light" : "dark";
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem(THEME_KEY, next);
  } catch {
    /* ignore: theme simply won't persist */
  }
}

async function copyToClipboard(button) {
  const source = document.getElementById(button.dataset.copy);
  const text = source?.value ?? source?.textContent ?? "";
  try {
    await navigator.clipboard.writeText(text.trim());
    const label = button.getAttribute("aria-label");
    button.dataset.copied = "true";
    button.setAttribute("aria-label", "Copied!");
    setTimeout(() => {
      delete button.dataset.copied;
      button.setAttribute("aria-label", label);
    }, 1500);
  } catch {
    source?.select?.();
  }
}

document.addEventListener("click", (event) => {
  const target = event.target instanceof Element ? event.target : null;
  if (!target) return;

  if (target.closest("[data-theme-toggle]")) {
    toggleTheme();
    return;
  }

  const copyButton = target.closest("[data-copy]");
  if (copyButton) {
    copyToClipboard(copyButton);
    return;
  }

  // Close any open <details> menu when clicking outside of it.
  for (const menu of document.querySelectorAll("details[data-menu][open]")) {
    if (!menu.contains(target)) menu.removeAttribute("open");
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  for (const menu of document.querySelectorAll("details[data-menu][open]")) menu.removeAttribute("open");
});

// Close menus after htmx navigations so a boosted link inside a menu doesn't leave it open.
document.addEventListener("htmx:afterSettle", () => {
  for (const menu of document.querySelectorAll("details[data-menu][open]")) menu.removeAttribute("open");
});

if ("serviceWorker" in navigator) {
  addEventListener("load", () => navigator.serviceWorker.register("/sw.js").catch(() => {}));
}
