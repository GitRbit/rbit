// Functional real-host prerequisite only. Never invokes the measured sampler loop.
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bootstrapContexts, hash } from './contexts.mjs';
import { validateConfig } from './measure.mjs';
function canonical(value) {
  if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
  if (value && typeof value === 'object') return '{' + Object.keys(value).sort().map(key => JSON.stringify(key) + ':' + canonical(value[key])).join(',') + '}';
  return JSON.stringify(value);
}
export function enabledSpec(config) {
  const spec = config.profileSpec;
  if (!spec || spec.name !== 'i18n-enabled' || canonical(spec.pages) !== canonical(config.pages) || canonical(spec.requestContexts) !== canonical(config.requestContexts)
      || spec.features.length || spec.runtimeEnv.RBIT_I18N__ENABLED !== 'true' || spec.runtimeEnv.RBIT_I18N__DEFAULT_LOCALE !== 'es' || spec.runtimeEnv.RBIT_ACTIONS__ENABLED !== 'false') throw new Error('Smoke requires the registered enabled-i18n profile');
  const [picker, inventory] = spec.pages;
  if (spec.pages.length !== 2 || picker.path !== '/-/plugins/rbit.i18n/language' || inventory.path !== '/-/plugins/rbit.i18n-admin/packs'
      || picker.context !== 'guest' || inventory.context !== 'admin' || picker.contentLanguage !== 'es' || inventory.contentLanguage !== 'es'
      || spec.requestContexts.guest.kind !== 'guest' || spec.requestContexts.admin.kind !== 'admin-session'
      || !['value="es"', 'value="en-XA"', 'value="ar-XB"', 'name="_csrf"', '>Idioma</label>', '<!doctype html>', 'lang="es"', 'dir="ltr"'].every(marker => picker.requiredMarkers.includes(marker))
      || !picker.forbiddenMarkers.includes('data-signed-in')
      || !['language.es', 'language.en-xa', 'language.ar-xb', 'packs/install', 'packs/remove', 'name="_csrf"', 'Instalar paquete de idioma', '<!doctype html>', 'lang="es"', 'dir="ltr"'].every(marker => inventory.requiredMarkers.includes(marker))) throw new Error('Smoke omitted registered guest/admin locale/archive semantics');
  if (typeof config.seedFileManifestSha256 !== 'string' || !/^[0-9a-f]{64}$/.test(config.seedFileManifestSha256)) throw new Error('Smoke requires exact seed identity');
  return hash(canonical(spec));
}

export async function smoke(config) {
  config = validateConfig(config);
  const profileSpecSha256 = enabledSpec(config);
  if (new Set(config.variants.map(v => new URL(v.baseUrl).origin)).size !== 2) throw new Error('Smoke requires two independent owned process origins');
  const results = [];
  for (const variant of config.variants) {
    const metadata = JSON.parse(readFileSync(variant.metadata, 'utf8'));
    if (new URL(metadata.baseUrl).origin !== new URL(variant.baseUrl).origin || metadata.profile !== 'release') throw new Error('Functional host metadata does not bind the registered release origin');
    const bootstrap = await bootstrapContexts(variant, config);
    results.push({ variant: variant.name, revision: metadata.declaredSourceRevision, executableSha256: metadata.executableSha256, contexts: bootstrap.evidence });
  }
  const baseline = results.find(result => result.variant === 'baseline'), candidate = results.find(result => result.variant === 'candidate');
  if (!baseline || !candidate || !baseline.revision || !candidate.revision) throw new Error('Functional smoke requires exact baseline/candidate identities');
  return { schema: 1, isPerformanceRun: false, semanticValid: true, profileSpecSha256, seedFileManifestSha256: config.seedFileManifestSha256, baselineRevision: baseline.revision, candidateRevision: candidate.revision, results, completedAtUTC: new Date().toISOString(), limits: 'Functional origin/auth/locale markers only; no timing bound, full HTML equivalence or independent compiler attestation.' };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 4) { console.error('usage: smoke.mjs CONFIG.json OUTPUT.json'); process.exitCode = 1; }
  else smoke(JSON.parse(readFileSync(process.argv[2], 'utf8'))).then(result => { writeFileSync(process.argv[3], JSON.stringify(result, null, 2) + '\n', { flag: 'wx', mode: 0o600 }); console.log('Functional i18n host prerequisites passed; no performance samples collected.'); }).catch(() => { console.error('Functional host prerequisite failed; no timing authorized by this evidence.'); process.exitCode = 1; });
}
