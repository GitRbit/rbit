#!/usr/bin/env python3
"""Offline analysis of actual scripts/perf reports. Never starts servers or builds.

Whole process-pair clusters are the primary resampling units. Optional NumPy only
accelerates weighted quantile evaluation; Python's seeded draws define both engines.
"""
from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import sys
import plan as experiment_plan
from urllib.parse import urlsplit

try:
    import numpy as np
except ImportError:
    np = None

REPLICATES = 20_000
SINGLE_CELL_ALPHA = .05
QUANTILES = (.5, .95)
SIZES = (80, 160, 320)
TARGET = 1.25
SEED = '20261101-i18n-frozen-v1'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def load(path):
    return json.loads(Path(path).read_text())


def load_hashed(path):
    raw = Path(path).read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for part in iter(lambda: f.read(1 << 20), b''):
            h.update(part)
    return h.hexdigest()


def quantile(values, q):
    require(0 <= q <= 1 and len(values) > 0, 'Bad quantile input')
    a = sorted(values)
    rank = (len(a) - 1) * q
    lo, hi = math.floor(rank), math.ceil(rank)
    return a[lo] + (a[hi] - a[lo]) * (rank - lo)


@dataclass(frozen=True)
class Cell:
    profile: str
    page: str
    path: str
    blocks: tuple[str, ...]
    left: tuple[tuple[float, ...], ...]
    right: tuple[tuple[float, ...], ...]

    def validate(self):
        require(self.left and len(self.left) == len(self.right) == len(self.blocks), 'Missing process pairs')
        require(len(set(self.blocks)) == len(self.blocks), 'Duplicate process pairs')
        rounds = len(self.left[0])
        require(rounds > 0, 'Empty process pair')
        require(all(len(x) == rounds for x in (*self.left, *self.right)), 'Incomplete paired-round matrix')
        require(all(isinstance(v, (float, int)) and not isinstance(v, bool) and math.isfinite(v) and v > 0 for arm in (self.left, self.right) for block in arm for v in block), 'Invalid latency; do not remove or clamp it')
        return len(self.left), rounds


def derived_seed(label):
    return int.from_bytes(hashlib.sha256(f'{SEED}:{label}'.encode()).digest(), 'big')


def circular_indices(rounds, block_length, rng):
    """One paired circular moving-block draw, truncated to exactly rounds values."""
    require(rounds > 0 and 1 <= block_length <= rounds, 'Invalid round-block size')
    out = []
    while len(out) < rounds:
        start = rng.randrange(rounds)
        out.extend((start + i) % rounds for i in range(min(block_length, rounds - len(out))))
    return out


def weighted_quantile(sorted_pairs, weights, q):
    """Type-7 quantile of a multiset without expanding repeated process draws."""
    total = sum(weights[index] for _, index in sorted_pairs)
    require(total > 0, 'Empty weighted empirical distribution')
    rank = (total - 1) * q
    lo, hi = math.floor(rank), math.ceil(rank)
    cumulative = 0
    low_value = None
    for value, index in sorted_pairs:
        cumulative += weights[index]
        if low_value is None and cumulative > lo:
            low_value = value
        if cumulative > hi:
            return low_value + (value - low_value) * (rank - lo)
    raise ValueError('Weighted rank not reached')


def draw_weights(k, rounds, draws, rng, sensitivity, block_length):
    if not sensitivity:
        weights = [0] * k
        for selected in rng.choices(range(k), k=draws):
            weights[selected] += 1
        return weights
    weights = [0] * (k * rounds)
    # The same cluster and within-cluster round indices supply BOTH arms.
    for selected in rng.choices(range(k), k=draws):
        for round_ in circular_indices(rounds, block_length, rng):
            weights[selected * rounds + round_] += 1
    return weights


def _numpy_quantiles(sorted_pairs, weight_rows, qs):
    values = np.asarray([value for value, _ in sorted_pairs], dtype=np.float64)
    indices = np.asarray([index for _, index in sorted_pairs], dtype=np.int64)
    cdf = np.cumsum(np.asarray(weight_rows, dtype=np.uint64)[:, indices], axis=1, dtype=np.uint64)
    totals = cdf[:, -1]
    output = []
    for q in qs:
        ranks = (totals - 1).astype(np.float64) * q
        lows, highs = np.floor(ranks).astype(np.uint64), np.ceil(ranks).astype(np.uint64)
        li = np.argmax(cdf > lows[:, None], axis=1)
        hi = np.argmax(cdf > highs[:, None], axis=1)
        output.append((values[li] + (values[hi] - values[li]) * (ranks - lows)).tolist())
    return output


def bootstrap(cell, *, replicates=REPLICATES, draws=None, sensitivity=False, block_length=10, label='', engine='auto'):
    k, rounds = cell.validate()
    require(replicates >= 1, 'Empty bootstrap')
    draws = k if draws is None else draws
    require(isinstance(draws, int) and draws > 0, 'Invalid process draw count')
    require(not sensitivity or 1 <= block_length <= rounds, 'Invalid sensitivity block length')
    require(engine in ('auto', 'python', 'numpy'), 'Unknown engine')
    require(engine != 'numpy' or np is not None, 'NumPy requested but unavailable')
    use_numpy = np is not None and engine != 'python'
    arms = [[v for block in a for v in block] for a in (cell.left, cell.right)]
    # Flat constant arms are exact under any allowed paired resampling scheme.
    if all(min(a) == max(a) for a in arms):
        ratio = arms[1][0] / arms[0][0]
        return {q: [ratio] * replicates for q in QUANTILES}
    ordered = [sorted((value, i if sensitivity else i // rounds) for i, value in enumerate(a)) for a in arms]
    rng = random.Random(derived_seed(f'{label}:{cell.profile}:{cell.path}:{draws}:{sensitivity}'))
    result = {q: [] for q in QUANTILES}
    # Bound the acceleration's intermediate CDF memory to 32 process draws/batch.
    for offset in range(0, replicates, 32):
        rows = [draw_weights(k, rounds, draws, rng, sensitivity, block_length) for _ in range(min(32, replicates - offset))]
        if use_numpy:
            left, right = [_numpy_quantiles(a, rows, QUANTILES) for a in ordered]
            for j, q in enumerate(QUANTILES):
                result[q].extend(b / a for a, b in zip(left[j], right[j]))
        else:
            for weights in rows:
                for q in QUANTILES:
                    result[q].append(weighted_quantile(ordered[1], weights, q) / weighted_quantile(ordered[0], weights, q))
    return result


def interval(ratios, alpha=SINGLE_CELL_ALPHA):
    median = quantile(ratios, .5)
    lower, upper = quantile(ratios, .025), quantile(ratios, .975)
    corrected_lower, corrected_upper = quantile(ratios, alpha), quantile(ratios, 1 - alpha)
    return {
        'ratio95PercentCI': [lower, upper],
        'delta95PercentCI': [100 * (lower - 1), 100 * (upper - 1)],
        'simultaneousOneSidedUpperRatio': corrected_upper,
        'simultaneousOneSidedUpperDeltaPercent': 100 * (corrected_upper - 1),
        'precisionPercentagePoints': 100 * max(corrected_upper - median, median - corrected_lower),
    }


def summarize(cell, *, alpha=SINGLE_CELL_ALPHA, replicates=REPLICATES, engine='auto', sensitivity=True):
    k, rounds = cell.validate()
    primary = bootstrap(cell, replicates=replicates, label='confirmatory-primary', engine=engine)
    secondary = bootstrap(cell, replicates=replicates, sensitivity=True, block_length=10, label='confirmatory-sensitivity', engine=engine) if sensitivity else None
    flat = [[v for block in arm for v in block] for arm in (cell.left, cell.right)]
    metrics = {}
    for q, name in zip(QUANTILES, ('p50', 'p95')):
        left, right = [quantile(a, q) for a in flat]
        ratios = primary[q]
        metrics[name] = {'leftMs': left, 'rightMs': right, 'ratio': right / left, 'deltaPercent': 100 * (right / left - 1), **interval(ratios, alpha)}
        if secondary:
            metrics[name]['pairedCircularRoundBlockSensitivity'] = interval(secondary[q], alpha)
    deltas = [100 * (quantile(b, .95) / quantile(a, .95) - 1) for a, b in zip(cell.left, cell.right)]
    return {'profile': cell.profile, 'page': cell.page, 'path': cell.path, 'processPairs': k, 'pairedRoundsPerProcess': rounds, 'metrics': metrics,
            'perProcessP95DeltaPercent': dict(zip(cell.blocks, deltas)),
            'perProcessDeltaDistribution': {str(q): quantile(deltas, q) for q in (0, .05, .25, .5, .75, .95, 1)}}


def choose_n(cells_by_control, *, alpha=SINGLE_CELL_ALPHA, replicates=REPLICATES, engine='auto'):
    """Accept only separately supplied A/A and B/B pilot cells, never A/B effects."""
    require(set(cells_by_control) == {'pilot-AA', 'pilot-BB'}, 'Only independent same-binary pilot groups may select N')
    require(all(cells_by_control[g] for g in cells_by_control), 'Pilot pages cannot be empty')
    require({(c.page, c.path) for c in cells_by_control['pilot-AA']} == {(c.page, c.path) for c in cells_by_control['pilot-BB']}, 'Same-binary pilot page families differ')
    require(len({cell.profile for cells in cells_by_control.values() for cell in cells}) == 1, 'Do not pool profiles for N')
    require(all(cell.validate()[0] == 12 for cells in cells_by_control.values() for cell in cells), 'Pilot requires exactly 12 process pairs per same-binary group')
    estimates = []
    for n in SIZES:
        entries = []
        for group, cells in cells_by_control.items():
            for cell in cells:
                ratios = bootstrap(cell, replicates=replicates, draws=n, label=f'pilot-only:{group}', engine=engine)[.95]
                entries.append({'group': group, 'page': cell.page, **interval(ratios, alpha)})
        width = max(item['precisionPercentagePoints'] for item in entries)
        estimates.append({'processPairs': n, 'maximumPrecisionPercentagePoints': width, 'cells': entries})
        if width <= TARGET:
            return {'selectedPairs': n, 'precisionLimited': False, 'evaluated': estimates}
    return {'selectedPairs': 320, 'precisionLimited': True, 'evaluated': estimates,
            'warning': 'Pilot cannot predict target precision within cap; no guarantee. Improve conditions before confirmation or retain explicitly limited precision.'}


def family_gate(comparison, expected_cells):
    require(len(comparison) == expected_cells, 'Incomplete primary family')
    return all(c['metrics']['p95']['simultaneousOneSidedUpperRatio'] <= 1.02 for c in comparison)


def same_stable_machine(machine):
    return {k: v for k, v in machine.items() if k != 'freeMemoryAtStart'}


def utc_time(value, label):
    parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
    require(parsed.tzinfo is not None and parsed.utcoffset() is not None, f'{label}: timestamp must include timezone')
    return parsed.astimezone(timezone.utc)


def confirmation_after_freeze(started, frozen_at):
    require(started >= frozen_at, 'Confirmatory process started before sample-size freeze')


def frozen_selection(manifest, manifest_path):
    freeze, freeze_sha = load_hashed(manifest['freezeRecord'])
    require(freeze['schema'] == 2, 'Superseded freeze schema')
    require(freeze['planImplementationSha256'] == sha(experiment_plan.__file__), 'Planner changed after freeze')
    require(freeze['experimentManifestSha256'] == sha(manifest_path) and freeze['analysisImplementationSha256'] == sha(__file__), 'Changed frozen manifest/analysis')
    require(freeze['selectionUsesOnlySameBinaryPilot'] is True and freeze['confirmatoryEffectNotInspected'] is True, 'Invalid freeze')
    records = {key: load_hashed(freeze[key]) for key in ('pilotAnalysis', 'pilotCompleteRecord')}
    for key, (_, digest) in records.items():
        require(digest == freeze[key + 'Sha256'], 'Changed pilot evidence')
    pilot = records['pilotAnalysis'][0]
    require(pilot['complete'] is True and pilot['valid'] is True and pilot['phase'] == 'pilot', 'Incomplete or non-pilot evidence')
    require(pilot['provenance']['manifestSha256'] == sha(manifest_path), 'Pilot used a different manifest')
    require(pilot['provenance']['analysisSha256'] == sha(__file__) and pilot['provenance']['planImplementationSha256'] == sha(experiment_plan.__file__), 'Pilot used different analysis/planner code')
    frozen_at = utc_time(freeze['frozenAtUTC'], 'freeze')
    pilot_finalized = utc_time(pilot['provenance']['processesFinalizedAtUTC'], 'pilot finalization')
    require(pilot_finalized <= frozen_at, 'Freeze predates complete pilot process finalization')
    require(utc_time(pilot['analysisFinishedAtUTC'], 'pilot analysis completion') <= frozen_at, 'Freeze predates pilot analysis completion')
    selected = freeze['selectedPairs']
    require(selected == pilot['selectedPairs'], 'N differs from pilot-only decision')
    require(set(selected) == {p['name'] for p in manifest['profiles']} and all(type(n) is int and n in SIZES for n in selected.values()), 'N outside frozen cap or profile family')
    evidence = {'freezeRecord': {'path': manifest['freezeRecord'], 'sha256': freeze_sha}, **{key: {'path': freeze[key], 'sha256': records[key][1]} for key in records}}
    return selected, pilot, frozen_at, evidence


def checked_protocol(plan_path, specification):
    path = Path(plan_path).resolve().parent / 'protocol.json'
    record, digest = load_hashed(path)
    require(record['schema'] == 2 and record['phase'] == specification['phase'] and record['complete'] is True and record['valid'] is True, 'Executor marked attempted run incomplete/invalid')
    require(record['planSha256'] == sha(plan_path) and record['executorSha256'] == sha(Path(__file__).with_name('execute.py')), 'Executor/plan provenance differs')
    require([b['block'] for b in record['blocks']] == [b['block'] for b in specification['blocks']], 'Executor did not complete the entire planned order')
    for block, expected in zip(record['blocks'], specification['blocks']):
        require({c['label'] for c in block['cleanup']} == {c['label'] for c in expected['commands']}, 'Executor cleanup omitted/repeated a launcher')
        require(block['valid'] is True and len(block['cleanup']) == 2 and all(c['clean'] is True and c['forcedKill'] is False and c['launcherExitCode'] == 0 for c in block['cleanup']), 'Executor cleanup was not clean; child exit alone is insufficient')
    started, ended = utc_time(record['startedAtUTC'], 'executor start'), utc_time(record['endedAtUTC'], 'executor end')
    require(started <= ended, 'Invalid executor chronology')
    return {'path': str(path), 'sha256': digest, 'startedAtUTC': started.isoformat(), 'endedAtUTC': ended.isoformat(), 'executorSha256': record['executorSha256']}


def load_run(plan_path, manifest_path, expected_phase):
    """Read sampler and launcher formats actually emitted by scripts/perf/*.mjs.

    Any invalid/missing sample invalidates the attempted run. Nothing is silently
    filtered except the predefined warmup phase. Raw files are never altered.
    """
    plan_path, manifest_path = Path(plan_path).resolve(), Path(manifest_path).resolve()
    plan, manifest = load(plan_path), load(manifest_path)
    executor_evidence = checked_protocol(plan_path, plan)
    require(plan['schema'] == 2 and plan['phase'] == expected_phase, 'Wrong run phase')
    require(plan['planScriptSha256'] == sha(experiment_plan.__file__), 'Planner implementation changed since generation')
    require(plan['manifestSha256'] == sha(manifest_path), 'Manifest changed since plan generation')
    family_cells, cell_alpha = experiment_plan.family(manifest)
    require(manifest['schema'] == 2 and manifest['analysis']['replicates'] >= REPLICATES, 'Changed protocol schema or Monte Carlo budget')
    require(manifest['analysis']['seed'] == SEED, 'Changed frozen seed')
    require(manifest['rounds'] == 100 and manifest['warmups'] == 20, 'Changed sampling design')
    require(manifest['pilot']['eligibleConfirmatoryPairs'] == list(SIZES) and manifest['pilot']['precisionTargetPercentagePoints'] == TARGET, 'Changed sample-size selection design')
    profiles_by_name = {p['name']: p for p in manifest['profiles']}
    require(manifest['candidateRevision'] == 'b951eed02f7fb13994d013f71b9ce87206f59f41', 'Wrong isolated i18n candidate; later Actions/docs heads are separate')
    require(manifest['limits'] == {'readinessSeconds': 30, 'samplerSeconds': 120, 'shutdownSeconds': 30}, 'Changed reviewed executor deadlines')
    require(manifest['baselineRevision'] == 'd3dc8980dd091c31ceffc61f916ca282b10ab506', 'Wrong isolated baseline')
    require(manifest['candidateRevision'] != manifest['baselineRevision'], 'No extraction comparison')
    # Revalidate recorded build/source inputs once, rather than trusting editable
    # arm labels in the command plan or checking only the binary metadata.
    for profile in manifest['profiles']:
        checked = {}
        for arm in ('A', 'B'):
            revision = manifest['baselineRevision' if arm == 'A' else 'candidateRevision']
            checked[arm] = experiment_plan.record(profile['artifacts'][arm], revision, profile['features'])
            require(checked[arm]['archiveSha256'] == manifest['sourceArchives'][arm]['archive_sha256'] and checked[arm]['cargoLockSha256'] == manifest['sourceArchives'][arm]['lockfile_sha256'], 'Build is not the pinned source archive/lock')
            require(checked[arm] == plan['artifacts'][profile['name']][arm], 'Build evidence changed since planning')
        require(experiment_plan.external_packages(checked['A']['sourceDir']) == experiment_plan.external_packages(checked['B']['sourceDir']), 'External Cargo.lock package records drifted')
        for key in ('rustcVV', 'cargoVV', 'target', 'buildEnvironment', 'effectiveReleaseProfile', 'packageLockSha256', 'assetManifestSha256', 'configInputsRecordSha256'):
            require(checked['A'][key] == checked['B'][key], f'A/B build input differs: {key}')
    input_evidence = plan['inputEvidence']
    require(set(input_evidence) == {'semanticChecksRecord', 'machineAndIdleConditionsRecord'}, 'Missing manual workload/conditions evidence')
    for role, record in input_evidence.items():
        require(record['path'] == manifest[role] and sha(record['path']) == record['sha256'], 'Manual workload/conditions evidence changed after planning')
        require(bool(load(record['path'])), 'Empty manual evidence record')
    for relative, expected in plan['harnessFiles'].items():
        require(sha(experiment_plan.ROOT / relative) == expected, 'Harness source changed since planning')
    require(sha(manifest['seedFileManifest']) == manifest['seedFileManifestSha256'], 'Seed inventory changed after registration')
    _, smoke_evidence = experiment_plan.checked_smoke(manifest)
    require(plan['smokeEvidence'] == smoke_evidence, 'Finalized functional smoke evidence changed after planning')
    seed_inventory = load(manifest['seedFileManifest'])
    selected, pilot, frozen_at, freeze_evidence = frozen_selection(manifest, manifest_path) if expected_phase == 'confirmatory' else (None, None, None, None)
    groups, evidence, seen_dirs, seen_blocks = {}, [], set(), set()
    stable_fixture = stable_harness = stable_machine = None
    phase_counts = Counter()
    previous_block_end = None
    first_process_start = None
    for block in plan['blocks']:
        name, phase, profile = block['block'], block['phase'], block['profile']
        require(name not in seen_blocks, 'Duplicate process-pair block')
        seen_blocks.add(name)
        phase_counts[(profile, phase)] += 1
        require(profile in profiles_by_name, 'Unregistered profile')
        pages = profiles_by_name[profile]['pages']
        config = block['config']
        config_path = Path(block['measureArgv'][-1])
        require(load(config_path) == config, f'{name}: actual config differs from plan')
        report_path = Path(config['output'])
        report = load(report_path)
        require(report.get('complete') is True and report.get('valid') is True, f'{name}: invalid/incomplete attempt; retain it, do not select valid-looking rows')
        require(all(report['config'].get(k) == v for k, v in config.items()), f'{name}: report/config differs')
        require(config.get('requestContexts') == profiles_by_name[profile]['requestContexts'], f'{name}: changed cookie-context bindings')
        require(config['pages'] == pages and config['rounds'] == 100 and config['warmups'] == 20, f'{name}: changed workload')
        with Path(str(report_path) + '.jsonl').open() as stream:
            meta = json.loads(next(stream))
            samples = [json.loads(line) for line in stream if line.strip()]
        require(meta.get('type') == 'metadata' and samples == report['samples'], f'{name}: JSONL differs')
        for key in ('config', 'servers', 'fixture', 'harness', 'machine', 'startedAt', 'bootstrap'):
            require(meta[key] == report[key], f'{name}: JSONL {key} differs')
        harness = {key: report['harness'][key] for key in ('measureSha256', 'supportSha256', 'contextsSha256')}
        require(harness['measureSha256'] == plan['harnessFiles'][str(Path(manifest['samplerPath']).resolve().relative_to(experiment_plan.ROOT))] and harness['supportSha256'] == plan['harnessFiles']['scripts/perf/support.mjs'], f'{name}: sampler source differs from plan')
        require(harness['contextsSha256'] == plan['harnessFiles'][str((Path(manifest['samplerPath']).resolve().parent / 'contexts.mjs').relative_to(experiment_plan.ROOT))], f'{name}: cookie-context implementation differs')
        machine = same_stable_machine(report['machine'])
        if stable_fixture is None:
            stable_fixture, stable_harness, stable_machine = report['fixture'], harness, machine
        require(report['fixture'] == stable_fixture and report['fixture']['status'] == '', f'{name}: dirty/changed fixture')
        require(harness == stable_harness and machine == stable_machine, f'{name}: changed harness/machine')
        mapping = {command['label']: command['artifactArm'] for command in block['commands']}
        require(len(mapping) == 2 and mapping['baseline'] in ('A', 'B'), f'{name}: invalid arm mapping')
        right = 'candidate' if phase == 'comparison' else 'control'
        arms = ['baseline', right]
        require(set(mapping) == set(arms), f'{name}: changed labels')
        expected_builds = ('A', 'B') if phase == 'comparison' else ('B', 'B') if phase in ('pilot-BB', 'control-BB') else ('A', 'A')
        require(tuple(mapping[arm] for arm in arms) == expected_builds, f'{name}: same-binary/control identity differs')
        require([v['name'] for v in config['variants']] == arms, f'{name}: changed request arm order')
        servers = {server['baseUrl']: server for server in report['servers']}
        require(len(servers) == 2, f'{name}: duplicate server')
        server_evidence = []
        block_start_times, block_end_times = [], []
        for variant in config['variants']:
            arm, url = variant['name'], variant['baseUrl']
            server = servers[url]
            build = plan['artifacts'][profile][mapping[arm]]
            expected_rev = manifest['baselineRevision' if mapping[arm] == 'A' else 'candidateRevision']
            require(build['revision'] == expected_rev and server['declaredSourceRevision'] == expected_rev, f'{name}: wrong revision')
            require(server['profile'] == 'release' and server['executableSha256'] == build['binarySha256'] and sha(server['executable']) == build['binarySha256'], f'{name}: wrong binary/profile')
            require(server['sourceDir'] == build['sourceDir'], f'{name}: wrong exported source')
            require(load(variant['metadata']) == server, f'{name}: server metadata differs')
            exit_path = variant['metadata'] + '.exit.json'
            exit_record = load(exit_path)
            require(exit_record['code'] == 0 and exit_record['signal'] is None, f'{name}: unclean shutdown')
            started = utc_time(server['startedAt'], 'process start')
            ended = utc_time(exit_record['endedAt'], 'process end')
            require(ended >= started, f'{name}: invalid process timestamps')
            if frozen_at is not None:
                confirmation_after_freeze(started, frozen_at)
            block_start_times.append(started)
            block_end_times.append(ended)
            require(server['dataDir'] not in seen_dirs, f'{name}: reused data directory')
            seen_dirs.add(server['dataDir'])
            # Added by the new executor before startup, in the documented inventory
            # format; old run03 lacks this evidence and is intentionally rejected.
            seed_record = variant['metadata'] + '.seed-before-start.json'
            require(load(seed_record) == seed_inventory, f'{name}: pre-start copied seed differs')
            expected_port = (3045 if phase_counts[(profile, phase)] % 2 else 3046) if arm == 'baseline' else (3046 if phase_counts[(profile, phase)] % 2 else 3045)
            require(server['port'] == expected_port == urlsplit(url).port, f'{name}: port alternation differs')
            server_evidence.append({'arm': arm, 'artifactArm': mapping[arm], 'startedAt': server['startedAt'], 'endedAt': exit_record['endedAt'], 'port': server['port'], 'metadataSha256': sha(variant['metadata']), 'exitSha256': sha(exit_path), 'seedBeforeStartSha256': sha(seed_record), 'serverLogSha256': sha(variant['metadata'] + '.log')})
        measured_start = utc_time(report['startedAt'], 'sampler start')
        measured_end = utc_time(report['endedAt'], 'sampler end')
        require(max(block_start_times) <= measured_start <= measured_end <= min(block_end_times), f'{name}: sampler outside live process-pair interval')
        require(previous_block_end is None or min(block_start_times) >= previous_block_end, f'{name}: process pairs overlap or UTC timeline changed')
        if first_process_start is None:
            first_process_start = min(block_start_times)
        previous_block_end = max(block_end_times)
        require([s['port'] for s in sorted(server_evidence, key=lambda s: utc_time(s['startedAt'], 'launch order'))] == [3045, 3046], f'{name}: launch order differs')
        bootstraps = {entry['variant']: entry['contexts'] for entry in report['bootstrap']}
        require(set(bootstraps) == set(arms), f'{name}: missing per-variant bootstrap evidence')
        rows = {}
        previous_sample_start = measured_start
        for sample in report['samples']:
            sample_start = utc_time(sample['startedAt'], 'sample start')
            require(previous_sample_start <= sample_start <= measured_end, f'{name}: sample outside monotone sampler interval')
            previous_sample_start = sample_start
            require(sample['phase'] in ('warmup', 'measured') and sample['variant'] in arms, f'{name}: unknown sample group')
            page = next((p for p in pages if p['name'] == sample['page']), None)
            require(page is not None, f'{name}: unknown page')
            context_name = page['context']
            context_spec = config['requestContexts'][context_name]
            context = bootstraps[sample['variant']][context_name]
            origin = next(v['baseUrl'] for v in config['variants'] if v['name'] == sample['variant'])
            require(sample['requestContext'] == context_name and context['kind'] == context_spec['kind'] and context['origin'] == origin, f'{name}: request context differs from registered origin/role')
            headers = sample['requestHeaders']
            require(not any(k.lower() == 'authorization' for k in headers), f'{name}: unexpected credential header')
            if context_spec['kind'] == 'anonymous':
                require(sample['cookieFingerprint'] is None and context['cookieFingerprint'] is None and not any(k.lower() == 'cookie' for k in headers), f'{name}: anonymous workload received credentials')
            else:
                digest = context['cookieFingerprint']
                require(isinstance(digest, str) and len(digest) == 64 and all(c in '0123456789abcdef' for c in digest) and sample['cookieFingerprint'] == digest and headers.get('Cookie') == '[redacted]' and context['semanticValid'] is True, f'{name}: private cookie/authority semantics changed after bootstrap')
                require(utc_time(context['atUTC'], 'bootstrap') <= measured_start, f'{name}: bootstrap did not precede warmup')
            require(not any(k.lower() in ('set-cookie', 'location', 'authorization', 'cookie') for k in sample['responseHeaders']), f'{name}: sensitive response header must not be serialized')
            if page.get('contentLanguage'):
                require(sample['responseHeaders'].get('content-language') == page['contentLanguage'], f'{name}: wrong locale metadata')
            limit = 20 if sample['phase'] == 'warmup' else 100
            require(type(sample['round']) is int and 1 <= sample['round'] <= limit, f'{name}: bad round')
            key = (sample['phase'], sample['page'], sample['round'], sample['variant'])
            require(key not in rows, f'{name}: duplicate sample')
            rows[key] = sample
            require(sample['valid'] is True and not sample['errors'] and sample['status'] == 200, f'{name}: transport/representation failure; entire attempt invalid')
            expected_url = next(v['baseUrl'] for v in config['variants'] if v['name'] == sample['variant']) + page['path']
            require(sample['url'] == expected_url, f'{name}: wrong request path/arm')
            require(sample['requestHeaders'].get('Accept-Encoding') == 'identity' and sample['responseHeaders'].get('content-encoding') in (None, 'identity'), f'{name}: changed encoding')
            for metric in ('totalMs', 'headersMs', 'renderMs'):
                value = sample.get(metric)
                require(type(value) in (int, float) and math.isfinite(value) and (value > 0 if metric == 'totalMs' else value >= 0), f'{name}: bad {metric}')
        require(len(rows) == len(pages) * 2 * 120, f'{name}: incomplete paired matrix')
        expected_sample_order = [(phase_, page_['name'], round_, arm_) for phase_, count in (('warmup', 20), ('measured', 100)) for round_ in range(1, count + 1) for page_ in pages for arm_ in (arms if round_ % 2 else list(reversed(arms)))]
        require([(s['phase'], s['page'], s['round'], s['variant']) for s in report['samples']] == expected_sample_order, f'{name}: paired request order / warmup/page sequence differs')
        for page in pages:
            group = groups.setdefault((profile, phase, page['name']), {'blocks': [], 'left': [], 'right': [], 'path': page['path']})
            group['blocks'].append(name)
            for arm, key in (('baseline', 'left'), (right, 'right')):
                group[key].append(tuple(rows[('measured', page['name'], r, arm)]['totalMs'] for r in range(1, 101)))
        postflight = {entry['variant']: entry['pages'] for entry in report['postflight']}
        require(set(postflight) == set(arms), f'{name}: missing postflight semantic evidence')
        for arm in arms:
            require([entry['page'] for entry in postflight[arm]] == [page['name'] for page in pages], f'{name}: postflight page family differs')
            for entry, page in zip(postflight[arm], pages):
                context = bootstraps[arm][page['context']]
                require(entry['valid'] is True and entry['path'] == page['path'] and entry['context'] == page['context'] and entry['origin'] == context['origin'] and entry['cookieFingerprint'] == context['cookieFingerprint'], f'{name}: postflight authority/path semantics differ')
                require(measured_start <= utc_time(entry['atUTC'], 'postflight') <= measured_end, f'{name}: postflight escaped live sampler interval')
                if page.get('contentLanguage'):
                    require(entry['contentLanguage'] == page['contentLanguage'], f'{name}: postflight locale differs')
        evidence.append({'block': name, 'reportSha256': sha(report_path), 'jsonlSha256': sha(str(report_path) + '.jsonl'), 'configSha256': sha(config_path), 'servers': server_evidence, 'validSamplesIncludingExcludedWarmups': len(rows)})
    if expected_phase == 'confirmatory':
        require(pilot['provenance']['harness'] == stable_harness and pilot['provenance']['fixture'] == stable_fixture and pilot['provenance']['machine'] == stable_machine, 'Pilot and confirmation harness/fixture/machine differ')
        require(pilot['provenance']['inputEvidence'] == input_evidence, 'Pilot and confirmation manual workload/conditions evidence differ')
        require(pilot['provenance']['seedManifestSha256'] == sha(manifest['seedFileManifest']), 'Pilot and confirmation seed differ')
    profiles = [p['name'] for p in manifest['profiles']]
    expected_counts = {}
    for profile in profiles:
        if expected_phase == 'pilot':
            for phase in ('pilot-AA', 'pilot-BB'):
                expected_counts[(profile, phase)] = 12
        else:
            require(selected[profile] in SIZES, 'N outside frozen cap')
            for phase, count in (('comparison', selected[profile]), ('control-before', 10), ('control-BB', 10), ('control-after', 10)):
                expected_counts[(profile, phase)] = count
    expected_order = []
    for profile in profiles:
        if expected_phase == 'pilot':
            expected_order.extend((profile, phase) for _ in range(12) for phase in ('pilot-AA', 'pilot-BB'))
        else:
            n = selected[profile]
            expected_order.extend([(profile, 'control-before')] * 10)
            for i in range(n):
                expected_order.append((profile, 'comparison'))
                if (i + 1) * 10 // n > i * 10 // n:
                    expected_order.append((profile, 'control-BB'))
            expected_order.extend([(profile, 'control-after')] * 10)
    require([(b['profile'], b['phase']) for b in plan['blocks']] == expected_order, 'Changed phase/profile ordering or incomplete run')
    require(dict(phase_counts) == expected_counts, 'Missing/extra process pairs, profiles or phases; do not stop on a favorable effect')
    cells = {key: Cell(key[0], key[2], value['path'], tuple(value['blocks']), tuple(value['left']), tuple(value['right'])) for key, value in groups.items()}
    require(utc_time(executor_evidence['startedAtUTC'], 'executor start') <= first_process_start <= previous_block_end <= utc_time(executor_evidence['endedAtUTC'], 'executor end'), 'Process timeline escaped executor interval')
    provenance = {'realHostSmoke': smoke_evidence, 'executorProtocol': executor_evidence, 'planSha256': sha(plan_path), 'manifestSha256': sha(manifest_path), 'analysisSha256': sha(__file__), 'planImplementationSha256': sha(experiment_plan.__file__), 'seedManifestSha256': sha(manifest['seedFileManifest']), 'inputEvidence': input_evidence, 'sampleSizeFreeze': freeze_evidence, 'firstProcessStartedAtUTC': first_process_start.isoformat(), 'processesFinalizedAtUTC': previous_block_end.isoformat(), 'harness': stable_harness, 'machine': stable_machine, 'fixture': stable_fixture, 'blocks': evidence,
                  'limits': 'Recorded evidence is not independent build/input attestation. Seed-before-start inventory is an executor record; final mutated directories cannot recreate startup state. No live-body or executable authorization claim.'}
    return manifest, cells, provenance


def write_new(path, value):
    path = Path(path).resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('x') as out:
        json.dump(value, out, indent=2, allow_nan=False)
        out.write('\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('pilot', 'confirmatory'))
    parser.add_argument('plan')
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--freeze-output')
    parser.add_argument('--engine', choices=('auto', 'python', 'numpy'), default='auto')
    args = parser.parse_args()
    manifest, cells, provenance = load_run(args.plan, args.manifest, args.phase)
    replicates = manifest['analysis']['replicates']
    require(replicates >= REPLICATES, 'At least 20,000 replicates required')
    result = {'schema': 2, 'complete': True, 'valid': True, 'phase': args.phase, 'provenance': provenance,
              'method': {'replicates': replicates, 'seed': SEED, 'backend': 'numpy' if np is not None and args.engine != 'python' else 'python', 'numpyVersion': None if np is None else np.__version__, 'pythonVersion': platform.python_version(), 'random': 'Python Random with SHA256-derived integer seed', 'primaryUnits': 'whole paired process clusters', 'estimand': 'Qq(pooled B requests)/Qq(pooled A requests), effectPercent=100*(ratio-1), q=.50/.95; not average/median per-process quantile effects', 'withinProcessSensitivity': 'paired circular blocks of ten contiguous rounds', 'warmups': 'all twenty excluded; every measured sample retained, including slow valid samples', 'invalidity': 'Any predefined transport/protocol/provenance violation invalidates attempted run; raw evidence retained; no outlier deletion, automatic replacement or favorable stopping', 'primaryFamily': f'{experiment_plan.family(manifest)[0]} pooled-request p95 page/profile cells only; p50 and sensitivity are descriptive', 'oneSidedCellAlpha': experiment_plan.family(manifest)[1], 'scope': 'warm OS caches, registered anonymous core and guest/admin enabled-i18n HTTP workloads on the declared machine/session; no disk-cold, other locale/provider/load or universal speed extrapolation'}}
    profiles = [p['name'] for p in manifest['profiles']]
    if args.phase == 'pilot':
        decisions = {}
        for profile in profiles:
            controls = {g: [cells[(profile, g, p['name'])] for p in next(p_ for p_ in manifest['profiles'] if p_['name'] == profile)['pages']] for g in ('pilot-AA', 'pilot-BB')}
            decisions[profile] = choose_n(controls, alpha=experiment_plan.family(manifest)[1], replicates=replicates, engine=args.engine)
        result['decisions'] = decisions
        result['selectedPairs'] = {p: d['selectedPairs'] for p, d in decisions.items()}
        result['precisionLimitedProfiles'] = [p for p, d in decisions.items() if d['precisionLimited']]
        result['note'] = 'Pilot variance estimate is uncertain; selected N does not guarantee final precision. No A/B effect was supplied.'
        result['analysisFinishedAtUTC'] = datetime.now(timezone.utc).isoformat()
        write_new(args.output, result)
        if args.freeze_output:
            require(str(Path(args.freeze_output).resolve()) == str(Path(manifest['freezeRecord']).resolve()), 'Freeze output must be the already-declared manifest path')
            freeze = {'schema': 2, 'experimentManifestSha256': sha(args.manifest), 'pilotCompleteRecord': str(Path(args.output).resolve()), 'pilotCompleteRecordSha256': sha(args.output), 'pilotAnalysis': str(Path(args.output).resolve()), 'pilotAnalysisSha256': sha(args.output), 'analysisImplementationSha256': sha(__file__), 'planImplementationSha256': sha(experiment_plan.__file__), 'frozenAtUTC': datetime.now(timezone.utc).isoformat(), 'selectedPairs': result['selectedPairs'], 'selectionUsesOnlySameBinaryPilot': True, 'confirmatoryEffectNotInspected': True, 'precisionLimitedProfiles': result['precisionLimitedProfiles']}
            write_new(args.freeze_output, freeze)
    else:
        require(not args.freeze_output, 'Cannot refreeze N after confirmatory effects exist')
        comparison, controls = [], []
        for (profile, group, page), cell in cells.items():
            summary = summarize(cell, alpha=experiment_plan.family(manifest)[1], replicates=replicates, engine=args.engine)
            summary['group'] = group
            (comparison if group == 'comparison' else controls).append(summary)
        require(len(comparison) == experiment_plan.family(manifest)[0], 'Incomplete primary family')
        result['comparison'], result['controls'] = comparison, controls
        result['allPrimaryUpperBoundsWithinTwoPercent'] = family_gate(comparison, experiment_plan.family(manifest)[0])
        result['gateAssessment'] = 'All prospectively registered corrected primary upper bounds <=2%; review validity, sensitivity and same-binary controls before a scoped claim.' if result['allPrimaryUpperBoundsWithinTwoPercent'] else 'Joint <=2% bound not established; distinguish inconclusive precision from regression using the reported intervals.'
        result['controlPolicy'] = 'Controls reported independently; never subtracted. Their uncertainty and temporal drift require review, not a hidden correction.'
        write_new(args.output, result)
    print(f'Wrote offline {args.phase} analysis to {args.output}; no server or build was launched.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError, StopIteration, TypeError) as error:
        print(f'Analysis refused: {error}. Preserve raw attempted-run evidence.', file=sys.stderr)
        sys.exit(1)
