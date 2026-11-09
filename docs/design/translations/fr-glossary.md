# French (`fr`) glossary

The terms the French tables in
`crates/rbit-localization/src/catalog/translations/fr/` use, one per concept. Follow
it when adding or reviewing French text, so a concept reads the same on every page.
The terminology follows the French interfaces of mainstream developer tools.

## Style

- Address the user as « vous ». Buttons and menu actions are infinitives
  (« Enregistrer », « Supprimer le dépôt »); headings are nouns (« Paramètres »).
- Sentence case: only the first word and proper nouns take a capital.
- Accented capitals: « État », « À propos », « Éditer ».
- Apostrophe: the typographic `’` (U+2019), with elision (« l’organisation »,
  « d’accès », « qu’il »).
- A narrow no-break space (`\u{202f}`) before `;` `?` `!`, a no-break space (`\u{a0}`)
  before `:`, and inside guillemets: `«\u{a0}…\u{a0}»`.
- Ellipsis: `…` (one character). Numbers come from the argument, never spelled out.
- Plurals: 0 and 1 are singular (`Plural::One`); `_` takes the plural form.
- Never translate machine values, code, commands (`git push`), file names, keys, or
  product and protocol names: rbit, Git, GitHub, Git LFS, SSH, HTTPS, SMTP, OIDC,
  OpenID Connect, SHA-256, WebAuthn, CI/CD, REST API, YAML, README.

## Terms

| English | French | Notes |
|---|---|---|
| repository | dépôt (m.) | |
| branch | branche (f.) | default branch: branche par défaut |
| commit (noun) | commit (m.) | to commit: valider (« Valider les modifications »); commit message: message de commit |
| head branch / base branch | branche source / branche de base | head repository: dépôt source |
| tag (Git) | tag (m.) | never « étiquette », which is a label |
| label | étiquette (f.) | |
| pull request | pull request (f.) | « la pull request », plural « pull requests » |
| issue | ticket (m.) | |
| milestone | jalon (m.) | |
| assignee | personne assignée (f.) | to assign: assigner; assign yourself: vous assigner |
| review | revue (f.) | reviewer: relecteur (m.); request a review: demander une revue |
| approve | approuver | request changes: demander des modifications |
| merge | fusionner / fusion (f.) | merge commit: commit de fusion |
| squash and merge | fusionner par squash | squash: squash (m.) |
| rebase and merge | rebaser et fusionner | rebase: rebase (m.) |
| conflict | conflit (m.) | |
| fork | fork (m.) | to fork: créer un fork |
| star | étoile (f.) | to star: ajouter une étoile; unstar: retirer l’étoile |
| watch | suivre | |
| clone | cloner | |
| push / pull (Git) | push (m.) / pull (m.) | to push: pousser |
| pull mirror / push mirror | miroir pull / miroir push (m.) | |
| release | release (f.) | asset: fichier (m.) |
| draft | brouillon (m.) | |
| diff | diff (m.) | |
| blame | blame (m.) | |
| file / folder | fichier (m.) / dossier (m.) | |
| README | README | |
| history | historique (m.) | |
| compare | comparer / comparaison (f.) | |
| comment | commentaire (m.) | to comment: commenter |
| thread / conversation | conversation (f.) | |
| timeline | chronologie (f.) | |
| check (CI) | vérification (f.) | required check: vérification requise |
| status (commit) | statut (m.) | state (open/closed): état (m.) |
| open / closed / merged | ouvert / fermé / fusionné | agree with the noun: « ticket ouvert », « pull request ouverte » |
| reopen | rouvrir | |
| branch protection | protection de branche (f.) | rule: règle (f.) |
| workflow | workflow (m.) | |
| run (Actions) | exécution (f.) | |
| job | job (m.) | step: étape (f.) |
| runner | runner (m.) | self-hosted: auto-hébergé |
| artifact | artefact (m.) | |
| secret | secret (m.) | variable: variable (f.) |
| environment | environnement (m.) | deployment: déploiement (m.) |
| log(s) | journal (m.) / journaux | audit log: journal d’audit |
| webhook | webhook (m.) | delivery: livraison (f.); payload: charge utile (f.) |
| access token | jeton d’accès (m.) | token: jeton; scope: portée (f.) |
| SSH key | clé SSH (f.) | fingerprint: empreinte (f.) |
| passkey | clé d’accès (f.) | |
| two-factor authentication | authentification à deux facteurs (f.) | authenticator app: application d’authentification |
| recovery codes | codes de récupération (m.) | |
| password | mot de passe (m.) | |
| sign in / sign out / sign up | se connecter / se déconnecter / s’inscrire | noun: connexion, déconnexion, inscription |
| account | compte (m.) | |
| profile | profil (m.) | avatar: avatar (m.) |
| settings | paramètres (m.) | configuration: configuration (f.) |
| administration | administration (f.) | site administrator: administrateur |
| instance | instance (f.) | |
| organization | organisation (f.) | |
| team | équipe (f.) | |
| member / owner | membre (m.) / propriétaire (m.) | |
| collaborator | collaborateur (m.) | |
| permission | autorisation (f.) | read/write/admin: lecture / écriture / administration |
| visibility | visibilité (f.) | public / private: public / privé |
| archive (verb) | archiver | archived: archivé |
| inbox | boîte de réception (f.) | notification: notification (f.) |
| dashboard | tableau de bord (m.) | |
| explore | explorer | |
| theme | thème (m.) | appearance: apparence (f.) |
| wiki | wiki (m.) | page: page (f.); revision: révision (f.) |
| search | rechercher / recherche (f.) | filter: filtre (m.) |
| save / cancel / delete | enregistrer / annuler / supprimer | |
| edit | modifier | |
| upload | téléverser / envoyer | |
| download | télécharger | |
| email | e-mail (m.) / adresse e-mail (f.) | |
| uptime | disponibilité (f.) | |
| rate limiting | limitation de débit (f.) | |
| upstream / remote | amont (m.) / dépôt distant (m.) | |
| fetch (mirror) | récupération (f.) / récupérer | |
| branding and policies | image de marque et règles | |
| timeout | délai (d’expiration) (m.) | |
| KiB, bytes | Kio, octets | |

## Terms to revisit

- « pull request » stays in English (as in GitHub’s French interface) rather than
  « demande de fusion » or « demande de tirage ».
- « ticket » rather than « issue » or « problème ».
- « release » rather than « version », which the interface already uses for the running
  software version.
- « runner » and « job » stay in English, like GitLab’s French interface.
