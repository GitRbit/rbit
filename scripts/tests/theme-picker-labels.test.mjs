import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("../../crates/rbit-web/assets/theme-picker-labels.js", import.meta.url), "utf8");
const context = vm.createContext({});
vm.runInContext(source.replace(/^export /gm, ""), context);
test("picker updates keep translated text and author bytes out of HTML", () => {
  const dialog = { dataset: { labelSingular: "tema", labelPlural: "temas", labelBy: "Por",
    labelCredit: "Integrado en rbit", labelApply: "Aplicar", labelApplied: "Aplicado" } };
  assert.equal(context.pickerCount(dialog, 0), "0 temas");
  assert.equal(context.pickerCount(dialog, 1), "1 tema");
  assert.equal(context.pickerCount(dialog, 250), "250 temas");
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
