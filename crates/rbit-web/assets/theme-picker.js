// Instant theme switching and the theme gallery. Every control here is a plain form or
// link that works without JavaScript; this module only makes it instant.
//
// * Theme forms (`form[data-theme-form]`, in the header menu and the gallery) apply the
//   chosen theme at once, remember it in the `rbit-theme` cookie and tell the server in
//   the background (which stores it for signed-in users).
// * The header's sun/moon button (`[data-theme-toggle]`) flips between light and dark in one
//   click, staying within a tinted scheme's family (Gruvbox dark ↔ light) when it has a
//   sibling. Its value is the theme it switches to; after every switch it is pointed back
//   at the theme it came from, so toggling twice always returns to where you started.
// * In the gallery (`[data-theme-gallery]`), hovering or focusing a card previews that theme
//   on the whole page; leaving the gallery or pressing Escape reverts the preview. The
//   variant filter and search narrow the cards instantly.
//
// Behaviour is attached with event delegation, so it keeps working after htmx swaps.

const COOKIE = "rbit-theme";
const COOKIE_MAX_AGE = 60 * 60 * 24 * 365;
const HOVER_DELAY_MS = 120;
const BUILT_IN = { system: null, light: "light", dark: "dark" };

const root = document.documentElement;
let applied = { slug: root.dataset.theme ?? "system", scheme: root.dataset.scheme ?? null };
let previewTimer = 0;

/** The light/dark scheme of `slug`: built-ins are known, tinted cards say so in `data-variant`. */
function schemeOf(slug, element) {
  if (slug in BUILT_IN) return BUILT_IN[slug];
  return element?.closest("[data-variant]")?.dataset.variant ?? null;
}

/** Shows `slug` on the page without remembering it. */
function show(slug, scheme) {
  if (slug === "system") {
    delete root.dataset.theme;
    delete root.dataset.scheme;
  } else {
    root.dataset.theme = slug;
    if (scheme) root.dataset.scheme = scheme;
    else delete root.dataset.scheme;
  }
}

/** Mirrors the page background into `theme-color` and the scheme into `color-scheme`. */
function syncMeta(scheme) {
  for (const meta of document.head.querySelectorAll('meta[name="theme-color"]')) meta.remove();
  const meta = document.createElement("meta");
  meta.name = "theme-color";
  meta.content = getComputedStyle(document.body).backgroundColor;
  document.head.append(meta);
  const colorScheme = document.head.querySelector('meta[name="color-scheme"]');
  if (colorScheme) colorScheme.content = scheme ?? "light dark";
}

function markCurrent(slug) {
  for (const button of document.querySelectorAll('form[data-theme-form] button[name="theme"]:not([data-theme-toggle])')) {
    button.setAttribute("aria-pressed", String(button.value === slug));
  }
}

/**
 * Links a tinted theme's own stylesheet in <head>, so it stays after boosted navigations
 * replace the body (and the gallery's combined sheet with it). The gallery carries the
 * asset build version, which busts the cache whenever any asset changes.
 */
function ensureStylesheet(slug, versionSource) {
  const current = document.head.querySelectorAll("link[data-theme-stylesheet]");
  if (slug in BUILT_IN) {
    for (const link of current) link.remove();
    return;
  }
  const href = `/assets/themes/${slug}.css`;
  if ([...current].some((link) => link.getAttribute("href").startsWith(`${href}?`) || link.getAttribute("href") === href)) return;
  const version = versionSource?.closest("[data-asset-version]")?.dataset.assetVersion;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = version ? `${href}?v=${version}` : href;
  link.dataset.themeStylesheet = "";
  // Drop the previous theme's sheet only once the new one has loaded: no flash in between.
  link.addEventListener("load", () => current.forEach((old) => old.remove()), { once: true });
  document.head.append(link);
}

function apply(slug, scheme, source) {
  clearTimeout(previewTimer);
  ensureStylesheet(slug, source);
  applied = { slug, scheme };
  show(slug, scheme);
  markCurrent(slug);
  syncMeta(scheme);
  const secure = location.protocol === "https:" ? "; Secure" : "";
  document.cookie = `${COOKIE}=${slug}; Path=/; Max-Age=${COOKIE_MAX_AGE}; SameSite=Lax${secure}`;
  // The server repeats the cookie and, for signed-in users, stores the choice.
  fetch("/themes", { method: "POST", body: new URLSearchParams({ theme: slug, js: "1" }), keepalive: true }).catch(() => {});
}

function revert() {
  clearTimeout(previewTimer);
  show(applied.slug, applied.scheme);
}

const opposite = (scheme) => (scheme === "dark" ? "light" : "dark");
const deviceScheme = () => (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");

/** The last toggle, so pages rendered after it (boosted navigations) still toggle back. */
let lastToggle = null;

document.addEventListener("htmx:afterSettle", () => {
  if (lastToggle && applied.slug === lastToggle.at) aimToggle(lastToggle.back.slug, lastToggle.back.scheme);
});

/** Points the toggle at `slug` (of `scheme`), updating every copy after htmx swaps. */
function aimToggle(slug, scheme) {
  for (const toggle of document.querySelectorAll("[data-theme-toggle]")) {
    toggle.value = slug;
    toggle.dataset.variant = scheme;
  }
}

/** Where the toggle goes after choosing `button`'s theme in a menu or the gallery. */
function aimAfterChoosing(button, slug, scheme) {
  if (slug === "system") aimToggle(opposite(deviceScheme()), opposite(deviceScheme()));
  else if (button.dataset.counterpart) aimToggle(button.dataset.counterpart, opposite(scheme));
  else if (slug in BUILT_IN) aimToggle(opposite(scheme), opposite(scheme));
}

document.addEventListener("submit", (event) => {
  const form = event.target;
  if (!(form instanceof HTMLFormElement) || !form.matches("[data-theme-form]")) return;
  const button = event.submitter;
  if (!(button instanceof HTMLButtonElement) || button.name !== "theme") return;
  event.preventDefault();
  const from = applied;
  if (button.matches("[data-theme-toggle]")) {
    // System has no scheme of its own: switch to the side the device isn't showing.
    const slug = from.slug === "system" ? opposite(deviceScheme()) : button.value;
    const scheme = slug in BUILT_IN ? BUILT_IN[slug] : button.dataset.variant || null;
    apply(slug, scheme, button);
    // Back to where we came from, except System, which pins the other built-in instead.
    const back = from.slug === "system" ? { slug: opposite(scheme), scheme: opposite(scheme) } : from;
    aimToggle(back.slug, back.scheme);
    lastToggle = { at: slug, back };
  } else {
    lastToggle = null;
    const scheme = schemeOf(button.value, button);
    apply(button.value, scheme, button);
    aimAfterChoosing(button, button.value, scheme);
  }
  for (const menu of document.querySelectorAll("details[data-menu][open]")) menu.removeAttribute("open");
});

// ------------------------------------------------------------------ gallery preview

function cardFrom(target) {
  const card = target instanceof Element ? target.closest("[data-theme-gallery] button[name='theme']") : null;
  return card;
}

function preview(card) {
  show(card.value, schemeOf(card.value, card));
}

document.addEventListener("pointerover", (event) => {
  if (event.pointerType === "touch") return;
  const card = cardFrom(event.target);
  if (!card) return;
  clearTimeout(previewTimer);
  previewTimer = setTimeout(() => preview(card), HOVER_DELAY_MS);
});

document.addEventListener("pointerout", (event) => {
  const gallery = event.target instanceof Element ? event.target.closest("[data-theme-gallery]") : null;
  if (gallery && !gallery.contains(event.relatedTarget)) revert();
});

document.addEventListener("focusin", (event) => {
  const card = cardFrom(event.target);
  if (card && card.matches(":focus-visible")) preview(card);
});

document.addEventListener("focusout", (event) => {
  const gallery = event.target instanceof Element ? event.target.closest("[data-theme-gallery]") : null;
  if (gallery && !gallery.contains(event.relatedTarget)) revert();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && (root.dataset.theme ?? "system") !== applied.slug) revert();
});

// Leaving the page mid-preview must not carry the preview along.
document.addEventListener("htmx:beforeRequest", revert);

// ------------------------------------------------------------------ gallery filter

/** Search text for each card, built once per gallery. */
const indexes = new WeakMap();

function indexOf(gallery) {
  let index = indexes.get(gallery);
  if (!index) {
    index = [...gallery.querySelectorAll("[data-theme-item]")].map((item) => ({
      item,
      variant: item.dataset.variant ?? "",
      text: `${item.textContent} ${item.closest("[data-theme-family]")?.dataset.themeFamily ?? ""}`.toLowerCase(),
    }));
    indexes.set(gallery, index);
  }
  return index;
}

function filter(gallery) {
  const variant = gallery.dataset.filter ?? "";
  const words = (gallery.querySelector("[data-theme-search]")?.value ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  let shown = 0;
  let anyVisible = false;
  for (const { item, variant: itemVariant, text } of indexOf(gallery)) {
    const visible = (!variant || !itemVariant || itemVariant === variant) && words.every((word) => text.includes(word));
    item.hidden = !visible;
    anyVisible ||= visible;
    if (visible && itemVariant) shown++;
  }
  for (const family of gallery.querySelectorAll("[data-theme-family]")) {
    family.hidden = !family.querySelector("[data-theme-item]:not([hidden])");
  }
  const count = gallery.querySelector("[data-theme-count]");
  if (count) count.textContent = `${shown} ${shown === 1 ? "theme" : "themes"}`;
  const empty = gallery.querySelector("[data-theme-empty]");
  if (empty) empty.hidden = anyVisible;

  const url = new URL(location.href);
  url.searchParams.delete("variant");
  url.searchParams.delete("q");
  if (variant) url.searchParams.set("variant", variant);
  if (words.length) url.searchParams.set("q", words.join(" "));
  history.replaceState(history.state, "", url);
}

document.addEventListener("click", (event) => {
  const link = event.target instanceof Element ? event.target.closest("[data-theme-gallery] a[data-variant-filter]") : null;
  if (!link) return;
  event.preventDefault();
  const gallery = link.closest("[data-theme-gallery]");
  gallery.dataset.filter = link.dataset.variantFilter;
  for (const other of gallery.querySelectorAll("a[data-variant-filter]")) {
    if (other === link) other.setAttribute("aria-current", "true");
    else other.removeAttribute("aria-current");
  }
  filter(gallery);
});

document.addEventListener("input", (event) => {
  const input = event.target instanceof Element ? event.target.closest("[data-theme-gallery] [data-theme-search]") : null;
  if (input) filter(input.closest("[data-theme-gallery]"));
});

document.addEventListener("submit", (event) => {
  const form = event.target instanceof Element ? event.target.closest("[data-theme-gallery] form[role='search']") : null;
  if (!form) return;
  event.preventDefault();
  filter(form.closest("[data-theme-gallery]"));
});
