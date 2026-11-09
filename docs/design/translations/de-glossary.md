# German (`de`) glossary

One term per concept for the German interface, so every page uses the same word. The
tables are in `crates/rbit-localization/src/catalog/translations/de/`; how to add entries
is in `docs/design/localization-contracts.md`, "Translating a locale". The terminology
follows the German interfaces of mainstream developer tools (GitHub, GitLab): established
English loanwords stay where German developers use them.

## Style

- Address the reader formally with „Sie“ („Ihr Konto“, „Geben Sie … ein“).
- Buttons and menu actions are infinitives: „Speichern“, „Löschen“, „Branch erstellen“.
- Headings and labels are nouns or short phrases, no final period. Sentences end with a
  period.
- Quotation marks are „…“; the ellipsis is the single character `…`.
- Compounds with a loanword, an abbreviation or a product name take a hyphen:
  „SSH-Schlüssel“, „Pull-Request-Titel“, „Workflow-Datei“, „Git-LFS-Objekt“,
  „Deploy-Schlüssel“, „Branch-Schutzregel“. Plain German compounds are written together,
  and „Standardbranch“ stays closed, as GitHub writes it.
- Keep buttons and tabs short; prefer the noun-free infinitive when the context names the
  object.
- Never translate command names, refs, file names, keys, code or product names (rbit,
  Git, GitHub, Git LFS, SSH, HTTPS, SHA-256, OpenID Connect, WebAuthn, SQLite).

## Plurals

German has `Plural::One` (exactly 1) and `_` (everything else, including 0).

## Terms

| English | German | Notes |
| --- | --- | --- |
| repository | Repository (das), pl. Repositorys | |
| branch | Branch (der), pl. Branches | |
| default branch | Standardbranch | |
| base branch / head branch | Ziel-Branch / Quell-Branch | of a pull request |
| tag | Tag (der), pl. Tags | |
| ref | Ref (die), pl. Refs | |
| commit (n.) | Commit (der), pl. Commits | |
| commit (v.) | committen | |
| push / pull / clone | pushen / pullen / klonen | |
| pull request | Pull-Request (der), pl. Pull-Requests | |
| issue | Issue (das), pl. Issues | |
| thread (an issue or pull request) | Thread (der), pl. Threads | |
| merge (v.) | zusammenführen | |
| merge (n.) | Merge (der) | „Merge-Konflikt“, „Merge-Methode“ |
| merged | zusammengeführt | |
| squash / rebase / fast-forward | Squash / Rebase / Fast-Forward | |
| conflict | Konflikt | |
| fork (n. / v.) | Fork (der) / forken | |
| star (n. / v.) | Stern (der) / mit Stern markieren | |
| stargazers | Sterne | the list of people |
| watch | beobachten | |
| release | Release (das), pl. Releases | |
| asset (of a release) | Asset (das), pl. Assets | |
| milestone | Meilenstein | |
| label | Label (das), pl. Labels | |
| assignee | zugewiesene Person, pl. Zugewiesene | „Zugewiesen“ as a short label |
| review (n.) | Review (das) | |
| review (v.) | prüfen | |
| reviewer | Reviewer (der), pl. Reviewer | |
| request a review | Review anfordern | „Review-Anfrage“ |
| approve | genehmigen | |
| request changes | Änderungen anfordern | |
| comment | Kommentar / kommentieren | |
| conversation | Unterhaltung | |
| discussion | Diskussion | |
| draft | Entwurf | |
| open / closed (state) | Offen / Geschlossen | „offen“, „geschlossen“ inside sentences |
| reopen | erneut öffnen | |
| mention | erwähnen | |
| diff | Diff (der) | |
| file / directory | Datei / Verzeichnis | |
| blame | Blame | |
| raw | Roh | the raw-file button |
| upload / download | hochladen / herunterladen | |
| contribution | Beitrag | |
| contributor | Mitwirkende Person, pl. Mitwirkende | |
| collaborator | Mitarbeiter (der), pl. Mitarbeitende | |
| organization | Organisation | |
| team / member / owner | Team / Mitglied / Inhaber | |
| role / permission / access | Rolle / Berechtigung / Zugriff | |
| visibility | Sichtbarkeit | öffentlich, privat, intern |
| archive | archivieren | |
| mirror | Spiegel (der) / spiegeln | |
| settings | Einstellungen | |
| profile / account | Profil / Konto | |
| sign in / sign out / sign up | anmelden / abmelden / registrieren | |
| password | Passwort | |
| two-factor authentication | Zwei-Faktor-Authentifizierung | |
| recovery code | Wiederherstellungscode | |
| passkey | Passkey (der) | |
| session | Sitzung | |
| access token | Zugriffstoken (das) | „persönliches Zugriffstoken“ |
| scope (of a token) | Bereich | |
| SSH key / deploy key / GPG key | SSH-Schlüssel / Deploy-Schlüssel / GPG-Schlüssel | |
| fingerprint | Fingerabdruck | |
| webhook | Webhook (der) | |
| delivery (of a webhook) | Zustellung | |
| payload | Payload (die) | |
| secret | Secret (das), pl. Secrets | |
| variable | Variable | |
| branch protection rule | Branch-Schutzregel | |
| status check / check | Statusprüfung / Prüfung | |
| workflow | Workflow (der) | |
| run (of a workflow) | Ausführung | „Workflow-Ausführung“ |
| job / step | Job (der) / Schritt | |
| runner | Runner (der) | |
| artifact | Artefakt | |
| environment | Umgebung | |
| deployment | Deployment (das) | |
| re-run | erneut ausführen | |
| cancel (a run) | abbrechen | |
| log | Protokoll | |
| audit log | Audit-Log (das) | |
| wiki / page / revision | Wiki (das) / Seite / Revision | |
| dashboard | Dashboard (das) | |
| explore | Entdecken | |
| notifications / inbox | Benachrichtigungen / Posteingang | |
| appearance / theme | Darstellung / Design | |
| administrator / instance | Administrator / Instanz | |
| email | E-Mail (die) | |
| Learn more | Mehr erfahren | |
| Save / Cancel / Delete / Edit | Speichern / Abbrechen / Löschen / Bearbeiten | |
| Create / Close / Remove / Add | Erstellen / Schließen / Entfernen / Hinzufügen | |
