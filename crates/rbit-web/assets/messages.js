// The server owns locale negotiation and formatting. This bounded text catalog is
// escaped into a data attribute, never executable script or trusted markup.
const MAX_CATALOG_BYTES = 64 * 1024;
const MAX_TEXT = 8192;
const MAX_ARGUMENT = 2048;
let source;
let catalog = Object.create(null);

function messages() {
  const value = globalThis.document?.body?.dataset?.browserMessages ?? '';
  if (value === source) return catalog;
  source = value;
  catalog = Object.create(null);
  if (value.length > MAX_CATALOG_BYTES) return catalog;
  try {
    const parsed = JSON.parse(value);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return catalog;
    for (const [key, text] of Object.entries(parsed).slice(0, 128)) {
      if (/^browser-[a-z0-9-]{1,100}$/.test(key) && typeof text === 'string' && text.length <= MAX_TEXT) catalog[key] = text;
    }
  } catch { /* A missing catalog leaves the server-rendered page usable. */ }
  return catalog;
}

export function text(key, args = {}) {
  const template = messages()[key];
  if (template === undefined) return undefined;
  return template.replace(/\uE000([a-z][a-z0-9-]{0,40})\uE001/g, (_, name) => {
    const value = args[name];
    if (typeof value !== 'string' && typeof value !== 'number') return '';
    return String(value).slice(0, MAX_ARGUMENT);
  }).slice(0, MAX_TEXT);
}

export function locale() {
  const language = globalThis.document?.documentElement?.lang;
  return typeof language === 'string' && language.length <= 128 ? language : 'en';
}

export function number(value, options) {
  const safe = Number.isFinite(Number(value)) ? Number(value) : 0;
  try { return new Intl.NumberFormat(locale(), options).format(safe); } catch { return String(safe); }
}

export function plural(key, count, args = {}) {
  const safe = Number.isFinite(Number(count)) ? Number(count) : 0;
  let category = 'other';
  try { category = new Intl.PluralRules(locale()).select(safe); } catch { /* English fallback remains available. */ }
  const values = { ...args, count: number(safe) };
  return text(`${key}-${category}`, values) ?? text(`${key}-other`, values);
}

globalThis.rbitMessages = Object.freeze({ text, locale, number, plural });
