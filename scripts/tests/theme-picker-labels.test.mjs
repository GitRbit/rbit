import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("../../crates/rbit-web/assets/theme-picker-labels.js", import.meta.url), "utf8");
const context = vm.createContext({});
vm.runInContext(source.replace(/^export /gm, ""), context);
test("picker updates keep translated text and author bytes out of HTML", () => {
  const dialog = { dataset: { plural: "es", countOne: "{n} tema", countMany: "{n} de temas", countOther: "{n} temas", labelBy: "Por",
    labelCredit: "Integrado en rbit", labelApply: "Aplicar", labelApplied: "Aplicado" } };
  assert.equal(context.pickerCount(dialog, 0), "0 temas");
  assert.equal(context.pickerCount(dialog, 1), "1 tema");
  assert.equal(context.pickerCount(dialog, 250), "250 temas");
  assert.equal(context.pickerCount(dialog, 1_000_000), "1000000 de temas");
  assert.equal(context.pickerCredit(dialog, "Name by <img src=x> & Author"), "Por <img src=x> & Author");
  assert.equal(context.pickerCredit(dialog, "System · built in"), "Integrado en rbit");
  assert.equal(context.pickerApply(dialog, true), "Aplicado");
  assert.equal(context.pickerApply(dialog, false), "Aplicar");
});
test("theme machine filter and no-script appearance fallback remain independent of labels", () => {
  const template = readFileSync(new URL("../../crates/rbit-web/templates/themes/picker.html", import.meta.url), "utf8");
  assert.deepEqual([...template.matchAll(/data-tpk-variant="([^"]*)"/g)].map(match => match[1]), ["", "dark", "light", "colorblind"]);
  const shell = readFileSync(new URL("../../crates/rbit-web/templates/base.html", import.meta.url), "utf8");
  assert.equal((shell.match(/href="\/settings\/appearance"[^>]*data-theme-dialog-open/g) ?? []).length, 2);
});
test("live counts follow CLDR plural rules: Russian one, few and many", () => {
  const russian = { dataset: { plural: "ru", countOne: "{n} тема", countFew: "{n} темы", countMany: "{n} тем" } };
  const expected = { 0: "тем", 1: "тема", 2: "темы", 4: "темы", 5: "тем", 11: "тем", 12: "тем", 14: "тем",
    21: "тема", 22: "темы", 25: "тем", 101: "тема", 111: "тем", 112: "тем", 122: "темы" };
  for (const [count, noun] of Object.entries(expected)) {
    assert.equal(context.countLabel(russian, Number(count)), `${count} ${noun}`, `${count}`);
  }
  assert.equal(context.pluralCategory("fr", 0), "one");
  assert.equal(context.pluralCategory("pt-BR", 1), "one");
  assert.equal(context.pluralCategory("en", 0), "other");
  assert.equal(context.pluralCategory("ja", 1), "other");
  assert.equal(context.pluralCategory("en-XA", 1), "one");
  // The form, not the code, orders the number: Korean puts it inside the phrase.
  assert.equal(context.countLabel({ dataset: { plural: "ko", countOther: "테마 {n}개" } }, 3), "테마 3개");
  // Unknown categories fall back to the general form rather than showing nothing.
  assert.equal(context.countLabel({ dataset: { plural: "de", countOther: "{n} Designs" } }, 1), "1 Designs");
});
test("both theme counts carry their server forms", () => {
  for (const path of ["picker.html", "gallery.html"]) {
    const template = readFileSync(new URL(`../../crates/rbit-web/templates/themes/${path}`, import.meta.url), "utf8");
    assert.match(template, /data-plural="\{\{ \w+\.count_plural\(\) \}\}"\{% for \(category, form\) in \w+\.count_forms\(\) %\} data-count-\{\{ category \}\}="\{\{ form \}\}"\{% endfor %\}/, path);
    assert.doesNotMatch(template, /label-singular|label-plural|shown_label/, path);
  }
  const picker = readFileSync(new URL("../../crates/rbit-web/assets/theme-picker.js", import.meta.url), "utf8");
  assert.match(picker, /count\.textContent = countLabel\(count, shown\);/);
  assert.doesNotMatch(picker, /"theme" : "themes"/);
});
