// Text from the compiled server catalog; never interpreted as markup.

// The CLDR plural category of a whole number, by the rules the server's catalog uses
// (rbit-localization `plural.rs`), so a live count matches the server-rendered one.
export function pluralCategory(lang, count) {
  const language = String(lang ?? "").toLowerCase().split("-")[0];
  const million = count !== 0 && count % 1_000_000 === 0;
  switch (language) {
    case "ru": {
      // one: 1, 21, 31 … (not 11); few: 2–4, 22–24 … (not 12–14); many: the rest.
      const ones = count % 10;
      const tens = count % 100;
      if (ones === 1 && tens !== 11) return "one";
      if (ones >= 2 && ones <= 4 && (tens < 12 || tens > 14)) return "few";
      return "many";
    }
    case "zh": case "ja": case "ko": return "other";
    case "fr": case "pt": return count === 0 || count === 1 ? "one" : million ? "many" : "other";
    case "es": return count === 1 ? "one" : million ? "many" : "other";
    default: return count === 1 ? "one" : "other";
  }
}

// A count in the server's words: the holder's `data-count-<category>` form for the count's
// plural category in its `data-plural` language, with `{n}` replaced by the number.
export function countLabel(holder, count) {
  const data = holder.dataset;
  const category = pluralCategory(data.plural, count);
  const form = data[`count${category[0].toUpperCase()}${category.slice(1)}`] ?? data.countOther ?? data.countMany ?? "{n}";
  return form.replace("{n}", String(count));
}
export function pickerCount(dialog, count) {
  return countLabel(dialog, count);
}
export function pickerCredit(dialog, credit) {
  const by = credit.indexOf(" by ");
  return by >= 0 ? `${dialog.dataset.labelBy} ${credit.slice(by + 4)}` : dialog.dataset.labelCredit;
}
export function pickerApply(dialog, applied) {
  return applied ? dialog.dataset.labelApplied : dialog.dataset.labelApply;
}
