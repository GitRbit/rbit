// The translation coverage report reads every locale table and every family's English
// declaration. It is a progress report, not a gate: these tests only prove it can count.
import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { coverage, tableEntries, variantCount } from "../i18n-coverage.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));

test("every locale table lists the same families, each with English messages", () => {
  const results = coverage(root);
  assert.deepEqual([...results.keys()], ["de", "es", "fr", "ja", "ko", "pt_br", "ru", "zh_hans"]);
  const families = (rows) => rows.map((row) => row.family).sort();
  const expected = families(results.get("de"));
  assert.ok(expected.length > 70, `found ${expected.length} families`);
  for (const [locale, rows] of results) {
    assert.deepEqual(families(rows), expected, `${locale} lists every family`);
    for (const row of rows) {
      assert.ok(row.total > 0, `${row.family} has English messages`);
      assert.ok(row.translated <= row.total, `${locale} ${row.family}`);
    }
  }
  const config = results.get("de").find((row) => row.family === "ConfigText");
  assert.ok(config.total > 100, "configuration schema entries are counted");
});

test("table arms are counted once per message", () => {
  const source = [
    "entries! { De:",
    "    ReviewMessage<'a> => MessageWriter |text| {",
    "        ReviewMessage::Approve => text.literal(\"Genehmigen\"),",
    "        ReviewMessage::Files { count } | ReviewMessage::Lines { count } => {",
    "            text.number(count);",
    "        }",
    "        ReviewMessage::Approve => text.literal(\"twice\"),",
    "    }",
    "    ChecksMessage => MessageWriter |text| {}",
    "    ConfigText<'a> => MessageWriter |text| {",
    "        ConfigText::Label { section: \"mail\", key: \"transport\" } => text.literal(\"Versand\"),",
    "        ConfigText::SectionTitle(\"mail\") => text.literal(\"E-Mail\"),",
    "    }",
    "}",
  ].join("\n");
  assert.deepEqual([...tableEntries(source)], [["ReviewMessage", 3], ["ChecksMessage", 0], ["ConfigText", 2]]);
});

test("variants are counted for enums and for declaring macros", () => {
  const source = [
    "pub enum Thing<'a> {",
    "    /// Docs.",
    "    Plain,",
    "    WithFields {",
    "        name: &'a str,",
    "    },",
    "    #[allow(dead_code)]",
    "    Tuple(u64),",
    "}",
    "labels! {",
    "    /// Fixed labels.",
    "    Labels, \"prefix\" {",
    "        One = \"one\" => \"One\",",
    "        Two = \"two\" => \"Two\",",
    "    }",
    "}",
  ].join("\n");
  assert.equal(variantCount(source, "Thing"), 3);
  assert.equal(variantCount(source, "Labels"), 2);
  assert.equal(variantCount(source, "Missing"), undefined);
});
