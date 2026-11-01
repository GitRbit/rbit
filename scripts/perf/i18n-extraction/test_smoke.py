"""Finalized smoke artifact linkage only; synthetic JSON is not a real-host proof."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parent))
import plan


class FinalizedSmoke(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(); self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.manifest = plan.load(Path(__file__).with_name('experiment.template.json'))
        self.manifest['seedFileManifestSha256'] = '1'*64
        self.manifest['realHostSmokeEvidence'] = str(self.root / 'smoke.json')
        self.manifest['realHostSmokeProtocol'] = str(self.root / 'protocol.json')
        enabled = self.manifest['profiles'][-1]
        self.builds = {arm: {'revision': self.manifest['baselineRevision' if arm == 'A' else 'candidateRevision'], 'binarySha256': ('a' if arm == 'A' else 'b')*64} for arm in ('A', 'B')}
        variants, results = [], []
        for label, arm, port in [('baseline', 'A', 3045), ('candidate', 'B', 3046)]:
            metadata = str(self.root / (label + '.server.json'))
            self.write(metadata, {'declaredSourceRevision': self.builds[arm]['revision'], 'executableSha256': self.builds[arm]['binarySha256']})
            self.write(metadata + '.exit.json', {'code': 0, 'signal': None})
            variants.append({'name': label, 'baseUrl': f'http://127.0.0.1:{port}', 'metadata': metadata})
            results.append({'variant': label, **{'revision': self.builds[arm]['revision'], 'executableSha256': self.builds[arm]['binarySha256']}, 'contexts': {name: {'kind': kind, 'origin': f'http://127.0.0.1:{port}', 'semanticValid': True} for name, kind in [('guest', 'guest'), ('admin', 'admin-session')]}})
        self.spec = {'schema': 2, 'phase': 'smoke', 'planScriptSha256': plan.digest(plan.__file__), 'harnessFiles': {str((Path(plan.__file__).resolve().parent / 'harness/smoke.mjs').relative_to(plan.ROOT)): plan.digest(Path(plan.__file__).parent / 'harness/smoke.mjs')}, 'artifacts': {'i18n-enabled': self.builds}, 'blocks': [{'block': 'owned-smoke', 'measureArgv': ['node', str(Path(plan.__file__).parent / 'harness/smoke.mjs'), 'config.json', self.manifest['realHostSmokeEvidence']], 'config': {'profileSpec': enabled, 'variants': variants}}]}
        self.write(self.root / 'commands.json', self.spec)
        self.protocol = {'schema': 2, 'phase': 'smoke', 'complete': True, 'valid': True, 'planSha256': plan.digest(self.root / 'commands.json'), 'executorSha256': plan.digest(Path(plan.__file__).with_name('execute.py')), 'startedAtUTC': '2020-01-01T00:00:00Z', 'endedAtUTC': '2020-01-01T00:02:00Z', 'blocks': [{'block': 'owned-smoke', 'valid': True, 'cleanup': [{'label': label, 'clean': True, 'forcedKill': False, 'launcherExitCode': 0} for label in ['baseline', 'candidate']]}]}
        self.result = {'isPerformanceRun': False, 'semanticValid': True, 'baselineRevision': self.manifest['baselineRevision'], 'candidateRevision': self.manifest['candidateRevision'], 'profileSpecSha256': plan.canonical_hash(enabled), 'seedFileManifestSha256': '1'*64, 'results': results, 'completedAtUTC': '2020-01-01T00:01:00Z'}
        self.save()

    @staticmethod
    def write(path, data): Path(path).write_text(json.dumps(data) + '\n')
    def save(self):
        self.write(self.manifest['realHostSmokeEvidence'], self.result)
        self.write(self.manifest['realHostSmokeProtocol'], self.protocol)
        self.manifest['realHostSmokeEvidenceSha256'] = plan.digest(self.manifest['realHostSmokeEvidence'])
        self.manifest['realHostSmokeProtocolSha256'] = plan.digest(self.manifest['realHostSmokeProtocol'])

    def test_complete_clean_smoke_links_exact_profile_seed_binaries_and_finalized_protocol(self):
        result, evidence = plan.checked_smoke(self.manifest)
        self.assertFalse(result['isPerformanceRun'])
        self.assertEqual(set(evidence), {'result', 'protocol', 'plan'})
    def test_changed_smoke_validator_is_not_accepted_by_rehashing_result_or_protocol(self):
        self.spec['harnessFiles'][next(iter(self.spec['harnessFiles']))] = '0'*64
        self.write(self.root / 'commands.json', self.spec)
        self.protocol['planSha256'] = plan.digest(self.root / 'commands.json')
        self.save()
        with self.assertRaisesRegex(ValueError, 'validator implementation'):
            plan.checked_smoke(self.manifest)

    def test_functional_success_does_not_hide_forced_or_failed_cleanup(self):
        self.protocol['blocks'][0]['cleanup'][0]['forcedKill'] = True
        self.save()
        with self.assertRaisesRegex(ValueError, 'cleanup failure'):
            plan.checked_smoke(self.manifest)
    def test_anonymous_profile_or_missing_admin_cannot_satisfy_enabled_prerequisite(self):
        original = copy.deepcopy(self.result)
        self.result['profileSpecSha256'] = plan.canonical_hash(self.manifest['profiles'][0])
        self.save()
        with self.assertRaisesRegex(ValueError, 'profile/dataset differs'):
            plan.checked_smoke(self.manifest)
        self.result = original
        del self.result['results'][0]['contexts']['admin']
        self.save()
        with self.assertRaisesRegex(ValueError, 'guest/admin semantic'):
            plan.checked_smoke(self.manifest)


if __name__ == '__main__': unittest.main()
