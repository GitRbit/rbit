// Progressive enhancements. Every page works without this file.
// Behaviour is attached with event delegation, so it keeps working after htmx swaps.

// Theme menu and gallery: instant switching, live previews, filtering.
import "./theme-picker.js";

// Preloading, partial swaps, progress and focus for boosted navigation.
import "./nav.js";

// Preloading boosted links on hover, press, touch and (for a few) visibility.
import "./preload.js";

// Instant list filters (dashboard repository list).
import "./filter.js";

// Diff pages: sticky file headers, the changed-file tree and its filter.
import "./diff.js";
import "./compare.js";
import "./repo.js";
import "./repo-documents.js";
import "./sticky-header.js";
import "./contributions.js";
import "./branch-graph.js";
import "./mirror-progress.js";
import "./repo-create.js";
import "./markdown.js";
import "./discussion.js";
import "./markdown-preview.js";

// Long code and diff views: exact placeholder heights for virtual blocks.
import "./blocks.js";

// Passkeys for two-factor authentication (settings and the second sign-in step).
import "./passkey.js";

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
document.addEventListener("htmx:afterSettle", (event) => {
  if (!event.detail.requestConfig?.boosted) return;
  for (const menu of document.querySelectorAll("details[data-menu][open]")) menu.removeAttribute("open");
});

// The branch picker can have thousands of refs. Load its first page only on open;
// failed requests retain the fallback link and can be retried by reopening the menu.
document.addEventListener("toggle", (event) => {
  const picker = event.target;
  if (picker instanceof HTMLDetailsElement && picker.open && picker.matches("[data-ref-picker]") && picker.querySelector("[data-ref-picker-unloaded]")) {
    window.htmx?.trigger(picker, "rbit:open");
  }
}, true);

// Installation metadata and offline support can wait until the page finishes loading.
addEventListener("load", () => {
  const manifest = document.createElement("link");
  manifest.rel = "manifest";
  const updateManifest = () => {
    const selected = document.documentElement?.dataset.theme ?? "system";
    const theme = selected === "system" ? (matchMedia("(prefers-color-scheme: dark)").matches ? "dna-dark" : "dna-day") : selected;
    manifest.href = `/manifest.webmanifest?theme=${encodeURIComponent(theme)}`;
  };
  updateManifest();
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", updateManifest);
  document.head.append(manifest);
  if ("serviceWorker" in navigator) navigator.serviceWorker.register("/sw.js").catch(() => {});
}, { once: true });
