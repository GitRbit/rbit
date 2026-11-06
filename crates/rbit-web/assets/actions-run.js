// Keep in-progress review input across polls without withholding server updates.
// Restore only into controls the NEW authorized document still renders. Never copy
// hidden fields, form actions, permissions or old HTML into the refreshed run.
const pending = new WeakMap();
const editable = 'input[name="trust_author"][type="checkbox"], textarea[name="comment"]';
const forms = run => [...run.querySelectorAll("form[data-run-review]")];
const fields = form => [...form.querySelectorAll(editable)];
const fieldKey = field => `${field.tagName}:${field.type}:${field.name}`;

function formKey(form) {
  return [form.method, form.action, form.dataset.runReview,
    form.querySelector('input[name="environment"]')?.value ?? ""].join("\n");
}
function runIdentity(value) {
  if (typeof value !== "string" || !value) return null;
  try {
    const url = new URL(value, location.href);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) return null;
    // The run route has no query selector: identity is its full deployed path.
    // URL parsing handles relative URLs, subpaths, query encoding and fragments.
    return url.origin + url.pathname;
  } catch { return null; }
}
function polling(event) {
  const detail = event.detail;
  const source = detail?.requestConfig?.elt;
  if (!(detail?.xhr?.status === 200 && detail.requestConfig.verb?.toLowerCase() === "get"
    && source instanceof Element && source.id === "run"
    && detail.target instanceof Element && detail.target.id === "run")) return false;
  if (detail.boosted === true || detail.requestConfig.boosted === true) return false;
  const current = runIdentity(location.href);
  if (!current || runIdentity(detail.requestConfig.path) !== current
    || runIdentity(detail.xhr.responseURL) !== current) return false;
  try {
    // Redirect/retarget/navigation instructions are not ordinary polling responses.
    return ["HX-Redirect", "HX-Location", "HX-Retarget", "HX-Reswap", "HX-Refresh", "HX-Push-Url", "HX-Replace-Url"]
      .every(name => detail.xhr.getResponseHeader(name) === null);
  } catch { return false; }
}

document.addEventListener("htmx:beforeSwap", event => {
  if (!polling(event)) return;
  const values = forms(event.detail.target).map(form => ({
    key: formKey(form),
    fields: fields(form).map(field => ({
      key: fieldKey(field), value: field.value, checked: field.checked,
      dirty: field.type === "checkbox" ? field.checked !== field.defaultChecked : field.value !== field.defaultValue,
      focused: field === document.activeElement,
      selection: field.tagName === "TEXTAREA" ? [field.selectionStart, field.selectionEnd, field.selectionDirection] : null,
    })),
  }));
  pending.set(event.detail.xhr, { run: runIdentity(location.href), values });
});

document.addEventListener("htmx:afterSwap", event => {
  const xhr = event.detail?.xhr;
  const saved = xhr && pending.get(xhr);
  if (xhr) pending.delete(xhr);
  if (!saved || saved.run !== runIdentity(location.href) || !polling(event)) return;
  for (const form of forms(event.detail.target)) {
    const previous = saved.values.find(item => item.key === formKey(form));
    if (!previous) continue;
    for (const field of fields(form)) {
      const value = previous.fields.find(item => item.key === fieldKey(field));
      if (!value || field.disabled || field.matches(":disabled") || field.readOnly) continue;
      if (value.dirty) {
        if (field.type === "checkbox") field.checked = value.checked;
        else field.value = value.value;
      }
      if (value.focused && document.activeElement === document.body) {
        field.focus({ preventScroll: true });
        if (value.selection) field.setSelectionRange(...value.selection);
      }
    }
  }
});
