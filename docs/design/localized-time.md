# Localized times and numbers

Relative times, dates, durations, counts and sizes are presentation text: they follow the
page's language like every other catalog message. Machine values never do. `datetime`
attributes stay ISO 8601; ids, URLs, attributes, API fields, logs and stored values keep
plain ASCII digits. See [localization contracts](localization-contracts.md) for the
catalog itself.

## Numbers

`rbit_localization::number` groups digits and places the decimal mark per locale (CLDR 46):

| Locale | Grouping | Decimal |
|---|---|---|
| `en`, `zh-Hans`, `ja`, `ko`, diagnostics | `1,234` | `1.5` |
| `de`, `pt-BR` | `1.234` | `1,5` |
| `es` | `1234`, `12.345` (CLDR minimum grouping 2) | `1,5` |
| `fr` | `1 234` with U+202F (narrow no-break space) | `1,5` |
| `ru` | `1 234` with U+00A0 (no-break space) | `1,5` |

`MessageWriter::number` and `RichWriter::number` use it, so every count written through a
family is grouped in the language being written (an English fallback in English).
`MessageWriter::digits` writes a label such as a year, never grouped.

## Times and dates

`rbit_localization::time::TimeMessage` is a table-backed family: each locale writes its
own phrases in `catalog/translations/<locale>/time.rs`, choosing noun forms with its plural
categories (Russian one/few/many, French and Portuguese with 0 and 1 as one, Chinese,
Japanese and Korean with one form).

| Message | English | Examples |
|---|---|---|
| `JustNow` | just now | gerade eben, только что, たった今 |
| `Ago(Amount)` | 3 hours ago | vor 3 Stunden, há 3 horas, 3 часа назад, 3小时前 |
| `In(Amount)` | in 2 days | in 2 Tagen, dans 2 jours, через 2 дня, 2일 후 |
| `WithinAMinute` | in under a minute | en menos de un minuto, 1 分以内 |
| `OnDate(CivilDate)` | on Nov 8, 2026 | am 8. Nov. 2026, le 8 nov. 2026, em 8 de nov. de 2026 |
| `Date(CivilDate)` | Nov 8, 2026 | 8 nov 2026, 8 нояб. 2026 г., 2026年11月8日, 2026년 11월 8일 |
| `Duration` | 3 hours 5 minutes | 3 Stunden 5 Minuten, 3 heures et 5 minutes, 3 時間 5 分 |
| `Elapsed` | 1h 2m | 1 Std. 2 Min., 1 h 2 min, 1 ч 2 мин, 1時間2分 |

Units are seconds, minutes, hours, days, weeks, months and years. `TimeMessage::past` and
`TimeMessage::future` choose the unit for a span of seconds the same way in every language
(minutes below an hour, hours below a day, days below 30 days, months below a year, then a
date); `future` rounds, so a 90-day expiry seen an hour later is still "in 3 months".

## The web adapter

`rbit_web::views::when` bridges core timestamps to the catalog. A view keeps the moment
(`When`), not text, and the template formats it with the page's locale:

```html
<time datetime="{{ row.when_iso }}">{% call ui::localized(row.when.relative(page.locale)) %}{% endcall %}</time>
```

`When::relative`, `When::until` and `When::date` return a `Localized`, which `ui::localized`
marks with its own `lang` where the page around it may be English. `count`, `bytes`,
`Size`, `uptime` and `Took` (an Actions duration) format quantities. A sentence that is
still hard-coded English (parts of the security settings) keeps an English time and carries
`lang="en"`. Where a translated sentence takes a time as an argument (`TokenText::LastUsed`,
`ActionsSecretsMessage::Updated`), the time is formatted in the same locale first.

## Caches

A response shared between readers must not carry one reader's language:

- The tree page's last-commit cells (`/tree-commits/…`) are pinned: cached for a year and
  shared by readers of every language. They show the commit's UTC day as ISO 8601
  (`2026-11-08`) beside the full `datetime`, so they are byte-identical in every catalog
  and never go stale the way a cached "2 days ago" did. The tree page's own latest commit
  shows the relative time in the page's language.
- The pull request counts fragment stays English and language-neutral.
- The Markdown document cache holds no times or counts.

## Not localized yet

The pinned last-commit cells could show a relative phrase in the reader's language from
their `datetime` on the client; they do not yet. Text that is still hard-coded English
keeps English times and numbers: the security
settings' session, passkey and linked-account lines, the explore page's original
`Message` counts, and the services' own messages (commit status descriptions such as
"Successful in 1m 3s", notification emails, API errors).
