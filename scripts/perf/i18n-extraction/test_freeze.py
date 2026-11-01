"""A confirmatory run cannot rewrite sample size, analysis or pilot provenance."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import analysis as a


class FreezeContracts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.manifest_path = self.root / 'experiment.json'
        self.pilot_path = self.root / 'pilot-analysis.json'
        self.freeze_path = self.root / 'freeze.json'
        self.manifest = {'freezeRecord': str(self.freeze_path), 'profiles': [{'name': 'default-disabled'}, {'name': 'core-only'}]}
        self.write(self.manifest_path, self.manifest)
        self.pilot = {'analysisFinishedAtUTC': '2020-01-01T00:00:30Z', 'phase': 'pilot', 'complete': True, 'valid': True, 'selectedPairs': {'default-disabled': 80, 'core-only': 160}, 'provenance': {'processesFinalizedAtUTC': '2020-01-01T00:00:20Z', 'manifestSha256': a.sha(self.manifest_path), 'analysisSha256': a.sha(a.__file__), 'planImplementationSha256': a.sha(a.experiment_plan.__file__)}}
        self.write(self.pilot_path, self.pilot)
        self.freeze = {'schema': 2, 'frozenAtUTC': '2020-01-01T00:01:00Z', 'experimentManifestSha256': a.sha(self.manifest_path), 'analysisImplementationSha256': a.sha(a.__file__), 'planImplementationSha256': a.sha(a.experiment_plan.__file__), 'selectionUsesOnlySameBinaryPilot': True, 'confirmatoryEffectNotInspected': True, 'pilotAnalysis': str(self.pilot_path), 'pilotAnalysisSha256': a.sha(self.pilot_path), 'pilotCompleteRecord': str(self.pilot_path), 'pilotCompleteRecordSha256': a.sha(self.pilot_path), 'selectedPairs': self.pilot['selectedPairs']}
        self.write(self.freeze_path, self.freeze)

    @staticmethod
    def write(path, value):
        Path(path).write_text(json.dumps(value) + '\n')

    def refresh_pilot_hashes(self):
        self.write(self.pilot_path, self.pilot)
        self.freeze['pilotAnalysisSha256'] = self.freeze['pilotCompleteRecordSha256'] = a.sha(self.pilot_path)
        self.write(self.freeze_path, self.freeze)

    def test_valid_pilot_preserves_distinct_profile_sample_sizes(self):
        selected, pilot, frozen_at, evidence = a.frozen_selection(self.manifest, self.manifest_path)
        self.assertEqual(selected, {'default-disabled': 80, 'core-only': 160})
        self.assertEqual(pilot['phase'], 'pilot')

    def test_no_refreeze_after_effect_inspection_or_sample_size_change(self):
        original = copy.deepcopy(self.freeze)
        for key, value, message in [('confirmatoryEffectNotInspected', False, 'Invalid freeze'), ('selectedPairs', {'default-disabled': 320, 'core-only': 160}, 'N differs')]:
            with self.subTest(key=key):
                self.freeze = copy.deepcopy(original)
                self.freeze[key] = value
                self.write(self.freeze_path, self.freeze)
                with self.assertRaisesRegex(ValueError, message):
                    a.frozen_selection(self.manifest, self.manifest_path)

    def test_freeze_chronology_binds_pilot_finalization_and_confirmation_start(self):
        _, _, frozen_at, _ = a.frozen_selection(self.manifest, self.manifest_path)
        with self.assertRaisesRegex(ValueError, 'started before sample-size freeze'):
            a.confirmation_after_freeze(a.utc_time('2020-01-01T00:00:59Z', 'test'), frozen_at)
        a.confirmation_after_freeze(a.utc_time('2020-01-01T00:01:00Z', 'test'), frozen_at)
        self.freeze['frozenAtUTC'] = '2020-01-01T00:00:19Z'
        self.write(self.freeze_path, self.freeze)
        with self.assertRaisesRegex(ValueError, 'predates complete pilot'):
            a.frozen_selection(self.manifest, self.manifest_path)
        self.freeze['frozenAtUTC'] = '2020-01-01T00:00:25Z'
        self.write(self.freeze_path, self.freeze)
        with self.assertRaisesRegex(ValueError, 'predates pilot analysis'):
            a.frozen_selection(self.manifest, self.manifest_path)
        self.freeze['frozenAtUTC'] = '2020-01-01T00:01:00'
        self.write(self.freeze_path, self.freeze)
        with self.assertRaisesRegex(ValueError, 'must include timezone'):
            a.frozen_selection(self.manifest, self.manifest_path)

    def test_changed_pilot_bytes_or_algorithm_cannot_reuse_freeze(self):
        self.pilot_path.write_text(self.pilot_path.read_text() + ' ')
        with self.assertRaisesRegex(ValueError, 'Changed pilot evidence'):
            a.frozen_selection(self.manifest, self.manifest_path)
        self.refresh_pilot_hashes()
        self.freeze['analysisImplementationSha256'] = '0' * 64
        self.write(self.freeze_path, self.freeze)
        with self.assertRaisesRegex(ValueError, 'Changed frozen manifest/analysis'):
            a.frozen_selection(self.manifest, self.manifest_path)

    def test_ab_data_incomplete_pilot_and_over_cap_n_cannot_select_confirmation_size(self):
        for change, message in [({'phase': 'confirmatory'}, 'non-pilot'), ({'complete': False}, 'Incomplete'), ({'selectedPairs': {'default-disabled': 640, 'core-only': 160}}, 'outside frozen cap')]:
            with self.subTest(change=change):
                original = copy.deepcopy(self.pilot)
                self.pilot.update(change)
                self.freeze['selectedPairs'] = self.pilot['selectedPairs']
                self.refresh_pilot_hashes()
                with self.assertRaisesRegex(ValueError, message):
                    a.frozen_selection(self.manifest, self.manifest_path)
                self.pilot = original


if __name__ == '__main__':
    unittest.main()
