"""Synthetic contract tests only: no servers, builds, real perf data or timings."""
from dataclasses import replace
import importlib.util
import math
from pathlib import Path
import random
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import analysis as a


def cell(left, right, profile='default-disabled', page='home', path='/'):
    return a.Cell(profile, page, path, tuple(f'process-{i}' for i in range(len(left))), tuple(tuple(x) for x in left), tuple(tuple(x) for x in right))


class StatisticalContracts(unittest.TestCase):
    def test_weighted_quantile_matches_explicit_multiset_including_zero_weight_and_interpolation(self):
        rng = random.Random(71)
        for _ in range(80):
            rows = [(rng.randrange(1, 20), i // 3) for i in range(12)]
            weights = [rng.randrange(5) for _ in range(4)]
            if not sum(weights):
                weights[0] = 1
            expanded = [value for value, i in rows for _ in range(weights[i])]
            for q in (0, .5, .95, 1):
                self.assertAlmostEqual(a.weighted_quantile(sorted(rows), weights, q), a.quantile(expanded, q))

    def test_twenty_thousand_replicates_preserve_paired_proportional_process_effect(self):
        # Process means span 100x. Independently drawing arms would spuriously
        # attribute common process noise to a three-percent candidate effect.
        left = [[1, 2], [10, 20], [100, 200], [3, 6]]
        c = cell(left, [[v * 1.03 for v in row] for row in left])
        result = a.bootstrap(c, replicates=20_000, engine='python')
        for q in a.QUANTILES:
            self.assertEqual(len(result[q]), 20_000)
            self.assertLess(max(abs(x - 1.03) for x in result[q]), 1e-12)
            self.assertGreater(a.interval(result[q])['simultaneousOneSidedUpperDeltaPercent'], 2)

    def test_identical_arms_remain_exactly_identical_under_cluster_and_round_block_draws(self):
        left = [[1 + i * 100 + j for j in range(20)] for i in range(4)]
        c = cell(left, left)
        for sensitivity in (False, True):
            result = a.bootstrap(c, replicates=512, sensitivity=sensitivity, engine='python')
            self.assertTrue(all(value == 1 for values in result.values() for value in values))

    def test_more_requests_do_not_remove_between_process_uncertainty(self):
        c1 = cell([[1]] * 4, [[.8], [.9], [1.1], [1.8]])
        c100 = cell([[1] * 100] * 4, [[v] * 100 for v in (.8, .9, 1.1, 1.8)])
        widths = []
        for c in (c1, c100):
            x = a.interval(a.bootstrap(c, replicates=1024, engine='python')[.95])['ratio95PercentCI']
            widths.append(x[1] - x[0])
        self.assertGreater(widths[1], .5)
        self.assertGreater(widths[1], .8 * widths[0])

    def test_contiguous_sensitivity_retains_serial_structure_and_detects_within_pair_variation(self):
        indices = a.circular_indices(23, 10, random.Random(29))
        self.assertEqual(len(indices), 23)
        for start in (0, 10, 20):
            for i in range(start + 1, min(start + 10, 23)):
                self.assertEqual(indices[i], (indices[i - 1] + 1) % 23)
        c = cell([[1] * 20] * 2, [[.8] * 10 + [1.2] * 10] * 2)
        primary = a.interval(a.bootstrap(c, replicates=512, engine='python')[.5])
        sensitivity = a.interval(a.bootstrap(c, replicates=1024, sensitivity=True, engine='python')[.5])
        self.assertEqual(primary['ratio95PercentCI'], [1, 1])
        lower, upper = sensitivity['ratio95PercentCI']
        self.assertGreater(upper - lower, .3)

    def test_joint_gate_uses_corrected_upper_tail_and_all_registered_ten_cells(self):
        distribution = [1.0] * 980 + [1.019] * 10 + [1.03] * 10
        bounds = a.interval(distribution, alpha=.05/10)
        self.assertLess(bounds['delta95PercentCI'][1], 2)
        self.assertGreater(bounds['simultaneousOneSidedUpperDeltaPercent'], 2)
        good = {'metrics': {'p95': a.interval([1.0] * 100)}}
        bad = {'metrics': {'p95': bounds}}
        self.assertTrue(a.family_gate([good] * 10, 10))
        self.assertFalse(a.family_gate([good] * 9 + [bad], 10))
        with self.assertRaisesRegex(ValueError, 'Incomplete primary family'):
            a.family_gate([good] * 9, 10)

    def test_family_is_derived_and_prospective_enabled_workload_cannot_disappear(self):
        import copy
        manifest = a.load(Path(__file__).with_name('experiment.template.json'))
        self.assertEqual(a.experiment_plan.family(manifest), (10, .005))
        changed = copy.deepcopy(manifest)
        changed['profiles'][-1]['pages'].pop()
        changed['analysis']['primaryCells'] = 9
        changed['analysis']['oneSidedCellAlpha'] = .05 / 9
        with self.assertRaisesRegex(ValueError, 'page family changed'):
            a.experiment_plan.family(changed)
        changed = copy.deepcopy(manifest)
        changed['profiles'][-1]['requestContexts']['admin']['kind'] = 'anonymous'
        with self.assertRaisesRegex(ValueError, 'distinct guest and administrator'):
            a.experiment_plan.family(changed)
        changed = copy.deepcopy(manifest)
        changed['analysis']['primaryCells'] = 8
        changed['analysis']['oneSidedCellAlpha'] = .05 / 8
        with self.assertRaisesRegex(ValueError, 'primary family differs'):
            a.experiment_plan.family(changed)
        controls = {g: [cell([[1]] * 12, [[1]] * 12, profile='i18n-enabled', page=str(i), path=f'/{i}') for i in range(2)] for g in ('pilot-AA', 'pilot-BB')}
        self.assertEqual(a.choose_n(controls, alpha=.005, replicates=256)['selectedPairs'], 80)

    def test_pilot_is_separate_and_freezes_smallest_qualifying_n(self):
        controls = {}
        for group in ('pilot-AA', 'pilot-BB'):
            controls[group] = [cell([[1] * 2] * 12, [[1.0005] * 2] * 12, page=str(i), path=f'/{i}') for i in range(4)]
        result = a.choose_n(controls, replicates=256, engine='python')
        self.assertEqual(result['selectedPairs'], 80)
        self.assertFalse(result['precisionLimited'])
        self.assertEqual(len(result['evaluated']), 1)
        with self.assertRaisesRegex(ValueError, 'Only independent same-binary pilot'):
            a.choose_n({'comparison': controls['pilot-AA']}, replicates=256)
        with self.assertRaisesRegex(ValueError, 'exactly 12'):
            a.choose_n({g: [replace(c, blocks=c.blocks[:10], left=c.left[:10], right=c.right[:10]) for c in cells] for g, cells in controls.items()}, replicates=256)

    def test_unattainable_pilot_precision_hits_fixed_cap_without_favorable_stopping(self):
        controls = {g: [cell([[1]] * 12, [[1]] * 12, page=str(i), path=f'/{i}') for i in range(4)] for g in ('pilot-AA', 'pilot-BB')}
        # Deliberately negative center: selection must use width, not a favorable
        # point estimate that would pass a noninferiority bound immediately.
        wide_negative = {q: [.5] * 128 + [.9] * 128 for q in a.QUANTILES}
        with patch.object(a, 'bootstrap', return_value=wide_negative) as bootstrap:
            result = a.choose_n(controls, replicates=256)
        self.assertEqual(result['selectedPairs'], 320)
        self.assertTrue(result['precisionLimited'])
        self.assertEqual([x['processPairs'] for x in result['evaluated']], [80, 160, 320])
        self.assertEqual(bootstrap.call_count, 24)

    def test_invalid_latencies_and_missing_pairs_are_refused_without_filtering(self):
        good = cell([[1, 2], [3, 4]], [[1, 2], [3, 4]])
        for value in (0, -1, float('nan'), float('inf'), True):
            with self.subTest(value=value), self.assertRaises(ValueError):
                a.bootstrap(replace(good, right=((1, value), (3, 4))), replicates=1)
        with self.assertRaisesRegex(ValueError, 'Incomplete paired-round'):
            a.bootstrap(replace(good, right=((1,), (3, 4))), replicates=1)
        with self.assertRaisesRegex(ValueError, 'Duplicate process'):
            a.bootstrap(replace(good, blocks=('same', 'same')), replicates=1)

    @unittest.skipIf(a.np is None, 'NumPy is optional and not installed')
    def test_numpy_acceleration_matches_python_draws_and_quantiles(self):
        c = cell([[1 + i + j / 10 for j in range(20)] for i in range(4)], [[1.02 + i * .9 + j / 8 for j in range(20)] for i in range(4)])
        for sensitivity in (False, True):
            reference = a.bootstrap(c, replicates=64, sensitivity=sensitivity, engine='python')
            fast = a.bootstrap(c, replicates=64, sensitivity=sensitivity, engine='numpy')
            for q in a.QUANTILES:
                for x, y in zip(reference[q], fast[q]):
                    self.assertAlmostEqual(x, y, places=12)


if __name__ == '__main__':
    unittest.main()
