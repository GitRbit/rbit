// The header's language dialog. The language button (`[data-language-dialog-open]`) is a link to
// /settings/language, which works without JavaScript; this module opens the dialog instead,
// fetched from /settings/language/dialog the first time. Each row submits the language page's
// own form: the choice is saved exactly as that page saves it (the account preference, or a
// visitor's cookie), then the page reloads once in its new language. The reloaded document
// carries a new catalog key, which locale.js records without reloading again.
//
// The dialog is modal: Escape and a click on the backdrop close it, the arrow keys move
// between languages, and focus returns to the button that opened it.

export const DIALOG_URL = "/settings/language/dialog";

/**
 * Posts `value` to the language form. Resolves `true` once the server saved it (it answers
 * with the redirect the native form follows), `false` when it refused the request.
 */
export async function saveLanguage(form, value, fetchImpl = fetch) {
  const response = await fetchImpl(form.getAttribute("action"), {
    method: "POST",
    body: new URLSearchParams({ locale: value }),
    credentials: "same-origin",
    redirect: "manual",
  });
  return response.type === "opaqueredirect" || response.ok;
}

/** The options the arrow keys move between, in reading order. */
function options(dialog) {
  return [...dialog.querySelectorAll("[data-lpk-option]")].filter((option) => !option.disabled);
}

export function installLanguagePicker(document, window) {
  let dialog = null;
  let loading = null;
  let opener = null;
  let saving = false;

  const load = () => {
    loading ??= window.fetch(DIALOG_URL, { headers: { Accept: "text/html" }, credentials: "same-origin" })
      .then((response) => {
        if (!response.ok) throw new Error(`${response.status}`);
        return response.text();
      })
      .then((html) => {
        const template = document.createElement("template");
        template.innerHTML = html.trim();
        const element = template.content.querySelector("[data-language-dialog]");
        // A redirect (to sign in again, say) answers with some other page: use the link.
        if (!element) throw new Error("no dialog");
        dialog = element;
        wire(dialog);
        return dialog;
      })
      .catch((error) => {
        loading = null;
        throw error;
      });
    return loading;
  };

  const choose = async (form, button) => {
    if (saving) return;
    if (button.getAttribute("aria-current") === "true") {
      dialog.close();
      return;
    }
    saving = true;
    form.setAttribute("aria-busy", "true");
    for (const option of options(dialog)) option.disabled = true;
    let saved = false;
    try {
      saved = await saveLanguage(form, button.value, window.fetch.bind(window));
    } catch {
      saved = false;
    }
    if (saved) {
      // One reload renders the page in the chosen language; nothing reloads after it.
      window.location.reload();
      return;
    }
    // Let the language page show what went wrong, as it does without scripts.
    window.location.assign(form.getAttribute("action"));
  };

  const wire = (element) => {
    const form = element.querySelector("[data-language-form]");
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const button = event.submitter;
      if (button && button.name === "locale") choose(form, button);
    });
    element.addEventListener("click", (event) => {
      const target = event.target;
      // A click on the backdrop lands on the dialog element itself.
      if (target === element || target?.closest?.("[data-lpk-close]")) element.close();
    });
    element.addEventListener("keydown", (event) => {
      const list = options(element);
      const index = list.indexOf(document.activeElement);
      const next = { ArrowDown: index + 1, ArrowUp: index - 1, Home: 0, End: list.length - 1 }[event.key];
      if (next === undefined || !list.length) return;
      event.preventDefault();
      list[(next + list.length) % list.length].focus();
    });
    element.addEventListener("close", () => {
      const target = opener;
      opener = null;
      if (target?.isConnected) target.focus();
    });
  };

  const open = async (link) => {
    const element = dialog ?? (await load());
    if (!element.isConnected) document.body.append(element);
    if (element.open) return;
    // On phones the button is a row of a menu; the menu closes and focus returns to its toggle.
    const menu = link.closest("details[data-menu]");
    menu?.removeAttribute("open");
    opener = menu?.querySelector("summary") ?? link;
    element.showModal();
    const current = element.querySelector('[data-lpk-option][aria-current="true"]') ?? options(element)[0];
    current?.focus();
  };

  document.addEventListener("click", (event) => {
    const link = event.target?.closest?.("[data-language-dialog-open]");
    if (!link || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    // Should the dialog fail to load, the link's own destination still works.
    open(link).catch(() => window.location.assign(link.getAttribute("href")));
  });

  // Fetch the dialog as soon as the pointer or focus heads for the button, so it opens at once.
  for (const type of ["pointerover", "focusin"]) {
    document.addEventListener(type, (event) => {
      if (!dialog && event.target?.closest?.("[data-language-dialog-open]")) load().catch(() => {});
    });
  }

  // Leaving the page closes the dialog.
  document.addEventListener("htmx:beforeRequest", () => {
    if (dialog?.open) dialog.close();
  });
}

if (typeof document !== "undefined" && typeof window !== "undefined") installLanguagePicker(document, window);
