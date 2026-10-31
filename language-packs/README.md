# Example language data

Install these data-only `.rbit-language` archives through **Admin → Plugins**.

- **es**: partial Spanish starter vocabulary; requires linguistic review before treating it as a production translation.
- **en-XA**: accented expansion pseudolocale.
- **ar-XB**: RTL pseudolocale, not an Arabic translation.

The complete shared schema, rather than an external translation service, is the source for
the pseudolocales. Partial catalogs fall back to built-in English. Typed selectors demonstrate
zero/one/many handling; browser plural categories use the browser's `Intl.PluralRules`.

Regenerate: `python3 scripts/build-language-packs.py`.
Check: `python3 scripts/build-language-packs.py --check`.

See [authoring, trust and resource limits](../docs/i18n.md). Checksums are integrity metadata,
not a publisher signature. Do not install translations from a publisher you do not trust.
