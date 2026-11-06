// Text from the compiled server catalog; never interpreted as markup.
export function pickerCount(dialog, count) {
  return `${count} ${count === 1 ? dialog.dataset.labelSingular : dialog.dataset.labelPlural}`;
}
export function pickerCredit(dialog, credit) {
  const by = credit.indexOf(" by ");
  return by >= 0 ? `${dialog.dataset.labelBy} ${credit.slice(by + 4)}` : dialog.dataset.labelCredit;
}
export function pickerApply(dialog, applied) {
  return applied ? dialog.dataset.labelApplied : dialog.dataset.labelApply;
}
