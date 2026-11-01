# Enabled-i18n fixture and actual callback prerequisites

Prepare this fixture with the completed, verified baseline default-feature artifact.
Fixture setup and smoke are functional checks; record concurrent build activity in
their evidence. Finish all compilation and heavy CPU work, then verify the registered
idle/thermal conditions before pilot or confirmatory timing.

Use only the three actual bundled validated archives; do not manufacture additional
packages or claim a maximum-capacity inventory workload:

| File | Exact ID | Locale | Version | SHA-256 |
| --- | --- | --- | --- | --- |
| es.rbit-language | language.es | es | 1.1.0 | 39a377a176a74f30339a4d9b5f4a7b198a5d9adaa47085be8e11bc8dd91a3a16 |
| en-XA.rbit-language | language.en-xa | en-XA | 1.1.0 | 6cba3fb0bded0fe49295fa9b210e8f785dc5106e4fe9fe8f55a08c557fef06c2 |
| ar-XB.rbit-language | language.ar-xb | ar-XB | 1.1.0 | 228d34d6a7ba05a05cfd49a9a893f5885859f76f9cf872be3afd06a2572dbb1f |

Spanish is an existing partial translation, not an invented complete language pack.
Read-only inspection of its actual Fluent archive confirmed `locale-title` =
`Idioma`, `locale-description` = `Elige el idioma para tu cuenta o este navegador.`,
and `plugin-install-language-pack` = `Instalar paquete de idioma`. Enabled GETs
must show those registered form/inventory markers as well as `Content-Language: es`,
HTML `lang="es"`, `dir="ltr"` and all expected locale/pack IDs. The pseudo-locales
remain installed data in the fixture; they are not the chosen render locale here.

1. Start an explicitly owned baseline default-feature release server on a fresh
   private seed data directory using `owned-serve.py`. It strips inherited RBIT_
   variables and keeps Actions/i18n/SSH/update workers disabled. No setup credentials
   go to an arbitrary already-listening port; wrapper probes the port and writes
   matching live PID/metadata ownership records. This is still a trusted idle-host
   protocol, not protection against hostile same-user local port/PID races.
2. Generate the deterministic existing fixture, then use `scripts/perf/seed.mjs`
   against that explicitly owned instance. The real first-account signup/setup flow
   must establish the administrator. Read the owned instance's setup token privately
   into an owned regular mode0600 file and pass only its path with
   `--setup-token-file`; the helper submits the official signup field. Never insert
   an admin or session with SQL, synthesize signed
   cookies, bypass Origin or infer privilege from a username. Retain Git refs/object
   identity and user/account row identity in private evidence.
3. The existing seed helper keeps throwaway `login:password` in its ignored
   credentials file. Convert privately to a JSON file with exactly `login` and
   `password`, mode0600; never put either in argv, environment, logs or results.
   The original credential file also needs private permissions. Do not reuse real
   deployment credentials. The new client logs in through `/login`, retains the
   host-issued `rbit_session` only in memory and verifies the real admin inventory.
4. Install only the three hash-pinned archives through actual administrator GET
   proofs plus bounded multipart POSTs while the provider remains disabled:

```sh
python3 scripts/perf/i18n-extraction/owned-serve.py --build-record /ABS/A-default-build.json --data-dir /ABS/seed-data --metadata /ABS/seed-server.json --port 3005
# In a separate controlled shell after the wrapper reports readiness:
node scripts/perf/fixture.mjs
node scripts/perf/seed.mjs http://127.0.0.1:3005 --setup-token-file /ABS/private-setup-token
node scripts/perf/i18n-extraction/harness/seed-i18n.mjs /ABS/seed-server.json /ABS/private-login.json /ABS/exact-source/language-packs /ABS/seed-i18n-record.json
```

The installer uses the fixed inventory/install paths, freshly obtained CSRF proofs,
real password authentication and exact current archive bytes. It never follows a
redirect, retries an uncertain install or enables the provider. Final GET must
contain all three IDs. Idempotent existing exact archives may be retained, but a
conflict fails setup rather than silently changing fixture bytes.

5. Stop the seed wrapper cleanly, drain/close all stores, verify no live writer or
   SQLite sidecar remains, and inspect read-only state. After establishing that
   no writer, open handle or sidecar exists, Python SQLite inspections may use
   `mode=ro&immutable=1` to avoid recreating empty WAL/shared-memory files. Ordinary
   `mode=ro` can recreate those files. If this occurs, record it and perform an
   explicit closed-store checkpoint; verify the base DB hash and final sidecar
   absence. Never delete a possibly uncheckpointed WAL or use immutable mode on a
   live store. In the closed SQLite DB,
   `SELECT plugin_id,enabled FROM plugin_settings WHERE plugin_id IN
   ('rbit.i18n','rbit.actions')` must return no rows for this fresh fixture. A persisted
   false would override enabled-profile environment just as a true would override
   disabled/core configuration. Do not fix this with ad hoc SQL; prepare a fresh
   correctly configured seed. `SELECT id,locale,version,digest,length(bytes) FROM
   language_packs ORDER BY id` must show exactly the three identities/digests above.
   Check archive BLOB hashes, account locale/preferences and absence of queued work;
   record row counts/schema, Git refs and complete content inventory without revealing
   secret key/cookie/password bytes. Reuse this one immutable seed across all strata.
6. For the enabled profile only, config default locale is `es`, effective i18n is
   true and Actions remains false. Preserve absent account preference so default
   resolution is intentional. Default-disabled/core profiles remain English. At
   every enabled process start the sampler performs a real password login for the
   existing administrator and a separate anonymous picker GET. Guest keeps only
   `rbit-plugin-browser`/optional locale cookie; admin keeps only `rbit_session`/
   optional locale cookie. Exact origin including port is mandatory; jars are never
   copied between variants or roles. GET proves authority through the real protected
   inventory and is checked before warmups. Invalid credentials, expired sessions,
   sign-in redirects, missing packs, English fallback/mismatched metadata or body
   semantics abort the attempted run. Proof values are issued by the host, never
   forged; they are not submitted during GET latency measurements.
7. Run the non-performance owned A/B smoke plan and register its exact seed/spec/
   binary/evidence hashes before any pilot. In the actual benchmark, 20 warmups and
   100 measured GET rounds follow this bootstrap. Stream semantics and cookie digest
   stability apply to every response. Postflight GETs recheck role/locale/inventory
   outside the measured matrix and are preserved immutably in the sampler result.

A malformed archive rerender is a useful **functional-only** assertion: authenticate,
obtain a fresh install proof, POST known invalid bounded bytes, expect the compiled
validation document and exactly unchanged three-pack inventory/digests. It is not a
mutable latency endpoint and must not run within any warmup/measured block. Existing
Web upload/proof/repair tests remain separate evidence. No setup or measurement
here grants package execution, broker authority or sandbox/worker admission.
