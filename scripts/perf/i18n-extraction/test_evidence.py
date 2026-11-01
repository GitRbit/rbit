"""Synthetic files in the existing sampler/launcher schema; never execute a binary."""
import copy
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import analysis as a
import plan as p


class SamplerEvidence(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest_path = self.root / 'experiment.json'
        self.plan_path = self.root / 'commands.json'
        template = Path(__file__).with_name('experiment.template.json')
        self.manifest = a.load(template)
        self.manifest['candidateRevision'] = 'b951eed02f7fb13994d013f71b9ce87206f59f41'
        self.manifest['samplerPath'] = str(p.ROOT / 'scripts/perf/i18n-extraction/harness/measure.mjs')
        input_evidence = {}
        for role in ('semanticChecksRecord', 'machineAndIdleConditionsRecord'):
            self.manifest[role] = str(self.root / (role + '.json'))
            self.write(self.manifest[role], {'synthetic': True})
            input_evidence[role] = {'path': self.manifest[role], 'sha256': a.sha(self.manifest[role])}
        self.manifest['seedFileManifest'] = str(self.root / 'seed.json')
        self.write(self.manifest['seedFileManifest'], {'files': {'database.sqlite': '0' * 64}})
        self.manifest['seedFileManifestSha256'] = a.sha(self.manifest['seedFileManifest'])
        executable = self.root / 'rbit'
        executable.write_bytes(b'SYNTHETIC DATA; never executed')
        self.builds = {}
        for profile in self.manifest['profiles']:
            for arm in ('A', 'B'):
                profile['artifacts'][arm] = profile['name'] + arm
            self.builds[profile['name']] = {
                arm: {'archiveSha256': self.manifest['sourceArchives'][arm]['archive_sha256'], 'cargoLockSha256': self.manifest['sourceArchives'][arm]['lockfile_sha256'], 'revision': self.manifest['baselineRevision' if arm == 'A' else 'candidateRevision'], 'binarySha256': a.sha(executable), 'sourceDir': str(self.root / ('source-' + arm)),
                      **{key: 'synthetic' for key in ('rustcVV', 'cargoVV', 'target', 'buildEnvironment', 'effectiveReleaseProfile', 'packageLockSha256', 'assetManifestSha256', 'configInputsRecordSha256')}} for arm in ('A', 'B')}
        self.write(self.manifest_path, self.manifest)
        profile = self.manifest['profiles'][0]['name']
        self.config = {'output': str(self.root / 'samples.json'), 'fixture': str(self.root / 'fixture'), 'notes': 'synthetic producer-schema test', 'rounds': 100, 'warmups': 20, 'pages': self.manifest['profiles'][0]['pages'], 'requestContexts': self.manifest['profiles'][0]['requestContexts'], 'variants': []}
        servers = []
        for i, label in enumerate(('baseline', 'control')):
            metadata = self.root / f'{label}.server.json'
            url = f'http://127.0.0.1:{3045+i}'
            self.config['variants'].append({'name': label, 'baseUrl': url, 'metadata': str(metadata)})
            server = {'baseUrl': url, 'profile': 'release', 'declaredSourceRevision': self.manifest['baselineRevision'], 'executableSha256': a.sha(executable), 'executable': str(executable), 'sourceDir': self.builds[profile]['A']['sourceDir'], 'dataDir': str(self.root / f'{label}.data'), 'port': 3045+i, 'startedAt': f'2020-01-01T10:00:0{i+1}Z'}
            self.write(metadata, server)
            self.write(str(metadata) + '.exit.json', {'code': 0, 'signal': None, 'endedAt': '2020-01-01T10:00:03Z'})
            Path(str(metadata) + '.log').write_text('synthetic server log; no process ran')
            self.write(str(metadata) + '.seed-before-start.json', a.load(self.manifest['seedFileManifest']))
            servers.append(server)
        harness = {name: a.sha(p.ROOT / name) for name in ('scripts/perf/i18n-extraction/harness/measure.mjs', 'scripts/perf/support.mjs', 'scripts/perf/serve.mjs', 'scripts/perf/i18n-extraction/harness/contexts.mjs')}
        config_path = self.root / 'measure.config.json'
        self.write(config_path, self.config)
        self.plan = {'schema': 2, 'smokeEvidence': {'synthetic': True}, 'inputEvidence': input_evidence, 'planScriptSha256': a.sha(p.__file__), 'phase': 'pilot', 'manifestSha256': a.sha(self.manifest_path), 'artifacts': self.builds, 'harnessFiles': harness, 'blocks': [{'block': 'synthetic-first-pilot-pair', 'phase': 'pilot-AA', 'profile': profile, 'config': self.config, 'measureArgv': ['node', 'scripts/perf/measure.mjs', str(config_path)], 'commands': [{'label': label, 'artifactArm': 'A'} for label in ('baseline', 'control')]}]}
        self.save_plan()
        self.report = {'startedAt': '2020-01-01T10:00:02.100Z', 'endedAt': '2020-01-01T10:00:02.900Z', 'complete': True, 'valid': True, 'config': self.config, 'servers': servers, 'fixture': {'revision': 'f'*40, 'status': ''}, 'machine': {'platform': 'synthetic'}, 'bootstrap': [{'variant': v['name'], 'contexts': {'anonymous': {'kind': 'anonymous', 'origin': v['baseUrl'], 'cookieFingerprint': None}}} for v in self.config['variants']], 'harness': {'contextsSha256': harness['scripts/perf/i18n-extraction/harness/contexts.mjs'], 'measureSha256': harness['scripts/perf/i18n-extraction/harness/measure.mjs'], 'supportSha256': harness['scripts/perf/support.mjs']}, 'samples': []}
        for phase, rounds in (('warmup', 20), ('measured', 100)):
            for round_ in range(1, rounds + 1):
                for page in self.config['pages']:
                    variants = self.config['variants'] if round_ % 2 else list(reversed(self.config['variants']))
                    for variant in variants:
                        self.report['samples'].append({'startedAt': '2020-01-01T10:00:02.200Z', 'requestContext': page['context'], 'cookieFingerprint': None, 'phase': phase, 'page': page['name'], 'round': round_, 'variant': variant['name'], 'valid': True, 'errors': [], 'status': 200, 'url': variant['baseUrl'] + page['path'], 'requestHeaders': {'Accept-Encoding': 'identity'}, 'responseHeaders': {}, 'totalMs': 1.0, 'headersMs': .8, 'renderMs': .2})
        self.report['postflight'] = self.postflight(self.config, self.report['bootstrap'], self.report['endedAt'])
        self.save_report()
        # This fixture isolates validation of actual producer file formats. Build
        # source-tree validation is the planner's separate preflight, not faked as
        # an actual compiler invocation or an independent provenance attestation.
        def build_record(path, revision, features):
            return next(b for profile in self.manifest['profiles'] for arm, b in self.builds[profile['name']].items() if path == profile['artifacts'][arm])
        self.addCleanup(patch.stopall)
        patch.object(a.experiment_plan, 'record', side_effect=build_record).start()
        patch.object(a.experiment_plan, 'external_packages', return_value=[]).start()
        patch.object(a.experiment_plan, 'checked_smoke', return_value=({}, {'synthetic': True})).start()

    @staticmethod
    def write(path, value):
        Path(path).write_text(json.dumps(value) + '\n')

    @staticmethod
    def postflight(config, bootstrap, at):
        contexts = {entry['variant']: entry['contexts'] for entry in bootstrap}
        return [{'variant': variant['name'], 'pages': [{'page': page['name'], 'path': page['path'], 'context': page['context'], 'origin': variant['baseUrl'], 'cookieFingerprint': contexts[variant['name']][page['context']]['cookieFingerprint'], 'valid': True, 'contentLanguage': page.get('contentLanguage'), 'bodySha256': '2'*64, 'atUTC': at} for page in config['pages']]} for variant in config['variants']]

    def save_plan(self):
        self.write(self.plan_path, self.plan)
        protocol = {'schema': 2, 'phase': self.plan['phase'], 'complete': True, 'valid': True, 'planSha256': a.sha(self.plan_path), 'executorSha256': a.sha(Path(a.__file__).with_name('execute.py')), 'startedAtUTC': '2020-01-01T00:00:00Z', 'endedAtUTC': '2020-01-02T00:00:00Z', 'blocks': [{'block': block['block'], 'valid': True, 'cleanup': [{'label': command['label'], 'clean': True, 'forcedKill': False, 'launcherExitCode': 0} for command in block['commands']]} for block in self.plan['blocks']]}
        self.write(self.root / 'protocol.json', protocol)

    def save_report(self):
        self.write(self.config['output'], self.report)
        metadata = {'type': 'metadata', **{k: v for k, v in self.report.items() if k not in ('samples', 'complete', 'valid', 'endedAt', 'postflight')}}
        with Path(self.config['output'] + '.jsonl').open('w') as f:
            f.write(json.dumps(metadata) + '\n')
            for sample in self.report['samples']:
                f.write(json.dumps(sample) + '\n')

    def analyze(self):
        return a.load_run(self.plan_path, self.manifest_path, 'pilot')

    def complete_pilot(self):
        blocks = []
        serial = 0
        original = copy.deepcopy(self.report)
        for profile in self.manifest['profiles']:
            for pair in range(1, 13):
                for phase, build_arm in (('pilot-AA', 'A'), ('pilot-BB', 'B')):
                    serial += 1
                    directory = self.root / f'pair-{serial}'
                    directory.mkdir()
                    config = copy.deepcopy(self.config)
                    config['output'] = str(directory / 'samples.json')
                    config['pages'] = profile['pages']
                    config['requestContexts'] = profile['requestContexts']
                    config['variants'] = []
                    servers = []
                    for label in ('baseline', 'control'):
                        port = (3045 if pair % 2 else 3046) if label == 'baseline' else (3046 if pair % 2 else 3045)
                        metadata = directory / f'{label}.server.json'
                        url = f'http://127.0.0.1:{port}'
                        config['variants'].append({'name': label, 'baseUrl': url, 'metadata': str(metadata)})
                        server = copy.deepcopy(original['servers'][0])
                        server.update({'baseUrl': url, 'port': port, 'dataDir': str(directory / (label + '.data')), 'declaredSourceRevision': self.manifest['baselineRevision' if build_arm == 'A' else 'candidateRevision'], 'sourceDir': self.builds[profile['name']][build_arm]['sourceDir']})
                        start = datetime(2020, 1, 1, tzinfo=timezone.utc) + timedelta(seconds=serial * 10 + port - 3044)
                        server['startedAt'] = start.isoformat()
                        self.write(metadata, server)
                        self.write(str(metadata) + '.exit.json', {'code': 0, 'signal': None, 'endedAt': (start + timedelta(seconds=3)).isoformat()})
                        self.write(str(metadata) + '.seed-before-start.json', a.load(self.manifest['seedFileManifest']))
                        Path(str(metadata) + '.log').write_text('synthetic only')
                        servers.append(server)
                    report = copy.deepcopy(original)
                    sampler_start = datetime(2020, 1, 1, tzinfo=timezone.utc) + timedelta(seconds=serial * 10 + 2.1)
                    bootstrap = [{'variant': variant['name'], 'contexts': {key: {'kind': spec['kind'], 'origin': variant['baseUrl'], 'cookieFingerprint': None if spec['kind'] == 'anonymous' else '1'*64, 'semanticValid': True, 'atUTC': sampler_start.isoformat()} for key, spec in config['requestContexts'].items()}} for variant in config['variants']]
                    report.update({'bootstrap': bootstrap, 'config': config, 'servers': servers, 'startedAt': sampler_start.isoformat(), 'endedAt': (sampler_start + timedelta(seconds=.8)).isoformat(), 'samples': []})
                    for sample_phase, rounds in (('warmup', 20), ('measured', 100)):
                        for round_ in range(1, rounds+1):
                            for page in config['pages']:
                                for variant in (config['variants'] if round_ % 2 else list(reversed(config['variants']))):
                                    sample = copy.deepcopy(original['samples'][0])
                                    sample.update({'startedAt': (sampler_start + timedelta(seconds=.1)).isoformat(), 'phase': sample_phase, 'round': round_, 'page': page['name'], 'variant': variant['name'], 'url': variant['baseUrl']+page['path']})
                                    spec = config['requestContexts'][page['context']]
                                    sample['requestContext'] = page['context']
                                    sample['cookieFingerprint'] = None if spec['kind'] == 'anonymous' else '1'*64
                                    sample['requestHeaders'] = {'Accept-Encoding': 'identity', **({} if spec['kind'] == 'anonymous' else {'Cookie': '[redacted]'})}
                                    sample['responseHeaders'] = {} if not page.get('contentLanguage') else {'content-language': page['contentLanguage']}
                                    report['samples'].append(sample)
                    report['postflight'] = self.postflight(config, bootstrap, report['endedAt'])
                    self.write(config['output'], report)
                    with Path(config['output'] + '.jsonl').open('w') as f:
                        f.write(json.dumps({'type': 'metadata', **{k: v for k, v in report.items() if k not in ('samples', 'complete', 'valid', 'endedAt', 'postflight')}}) + '\n')
                        for sample in report['samples']:
                            f.write(json.dumps(sample) + '\n')
                    config_path = directory / 'measure.config.json'
                    self.write(config_path, config)
                    blocks.append({'block': f'pair-{serial}', 'phase': phase, 'profile': profile['name'], 'config': config, 'measureArgv': ['node', 'scripts/perf/measure.mjs', str(config_path)], 'commands': [{'label': label, 'artifactArm': build_arm} for label in ('baseline', 'control')]})
        self.plan['blocks'] = blocks
        self.save_plan()

    def test_complete_actual_schema_pilot_loads_all_cells_and_freezes_without_ab_effects(self):
        self.manifest['freezeRecord'] = str(self.root / 'freeze.json')
        self.write(self.manifest_path, self.manifest)
        self.plan['manifestSha256'] = a.sha(self.manifest_path)
        self.complete_pilot()
        output = self.root / 'pilot-analysis.json'
        argv = ['analysis.py', 'pilot', str(self.plan_path), '--manifest', str(self.manifest_path), '--output', str(output), '--freeze-output', self.manifest['freezeRecord'], '--engine', 'python']
        with patch.object(sys, 'argv', argv):
            a.main()
        result, freeze = a.load(output), a.load(self.manifest['freezeRecord'])
        self.assertTrue(result['complete'] and result['valid'])
        self.assertEqual(len(result['provenance']['blocks']), 72)
        self.assertEqual(result['selectedPairs'], {'default-disabled': 80, 'core-only': 80, 'i18n-enabled': 80})
        self.assertNotIn('comparison', result)
        self.assertEqual(freeze['selectedPairs'], result['selectedPairs'])
        self.assertEqual(freeze['analysisImplementationSha256'], a.sha(a.__file__))
        self.assertEqual(freeze['pilotAnalysisSha256'], a.sha(output))

    def test_valid_first_pair_cannot_stand_in_for_complete_pilot_or_primary_family(self):
        with self.assertRaisesRegex(ValueError, 'Changed phase/profile ordering or incomplete run'):
            self.analyze()

    def test_transport_failure_invalidates_attempt_without_mutating_raw_evidence(self):
        self.report['samples'][100].update({'valid': False, 'status': 500, 'errors': ['synthetic transport error']})
        self.save_report()
        before = a.sha(self.config['output'])
        with self.assertRaisesRegex(ValueError, 'transport/representation failure'):
            self.analyze()
        self.assertEqual(before, a.sha(self.config['output']))

    def test_jsonl_disagreement_is_rejected_even_when_summary_claims_valid(self):
        with Path(self.config['output'] + '.jsonl').open('a') as f:
            f.write(json.dumps(self.report['samples'][0]) + '\n')
        with self.assertRaisesRegex(ValueError, 'JSONL differs'):
            self.analyze()

    def test_duplicate_or_missing_round_and_changed_request_order_are_protocol_failures(self):
        original = copy.deepcopy(self.report)
        mutations = [('duplicate sample', lambda rows: rows.append(rows[0])), ('incomplete paired matrix', lambda rows: rows.pop()), ('paired request order', lambda rows: rows.__setitem__(slice(0, 2), list(reversed(rows[:2]))))]
        for expected, mutate in mutations:
            with self.subTest(expected=expected):
                self.report = copy.deepcopy(original)
                mutate(self.report['samples'])
                self.save_report()
                with self.assertRaisesRegex(ValueError, expected):
                    self.analyze()

    def test_manual_prerequisite_records_cannot_change_after_registration(self):
        Path(self.manifest['semanticChecksRecord']).write_text('{"changed":true}')
        with self.assertRaisesRegex(ValueError, 'evidence changed after planning'):
            self.analyze()

    def test_sampler_and_request_timestamps_must_fall_inside_live_process_pair(self):
        self.report['endedAt'] = '2020-01-01T10:00:04Z'
        self.save_report()
        with self.assertRaisesRegex(ValueError, 'sampler outside live process-pair'):
            self.analyze()
        self.report['endedAt'] = '2020-01-01T10:00:02.900Z'
        self.report['samples'][0]['startedAt'] = '2020-01-01T10:00:00Z'
        self.save_report()
        with self.assertRaisesRegex(ValueError, 'sample outside monotone sampler'):
            self.analyze()

    def test_executor_failure_cannot_be_hidden_by_clean_child_exit_records(self):
        protocol_file = self.root / 'protocol.json'
        original = a.load(protocol_file)
        for change in ('invalid', 'forced', 'launcher'):
            protocol = copy.deepcopy(original)
            if change == 'invalid': protocol['valid'] = False
            if change == 'forced': protocol['blocks'][0]['cleanup'][0]['forcedKill'] = True
            if change == 'launcher': protocol['blocks'][0]['cleanup'][0]['launcherExitCode'] = 9
            self.write(protocol_file, protocol)
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, 'incomplete/invalid|cleanup was not clean'):
                self.analyze()

    def test_same_binary_control_cannot_be_silently_replaced_by_candidate(self):
        self.plan['blocks'][0]['commands'][1]['artifactArm'] = 'B'
        self.save_plan()
        with self.assertRaisesRegex(ValueError, 'same-binary/control identity'):
            self.analyze()

    def test_bad_shutdown_or_unverified_seed_prevents_analysis(self):
        metadata = self.config['variants'][0]['metadata']
        self.write(metadata + '.exit.json', {'code': 1, 'signal': None})
        with self.assertRaisesRegex(ValueError, 'unclean shutdown'):
            self.analyze()
        self.write(metadata + '.exit.json', {'code': 0, 'signal': None, 'endedAt': '2020-01-01T10:00:03Z'})
        self.write(metadata + '.seed-before-start.json', {'files': {}})
        with self.assertRaisesRegex(ValueError, 'pre-start copied seed differs'):
            self.analyze()


if __name__ == '__main__':
    unittest.main()
