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
// * The header's palette button (`[data-theme-dialog-open]`, a link to the gallery) opens the
//   theme picker dialog instead, fetched from `/themes/picker` the first time. It searches and
//   filters as you type; the highlighted row (arrow keys or the pointer) shows in the dialog's
//   preview pane, and Enter, a click or the pane's Apply button applies it to the page.
// * In the gallery (`[data-theme-gallery]`), resting the pointer on or focusing a card previews
//   that theme on the whole page; leaving the gallery or pressing Escape reverts the preview.
//   The variant filter and search narrow the cards instantly.
// * Previews cross-fade (`data-theme-fade` on <html>, and always in the dialog's pane), so
//   sweeping the pointer or scrolling across themes never flashes the page between them.
//
// Behaviour is attached with event delegation, so it keeps working after htmx swaps.

const COOKIE = "rbit-theme";
const COOKIE_MAX_AGE = 60 * 60 * 24 * 365;
const HOVER_DELAY_MS = 200;
/** Matches the `data-theme-fade` transition in app.css, with a little slack. */
const FADE_MS = 350;
/** Themes whose tokens live in app.css, with their schemes; others link their own sheet. */
const BUILT_IN = { system: null, light: "light", dark: "dark", "dna-dark": "dark", "dna-day": "light" };

const root = document.documentElement;
let applied = { slug: root.dataset.theme ?? "system", scheme: root.dataset.scheme ?? null };
let previewTimer = 0;
/** The gallery card the pointer last headed for, so moving within it doesn't restart the delay. */
let pending = null;
let fadeTimer = 0;

/**
 * Shows `slug` as a cross-fade rather than a cut. The fade stays on until the last change
 * settles: dropping it mid-transition would snap the colours to their end.
 */
function fadeTo(slug, scheme) {
  if ((root.dataset.theme ?? "system") === slug) return;
  root.dataset.themeFade = "";
  show(slug, scheme);
  clearTimeout(fadeTimer);
  fadeTimer = setTimeout(() => delete root.dataset.themeFade, FADE_MS);
}

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
  const current = document.head.querySelectorAll("[data-theme-stylesheet]");
  if (slug in BUILT_IN) {
    for (const link of current) link.remove();
    return;
  }
  const href = `/assets/themes/${slug}.css`;
  if ([...current].some((sheet) => sheet.dataset.theme === slug || sheet.getAttribute("href")?.startsWith(`${href}?`) || sheet.getAttribute("href") === href)) return;
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
  rememberRecent(slug);
}

function revert() {
  clearTimeout(previewTimer);
  pending = null;
  fadeTo(applied.slug, applied.scheme);
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
  if (form.closest("[data-theme-dialog]")) syncSide();
});

// ------------------------------------------------------------------ gallery preview

function cardFrom(target) {
  const card = target instanceof Element ? target.closest("[data-theme-gallery] button[name='theme']") : null;
  return card;
}

function preview(card) {
  fadeTo(card.value, schemeOf(card.value, card));
}

// `pointermove` rather than `pointerover`, so cards scrolling under a resting pointer don't
// preview one after another; the delay skips cards the pointer only passes over.
document.addEventListener("pointermove", (event) => {
  if (event.pointerType === "touch") return;
  const card = cardFrom(event.target);
  if (!card || card === pending) return;
  pending = card;
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

// ------------------------------------------------------------------ picker dialog

const PICKER_URL = "/themes/picker";
const RECENT_KEY = "rbit-recent-themes";
const RECENT_MAX = 5;

/** The dialog once fetched; kept across boosted navigations, which replace the body. */
let dialog = null;
let loading = null;
/** Each row with its search text, built once. */
let rows = [];

function readRecent() {
  try {
    const list = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(list) ? list.filter((slug) => typeof slug === "string") : [];
  } catch {
    return [];
  }
}

function rememberRecent(slug) {
  try {
    const list = [slug, ...readRecent().filter((other) => other !== slug)].slice(0, RECENT_MAX);
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    /* storage unavailable: no recent list */
  }
}

function loadDialog() {
  loading ??= fetch(PICKER_URL, { headers: { Accept: "text/html" } })
    .then((response) => {
      if (!response.ok) throw new Error(`${response.status}`);
      return response.text();
    })
    .then((html) => {
      const template = document.createElement("template");
      template.innerHTML = html.trim();
      dialog = template.content.querySelector("[data-theme-dialog]");
      document.body.append(dialog);
      rows = [...dialog.querySelectorAll("[data-tpk-group] [data-tpk-option]")].map((option) => ({
        option,
        variant: option.dataset.variant,
        text: option.dataset.search,
      }));
      wireDialog(dialog);
      return dialog;
    })
    .catch((error) => {
      loading = null;
      throw error;
    });
  return loading;
}

const part = (name) => dialog.querySelector(`[data-tpk-${name}]`);
const listOf = () => dialog.querySelector(".tpk-list");

/** Rows the keyboard can reach, in order: shown, in a shown group. */
function reachable() {
  return [...listOf().querySelectorAll("[data-tpk-option]")].filter((option) => !option.hidden && !option.closest("[hidden]"));
}

function activeRow() {
  return listOf().querySelector('[data-tpk-option][aria-selected="true"]');
}

/** Highlights `option`, shows it in the preview pane, and keeps it in view when asked. */
function activate(option, { scroll = "nearest" } = {}) {
  const previous = activeRow();
  if (previous !== option) {
    previous?.setAttribute("aria-selected", "false");
    option?.setAttribute("aria-selected", "true");
  }
  const search = part("search");
  if (option) {
    search.setAttribute("aria-activedescendant", option.id);
    if (scroll) option.scrollIntoView({ block: scroll });
  } else {
    search.removeAttribute("aria-activedescendant");
  }
  syncSide();
}

/** Points the preview pane, its description and its Apply button at the highlighted row. */
function syncSide() {
  const option = activeRow();
  if (!option) return;
  part("preview").dataset.theme = option.value;
  part("name").textContent = option.querySelector(".tpk-name").textContent;
  part("meta").textContent = `${option.dataset.family} · ${option.querySelector(".tpk-variant").textContent}`;
  // Row titles read "Name by Author" or "Name · built in".
  const credit = option.title;
  const by = credit.indexOf(" by ");
  part("author").textContent = by >= 0 ? `By ${credit.slice(by + 4)}` : "Built into rbit";
  const button = part("apply");
  const isCurrent = option.value === applied.slug;
  button.value = option.value;
  button.dataset.variant = option.dataset.variant;
  button.dataset.counterpart = option.dataset.counterpart;
  button.disabled = isCurrent;
  button.textContent = isCurrent ? "Applied" : "Apply";
}

function renderRecent() {
  const section = part("recent");
  for (const old of section.querySelectorAll("[data-tpk-option]")) old.remove();
  for (const slug of readRecent()) {
    const source = dialog.querySelector(`[data-tpk-group] [data-tpk-option][value="${CSS.escape(slug)}"]`);
    if (!source) continue;
    const copy = source.cloneNode(true);
    copy.id = `tpk-recent-${slug}`;
    copy.setAttribute("aria-selected", "false");
    copy.hidden = false;
    section.append(copy);
  }
  section.dataset.empty = String(!section.querySelector("[data-tpk-option]"));
}

function filterDialog() {
  const words = part("search").value.toLowerCase().split(/\s+/).filter(Boolean);
  const variant = dialog.querySelector('[data-tpk-variant][aria-pressed="true"]')?.dataset.tpkVariant ?? "";
  let shown = 0;
  for (const { option, variant: rowVariant, text } of rows) {
    const visible = (!variant || !rowVariant || rowVariant === variant) && words.every((word) => text.includes(word));
    option.hidden = !visible;
    if (visible) shown++;
  }
  const list = listOf();
  const recent = part("recent");
  for (const option of recent.querySelectorAll("[data-tpk-option]")) {
    option.hidden = Boolean(variant) && Boolean(option.dataset.variant) && option.dataset.variant !== variant;
  }
  recent.hidden = words.length > 0 || !recent.querySelector("[data-tpk-option]:not([hidden])");
  for (const group of list.querySelectorAll("[data-tpk-group]")) {
    const count = group.querySelectorAll("[data-tpk-option]:not([hidden])").length;
    group.hidden = count === 0;
    group.querySelector(".tpk-group-title span").textContent = count;
    // A letter heading stays only while a row up to the next heading is shown.
    for (const letter of group.querySelectorAll("[data-tpk-letter]")) {
      let next = letter.nextElementSibling;
      let any = false;
      while (next && !next.matches("[data-tpk-letter]")) {
        any ||= !next.hidden;
        next = next.nextElementSibling;
      }
      letter.hidden = !any;
    }
  }
  for (const link of dialog.querySelectorAll("[data-tpk-jump]")) {
    link.hidden = document.getElementById(link.dataset.tpkJump)?.hidden ?? true;
  }
  part("count").textContent = `${shown} ${shown === 1 ? "theme" : "themes"}`;
  part("empty").hidden = shown > 0;
  list.scrollTop = 0;
  activate(reachable()[0] ?? null, { scroll: false });
  syncIndex();
}

/** Marks the family index entry of the group at the top of the list. */
function syncIndex() {
  const list = listOf();
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

function move(by) {
  const options = reachable();
  if (!options.length) return;
  const index = options.indexOf(activeRow());
  const next = index < 0 ? 0 : Math.max(0, Math.min(options.length - 1, index + by));
  activate(options[next]);
}

function wireDialog(dialog) {
  const search = part("search");
  const list = listOf();

  search.addEventListener("input", filterDialog);

  dialog.addEventListener("keydown", (event) => {
    const page = Math.max(1, Math.floor(list.clientHeight / 44));
    const keys = { ArrowDown: 1, ArrowUp: -1, PageDown: page, PageUp: -page };
    if (event.key in keys) {
      event.preventDefault();
      move(keys[event.key]);
    } else if (event.key === "Enter" && event.target === search) {
      event.preventDefault();
      const option = activeRow();
      if (option) list.requestSubmit(option);
    }
  });

  // The pointer highlights too; `pointermove` rather than `pointerover`, so rows scrolling
  // under a resting pointer don't take the highlight from the keyboard.
  list.addEventListener("pointermove", (event) => {
    if (event.pointerType === "touch") return;
    const option = event.target instanceof Element ? event.target.closest("[data-tpk-option]") : null;
    if (option && option !== activeRow()) activate(option, { scroll: false });
  });

  list.addEventListener("scroll", () => requestAnimationFrame(syncIndex), { passive: true });

  dialog.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target : null;
    // A click on the backdrop lands on the dialog element itself.
    if (target === dialog || target?.closest("button[data-tpk-close]")) {
      dialog.close();
      return;
    }
    // Touch has no hover: a tapped row is highlighted as it is applied.
    const option = target?.closest("[data-tpk-option]");
    if (option && option !== activeRow()) activate(option, { scroll: false });
    const variantButton = target?.closest("[data-tpk-variant]");
    if (variantButton) {
      for (const other of dialog.querySelectorAll("[data-tpk-variant]")) {
        other.setAttribute("aria-pressed", String(other === variantButton));
      }
      filterDialog();
      search.focus();
    }
    const jump = target?.closest("[data-tpk-jump]");
    if (jump) {
      event.preventDefault();
      const group = document.getElementById(jump.dataset.tpkJump);
      list.scrollTo({ top: group.offsetTop - list.offsetTop });
      activate(group.querySelector("[data-tpk-option]:not([hidden])"), { scroll: false });
      search.focus();
    }
  });

}

async function openDialog() {
  const element = dialog ?? (await loadDialog());
  if (!element.isConnected) document.body.append(element);
  if (element.open) return;
  renderRecent();
  part("search").value = "";
  element.showModal();
  filterDialog();
  const current = listOf().querySelector('[data-tpk-group] [data-tpk-option][aria-pressed="true"]');
  if (current && !current.hidden) activate(current, { scroll: "center" });
  part("search").focus();
}

document.addEventListener("click", (event) => {
  const opener = event.target instanceof Element ? event.target.closest("[data-theme-dialog-open]") : null;
  if (!opener || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  event.preventDefault();
  // Should the dialog fail to load, the link's own destination still works.
  openDialog().catch(() => {
    location.href = opener.href;
  });
});

// Fetch the dialog as soon as the pointer or focus heads for the button, so it opens at once.
for (const type of ["pointerover", "focusin"]) {
  document.addEventListener(type, (event) => {
    if (!dialog && event.target instanceof Element && event.target.closest("[data-theme-dialog-open]")) loadDialog().catch(() => {});
  });
}

// Leaving the page closes the picker.
document.addEventListener("htmx:beforeRequest", () => {
  if (dialog?.open) dialog.close();
});
