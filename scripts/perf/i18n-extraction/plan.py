#!/usr/bin/env python3
"""Generate plans/configs only. This program never builds, starts or times a server."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def load(path):
    return json.loads(Path(path).read_text())


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def require(ok, message):
    if not ok:
        raise ValueError(message)


def inventory(directory):
    directory = Path(directory).resolve()
    files = {}
    for path in sorted(directory.rglob('*')):
        require(not path.is_symlink(), f'Symlink requires explicit separate treatment: {path}')
        if path.is_file():
            files[path.relative_to(directory).as_posix()] = digest(path)
    require(files, f'Empty inventory: {directory}')
    return {'files': files}


def checked_file(path, expected):
    require(path and expected, 'Missing artifact path/hash')
    require(digest(path) == expected, f'Artifact digest differs: {path}')


def checked_tree(directory, manifest):
    require(directory and manifest, 'Missing directory/inventory')
    require(inventory(directory) == load(manifest), f'Directory inventory differs: {directory}')


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()


def family(manifest):
    require(manifest['protocolVersion'] == 'i18n-extraction-ten-cell-v2', 'A different workload needs a separately reviewed prospective protocol version')
    profiles = manifest['profiles']
    require([p['name'] for p in profiles] == ['default-disabled', 'core-only', 'i18n-enabled'], 'Prospective v2 feature strata cannot shrink or change')
    core_paths = ['/', '/perf/perf-fixture', '/perf/perf-fixture/blob/main/src/medium.rs', '/perf/perf-fixture/compare/main...big-change']
    enabled_paths = ['/-/plugins/rbit.i18n/language', '/-/plugins/rbit.i18n-admin/packs']
    for index, profile in enumerate(profiles):
        require(profile['features'] == (['--no-default-features'] if index == 1 else []), 'Compile feature stratum changed')
        require([page['path'] for page in profile['pages']] == (enabled_paths if index == 2 else core_paths), 'Prospective v2 page family changed')
        require(profile['runtimeEnv']['RBIT_ACTIONS__ENABLED'] == 'false' and profile['runtimeEnv']['RBIT_I18N__ENABLED'] == ('true' if index == 2 else 'false'), 'Runtime provider stratum changed')
        if index < 2:
            require(set(profile['requestContexts']) == {'anonymous'} and profile['requestContexts']['anonymous']['kind'] == 'anonymous' and all(page['context'] == 'anonymous' for page in profile['pages']), 'Core pages must remain anonymous')
        else:
            picker, inventory = profile['pages']
            require(profile['runtimeEnv']['RBIT_I18N__DEFAULT_LOCALE'] == 'es', 'Enabled locale changed')
            require(set(profile['requestContexts']) == {'guest', 'admin'} and profile['requestContexts']['guest']['kind'] == 'guest' and profile['requestContexts']['admin']['kind'] == 'admin-session', 'Enabled UI requires distinct guest and administrator contexts')
            require(picker['context'] == 'guest' and inventory['context'] == 'admin' and picker['contentLanguage'] == inventory['contentLanguage'] == 'es', 'Enabled callback authority/locale changed')
            require({'value="es"', 'value="en-XA"', 'value="ar-XB"', 'name="_csrf"', '>Idioma</label>', '<!doctype html>', 'lang="es"', 'dir="ltr"'}.issubset(picker['requiredMarkers']) and 'data-signed-in' in picker['forbiddenMarkers'], 'Picker locale/anonymous semantics missing')
            require({'language.es', 'language.en-xa', 'language.ar-xb', 'packs/install', 'packs/remove', 'name="_csrf"', 'Instalar paquete de idioma', '<!doctype html>', 'lang="es"', 'dir="ltr"'}.issubset(inventory['requiredMarkers']), 'Administrator inventory/archive/translated semantics missing')
    require(profiles and len({p['name'] for p in profiles}) == len(profiles), 'Duplicate/empty feature profiles')
    cells = 0
    for profile in profiles:
        pages = profile['pages']
        require(pages and len({p['name'] for p in pages}) == len(pages) and len({p['path'] for p in pages}) == len(pages), 'Duplicate/empty profile pages')
        require(all(p['context'] in profile['requestContexts'] for p in pages), 'Unbound request context')
        cells += len(pages)
    require(manifest['analysis']['primaryCells'] == cells, 'Declared primary family differs from profile/page cells')
    alpha = .05 / cells
    require(math.isclose(manifest['analysis']['oneSidedCellAlpha'], alpha, rel_tol=0, abs_tol=1e-15), 'Wrong prospective family correction')
    return cells, alpha


def phase_schedule(phase, n=None):
    if phase == 'smoke':
        return [('smoke', 'A', 'B')]
    if phase == 'pilot':
        return [('pilot-AA', 'A', 'A'), ('pilot-BB', 'B', 'B')] * 12
    require(phase == 'confirmatory' and n in (80, 160, 320), 'Invalid frozen process count')
    result = [('control-before', 'A', 'A')] * 10
    for i in range(n):
        result.append(('comparison', 'A', 'B'))
        if (i + 1) * 10 // n > i * 10 // n:
            result.append(('control-BB', 'B', 'B'))
    return result + [('control-after', 'A', 'A')] * 10


def checked_smoke(manifest):
    checked_file(manifest['realHostSmokeEvidence'], manifest['realHostSmokeEvidenceSha256'])
    checked_file(manifest['realHostSmokeProtocol'], manifest['realHostSmokeProtocolSha256'])
    result = load(manifest['realHostSmokeEvidence'])
    protocol = load(manifest['realHostSmokeProtocol'])
    plan_path = Path(manifest['realHostSmokeProtocol']).with_name('commands.json')
    specification = load(plan_path)
    require(protocol['schema'] == 2 and protocol['phase'] == 'smoke' and protocol['complete'] is True and protocol['valid'] is True, 'Smoke phase did not finalize cleanly')
    require(protocol['planSha256'] == digest(plan_path) and protocol['executorSha256'] == digest(Path(__file__).with_name('execute.py')) and specification['planScriptSha256'] == digest(__file__), 'Smoke plan/executor implementation differs')
    require(specification['schema'] == 2 and specification['phase'] == 'smoke' and len(specification['blocks']) == len(protocol['blocks']) == 1, 'Smoke must complete the registered single owned A/B block')
    block, completion = specification['blocks'][0], protocol['blocks'][0]
    require(completion['block'] == block['block'] and completion['valid'] is True and {c['label'] for c in completion['cleanup']} == {'baseline', 'candidate'} and len(completion['cleanup']) == 2 and all(c['clean'] is True and c['forcedKill'] is False and c['launcherExitCode'] == 0 for c in completion['cleanup']), 'Smoke cleanup failure cannot satisfy timing prerequisites')
    smoke_path = Path(__file__).resolve().parent / 'harness/smoke.mjs'
    require(specification['harnessFiles'].get(str(smoke_path.relative_to(ROOT))) == digest(smoke_path) and Path(block['measureArgv'][1]).resolve() == smoke_path, 'Smoke validator implementation differs')
    for relative, expected in specification['harnessFiles'].items():
        checked_file(ROOT / relative, expected)
    require(Path(block['measureArgv'][-1]).resolve() == Path(manifest['realHostSmokeEvidence']).resolve(), 'Smoke protocol did not produce the registered evidence')
    enabled = next(p for p in manifest['profiles'] if p['name'] == 'i18n-enabled')
    require(result['isPerformanceRun'] is False and result['semanticValid'] is True and result['baselineRevision'] == manifest['baselineRevision'] and result['candidateRevision'] == manifest['candidateRevision'], 'Smoke did not use isolated source identities')
    require(result['profileSpecSha256'] == canonical_hash(enabled) and block['config']['profileSpec'] == enabled and result['seedFileManifestSha256'] == manifest['seedFileManifestSha256'], 'Smoke profile/dataset differs')
    observed = {entry['variant']: entry for entry in result['results']}
    require(set(observed) == {'baseline', 'candidate'} and len(result['results']) == 2, 'Smoke omitted an independent arm')
    for label, arm in (('baseline', 'A'), ('candidate', 'B')):
        entry = observed[label]
        build = specification['artifacts']['i18n-enabled'][arm]
        require(entry['revision'] == build['revision'] and entry['executableSha256'] == build['binarySha256'], 'Smoke binary identity differs')
        require(set(entry['contexts']) == {'guest', 'admin'} and entry['contexts']['guest']['kind'] == 'guest' and entry['contexts']['admin']['kind'] == 'admin-session' and all(context['semanticValid'] is True for context in entry['contexts'].values()), 'Smoke omitted real guest/admin semantic checks')
        variant = next(v for v in block['config']['variants'] if v['name'] == label)
        metadata = load(variant['metadata'])
        exit_record = load(variant['metadata'] + '.exit.json')
        require(metadata['declaredSourceRevision'] == build['revision'] and metadata['executableSha256'] == build['binarySha256'] and exit_record['code'] == 0 and exit_record['signal'] is None, 'Smoke child metadata/shutdown differs')
    require(len({entry['contexts']['guest']['origin'] for entry in observed.values()}) == 2, 'Smoke requires independent process origins')
    def utc(value):
        parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
        require(parsed.tzinfo is not None, 'Smoke timestamps need explicit timezone')
        return parsed.astimezone(timezone.utc)
    require(utc(protocol['startedAtUTC']) <= utc(result['completedAtUTC']) <= utc(protocol['endedAtUTC']), 'Smoke evidence escaped finalized executor interval')
    evidence = {'result': {'path': manifest['realHostSmokeEvidence'], 'sha256': manifest['realHostSmokeEvidenceSha256']}, 'protocol': {'path': manifest['realHostSmokeProtocol'], 'sha256': manifest['realHostSmokeProtocolSha256']}, 'plan': {'path': str(plan_path), 'sha256': digest(plan_path)}}
    return result, evidence


def external_packages(source):
    lock = tomllib.loads((Path(source) / 'Cargo.lock').read_text())
    return sorted((p for p in lock['package'] if 'source' in p), key=lambda p: (p['name'], p['version'], p['source']))


def record(path, revision, features):
    require(path, 'Fill all build-record paths first')
    r = load(path)
    require(r['rustcVV'] and r['rustcVV'].startswith('rustc 1.95.0 ') and r['cargoVV'] and r['cargoVV'].startswith('cargo 1.95.0 '), 'Use the pinned production Rust/Cargo 1.95 toolchain, not lint artifacts')
    require(r['outputEnvironment']['CARGO_TARGET_DIR'] and Path(r['outputEnvironment']['CARGO_TARGET_DIR']).resolve() == Path(r['targetDir']).resolve(), 'Build output directory is not bound to CARGO_TARGET_DIR')
    require(r['revision'] == revision, 'Build revision differs')
    require(r['features'] == features, 'Feature stratum differs')
    require(r['command'] == ['cargo', 'build', '--release', '--locked', '-p', 'rbit-server', *features], 'Build command differs')
    for name in ('sourceArchive', 'sourceFilesManifest', 'assetManifest', 'buildLog', 'configInputsRecord'):
        checked_file(r[name], r['archiveSha256'] if name == 'sourceArchive' else r[name + 'Sha256'])
    checked_file(r['executable'], r['binarySha256'])
    expected_executable = Path(r['targetDir']) / 'release' / Path(r['executable']).name
    require(expected_executable.resolve() == Path(r['executable']).resolve(), 'Executable not in selected target/release directory')
    require(Path(r['executable']).name in ('rbit', 'rbit.exe'), 'Unexpected server executable')
    checked_tree(r['sourceDir'], r['sourceFilesManifest'])
    checked_file(Path(r['sourceDir']) / 'Cargo.lock', r['cargoLockSha256'])
    checked_file(Path(r['sourceDir']) / 'package-lock.json', r['packageLockSha256'])
    for name, sha in load(r['assetManifest'])['files'].items():
        checked_file(Path(r['sourceDir']) / name, sha)
    require(r['effectiveReleaseProfile'] == {'lto': 'fat', 'codegen-units': 1, 'panic': 'abort', 'strip': 'symbols'}, 'Unexpected release profile')
    require(all(r[k] for k in ('rustcVV', 'cargoVV', 'target', 'buildEnvironment')), 'Missing compiler/environment evidence')
    return r


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest')
    parser.add_argument('--phase', choices=('smoke', 'pilot', 'confirmatory'), required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    path = Path(args.manifest).resolve()
    m = load(path)
    require(m['schema'] == 2, 'Unsupported manifest')
    require(m['baselineRevision'] == 'd3dc8980dd091c31ceffc61f916ca282b10ab506', 'Wrong isolated baseline')
    require(isinstance(m['candidateRevision'], str) and re.fullmatch('[0-9a-f]{40}', m['candidateRevision']), 'Pin exact clean candidate first')
    require(m['candidateRevision'] == 'b951eed02f7fb13994d013f71b9ce87206f59f41', 'Do not substitute a later Actions/docs head for isolated i18n B')
    require(m['candidateRevision'] != m['baselineRevision'], 'Identical revisions are not the extraction comparison')
    require(m['rounds'] == 100 and m['warmups'] == 20 and m['ports'] == [3045, 3046], 'Unexpected frozen sampling design')
    family(m)
    require(m['limits'] == {'readinessSeconds': 30, 'samplerSeconds': 120, 'shutdownSeconds': 30}, 'Changed reviewed executor deadlines')
    require(m['samplerPath'] and m['samplerCapabilitiesRecord'], 'Auth-capable sampler/executor evidence is required; the historical anonymous sampler cannot measure administrator inventory')
    sampler = Path(m['samplerPath']).resolve()
    capability = load(m['samplerCapabilitiesRecord'])
    require(isinstance(m['realHostSmokeEvidence'], str) and m['realHostSmokeEvidence'], 'Set a dedicated real-host smoke evidence path before planning')
    smoke, smoke_evidence = (None, None) if args.phase == 'smoke' else checked_smoke(m)
    require(capability['samplerSha256'] == digest(sampler) and capability['originScopedCookieContexts'] is True and capability['noRedirects'] is True and capability['redactedResults'] is True, 'Auth sampler capability evidence differs')
    require(all(p['runtimeEnv']['RBIT_ACTIONS__ENABLED'] == 'false' for p in m['profiles']), 'Actions must remain disabled')
    require(m['machineAndIdleConditionsRecord'] and m['semanticChecksRecord'], 'Missing idle-machine/semantic evidence')
    require(load(m['machineAndIdleConditionsRecord']) and load(m['semanticChecksRecord']), 'Empty evidence records')
    checked_file(m['seedFileManifest'], m['seedFileManifestSha256'])
    checked_tree(m['quiescentSeedDirectory'], m['seedFileManifest'])
    seed_names = load(m['seedFileManifest'])['files']
    require(not any(name.endswith(('-wal', '-shm', '-journal')) for name in seed_names), 'Seed contains SQLite sidecars: verify closed/checkpointed snapshot before proceeding')
    require((Path(m['fixtureDirectory']) / '.git').exists(), 'Sampler requires the deterministic fixture Git identity')
    freeze = None
    if args.phase == 'confirmatory':
        require(m['freezeRecord'], 'Freeze N before any confirmatory A/B samples')
        freeze = load(m['freezeRecord'])
        require(freeze['experimentManifestSha256'] == digest(path), 'Manifest changed after sample-size freeze')
        for name in ('pilotCompleteRecord', 'pilotAnalysis'):
            checked_file(freeze[name], freeze[name + 'Sha256'])
        require(freeze['planImplementationSha256'] == digest(__file__), 'Planner changed after freeze')
        require(freeze['analysisImplementationSha256'] and freeze['frozenAtUTC'], 'Missing frozen analysis provenance')
        require(freeze['selectionUsesOnlySameBinaryPilot'] is True and freeze['confirmatoryEffectNotInspected'] is True, 'Invalid sample-size selection')
    output = Path(args.output).resolve()
    require(not output.exists(), 'Refuse to overwrite an existing run plan')
    plans = []
    artifacts = {}
    for profile in m['profiles']:
        if args.phase == 'smoke' and profile['name'] != 'i18n-enabled':
            continue
        builds = {arm: record(profile['artifacts'][arm], m['baselineRevision' if arm == 'A' else 'candidateRevision'], profile['features']) for arm in ('A', 'B')}
        for key in ('rustcVV', 'cargoVV', 'target', 'buildEnvironment', 'effectiveReleaseProfile', 'packageLockSha256', 'assetManifestSha256', 'configInputsRecordSha256'):
            require(builds['A'][key] == builds['B'][key], f'{profile["name"]}: A/B {key} differs')
        require(external_packages(builds['A']['sourceDir']) == external_packages(builds['B']['sourceDir']), 'External Cargo.lock package records drifted')
        require(builds['A']['targetDir'] != builds['B']['targetDir'] and builds['A']['sourceDir'] != builds['B']['sourceDir'], 'Build/source directories must be independent')
        for arm, build in builds.items():
            require(build['archiveSha256'] == m['sourceArchives'][arm]['archive_sha256'] and build['cargoLockSha256'] == m['sourceArchives'][arm]['lockfile_sha256'], 'Build record is not the pinned isolated archive/lock')
        if profile['name'] == 'i18n-enabled' and smoke is not None:
            require(smoke['profileSpecSha256'] == canonical_hash(profile) and smoke['seedFileManifestSha256'] == m['seedFileManifestSha256'], 'Smoke did not exercise the registered enabled profile/dataset')
            require(len({entry['contexts']['guest']['origin'] for entry in smoke['results']}) == 2 and all(set(entry['contexts']) == {'guest', 'admin'} and entry['contexts']['guest']['kind'] == 'guest' and entry['contexts']['admin']['kind'] == 'admin-session' and entry['contexts']['guest']['semanticValid'] is True and entry['contexts']['admin']['semanticValid'] is True for entry in smoke['results']), 'Smoke omitted actual guest/admin semantic checks')
            by_variant = {entry['variant']: entry for entry in smoke['results']}
            require(set(by_variant) == {'baseline', 'candidate'} and all(by_variant[label]['executableSha256'] == builds[arm]['binarySha256'] and by_variant[label]['revision'] == builds[arm]['revision'] for label, arm in (('baseline', 'A'), ('candidate', 'B'))), 'Enabled real-host smoke used different release binaries')
        artifacts[profile['name']] = builds
        n = None if args.phase in ('smoke', 'pilot') else freeze['selectedPairs'][profile['name']]
        schedule = phase_schedule(args.phase, n)
        phase_counts = {}
        for i, (phase, left, right) in enumerate(schedule, 1):
            phase_counts[phase] = phase_counts.get(phase, 0) + 1
            stem = f'{profile["name"]}-{i:03}-{phase}'
            directory = output / stem
            arms = [('baseline', left), ('candidate' if phase in ('comparison', 'smoke') else 'control', right)]
            launch_order = arms if phase_counts[phase] % 2 else list(reversed(arms))
            commands, variants = [], {}
            for port, (label, build_arm) in zip(m['ports'], launch_order):
                b = builds[build_arm]
                metadata = directory / f'{label}.server.json'
                data = directory / f'{label}.data'
                commands.append({'label': label, 'artifactArm': build_arm, 'seedCopy': {'source': m['quiescentSeedDirectory'], 'destination': str(data), 'verifyCopiedManifest': m['seedFileManifest'], 'verificationRecord': str(metadata) + '.seed-before-start.json'}, 'argv': ['node', 'scripts/perf/serve.mjs', '--profile', 'release', '--port', str(port), '--target-dir', b['targetDir'], '--data-dir', str(data), '--metadata', str(metadata), '--source-dir', b['sourceDir'], '--source-revision', b['revision']], 'cwd': str(ROOT), 'envOverrides': profile['runtimeEnv'], 'stripInheritedEnvironmentPrefix': 'RBIT_'})
                variants[label] = {'name': label, 'baseUrl': f'http://127.0.0.1:{port}', 'metadata': str(metadata)}
            config = {'output': str(directory / 'samples.json'), 'fixture': m['fixtureDirectory'], 'notes': f'Frozen isolated i18n process-pair experiment; {profile["name"]}; {phase}; warm OS caches; no execution/performance claim before analysis', 'rounds': 100, 'warmups': 20, 'variants': [variants[label] for label, _ in arms], 'pages': profile['pages'], 'requestContexts': profile['requestContexts'], 'profileSpec': profile, 'seedFileManifestSha256': m['seedFileManifestSha256']}
            plans.append({'block': stem, 'phase': phase, 'profile': profile['name'], 'commands': commands, 'config': config, 'measureArgv': (['node', str(sampler.parent / 'smoke.mjs'), str(directory / 'measure.config.json'), m['realHostSmokeEvidence']] if phase == 'smoke' else ['node', str(sampler), str(directory / 'measure.config.json')]), 'cwd': str(ROOT), 'requiredReadiness': 'both /healthz 200; bounded wait; no measurements while compiling', 'requiredFinalization': 'SIGTERM both; bounded cleanup; require launcher exit records code=0 and signal=null; preserve every failure'})
    output.mkdir(parents=True)
    for block in plans:
        directory = output / block['block']
        directory.mkdir()
        (directory / 'measure.config.json').write_text(json.dumps(block['config'], indent=2) + '\n')
    evidence = {'schema': 2, 'status': 'PLAN_ONLY_NOT_EXECUTED', 'manifestSha256': digest(path), 'planScriptSha256': digest(__file__), 'phase': args.phase, 'smokeEvidence': smoke_evidence, 'inputEvidence': {key: {'path': m[key], 'sha256': digest(m[key])} for key in ('semanticChecksRecord', 'machineAndIdleConditionsRecord')}, 'harnessFiles': {name: digest(ROOT / name) for name in ('scripts/perf/serve.mjs', 'scripts/perf/support.mjs', 'scripts/perf/fixture.mjs', 'scripts/perf/seed.mjs', str((Path(__file__).parent / 'execute.py').relative_to(ROOT)), str(sampler.relative_to(ROOT)), str((sampler.parent / 'contexts.mjs').relative_to(ROOT)), str((Path(__file__).parent / 'harness/smoke.mjs').relative_to(ROOT)), str((Path(__file__).parent / 'harness/seed-i18n.mjs').relative_to(ROOT)))}, 'artifacts': artifacts, 'blocks': plans}
    (output / 'commands.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(f'Wrote {len(plans)} process-pair command plans; no processes launched.')


if __name__ == '__main__':
    main()
