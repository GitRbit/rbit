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
import "./repo.js";

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

// `--subnav-height` follows the page's tabs under the top bar (base.html `.site-subnav`), so
// app.css can place sticky elements below them. A boosted navigation may replace the tabs'
// container, so the observer moves to the new one after every swap.
const subnavObserver = new ResizeObserver(measureSubnav);
let observedSubnav = null;

function measureSubnav() {
  const subnav = document.querySelector("body > .site-subnav");
  if (subnav?.isConnected) document.documentElement.style.setProperty("--subnav-height", `${subnav.offsetHeight}px`);
}

function trackSubnav() {
  const subnav = document.querySelector("body > .site-subnav");
  if (subnav !== observedSubnav) {
    if (observedSubnav) subnavObserver.unobserve(observedSubnav);
    if (subnav) subnavObserver.observe(subnav);
    observedSubnav = subnav;
  }
  measureSubnav();
}

trackSubnav();
document.addEventListener("htmx:afterSettle", trackSubnav);

// The branch picker can have thousands of refs. Load its first page only on open;
// failed requests retain the fallback link and can be retried by reopening the menu.
document.addEventListener("toggle", (event) => {
  const picker = event.target;
  if (picker instanceof HTMLDetailsElement && picker.open && picker.matches("[data-ref-picker]") && picker.querySelector("[data-ref-picker-unloaded]")) {
    window.htmx?.trigger(picker, "rbit:open");
  }
}, true);

if ("serviceWorker" in navigator) {
  addEventListener("load", () => navigator.serviceWorker.register("/sw.js").catch(() => {}));
}
