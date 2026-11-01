#!/usr/bin/env python3
"""Execute a reviewed, frozen command plan. Never builds or changes product source.

Not executed during preparation. --execute-reviewed-plan is an explicit future
invocation guard, not a substitute for validating the machine/fixture evidence.
"""
import argparse
from datetime import datetime, timezone
import http.client
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import time
import plan


def now():
    return datetime.now(timezone.utc).isoformat()


def write_protocol(path, protocol):
    temporary = path.with_suffix('.next.json')
    temporary.write_text(json.dumps(protocol, indent=2) + '\n')
    temporary.replace(path)


def wait_ready(proc, port, *, timeout=30):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if proc.poll() is not None:
            raise RuntimeError('Benchmark server exited before readiness')
        connection = http.client.HTTPConnection('127.0.0.1', port, timeout=1)
        try:
            connection.request('GET', '/healthz')
            response = connection.getresponse()
            if response.status == 200:
                response.read(1024)
                if proc.poll() is not None:
                    raise RuntimeError('Owned launcher exited during readiness')
                return
        except OSError:
            pass
        finally:
            connection.close()
        time.sleep(.05)
    raise RuntimeError('Benchmark readiness deadline exceeded; no redirect or retry block')


def require_free_ports(ports):
    reservations = []
    try:
        for port in ports:
            listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            reservations.append(listener)
            listener.bind(('127.0.0.1', port))
    except OSError:
        raise RuntimeError('A registered loopback port is occupied; refuse fixture credentials') from None
    finally:
        for listener in reservations:
            listener.close()


def stop(proc):
    forced = False
    if proc.poll() is None:
        try:
            proc.send_signal(signal.SIGTERM)
            proc.wait(timeout=30)
        except BaseException as failure:
            # A single interrupt during graceful wait must not leave an owned
            # server alive. Kill/reap before propagating interruption/invalidity.
            forced = True
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                proc.wait(timeout=30)
            except BaseException:
                try:
                    proc.kill()
                except ProcessLookupError:
                    pass
                proc.wait(timeout=30)
            if not isinstance(failure, subprocess.TimeoutExpired):
                raise failure
    return {'forcedKill': forced, 'launcherExitCode': proc.returncode}


def run_block(block, manifest, protocol):
    running, cleanup = [], []
    error = None
    try:
        require_free_ports([int(c['argv'][c['argv'].index('--port') + 1]) for c in block['commands']])
        for command in block['commands']:
            copy = command['seedCopy']
            source, destination = Path(copy['source']), Path(copy['destination'])
            plan.require(not destination.exists(), 'Data directory reuse refused')
            plan.checked_tree(source, copy['verifyCopiedManifest'])
            shutil.copytree(source, destination)
            copied = plan.inventory(destination)
            plan.require(copied == plan.load(copy['verifyCopiedManifest']), 'Copied seed content differs')
            verification = Path(copy['verificationRecord'])
            with verification.open('x') as f:
                json.dump(copied, f, sort_keys=True)
                f.write('\n')
            # No shell interpolation; credentials exist only in private files and
            # the HTTP client's in-memory jar, never argv or server environment.
            environment = {k: v for k, v in os.environ.items() if not k.startswith('RBIT_')}
            environment.update(command['envOverrides'])
            log = Path(block['config']['output']).parent / f'{command["label"]}.launcher.log'
            stream = log.open('x')
            try:
                proc = subprocess.Popen(command['argv'], cwd=command['cwd'], env=environment,
                                        stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
            except BaseException:
                stream.close()
                raise
            running.append((proc, stream, command))
            metadata = command['argv'][command['argv'].index('--metadata') + 1]
            port = int(command['argv'][command['argv'].index('--port') + 1])
            with Path(metadata + '.owner.json').open('x') as owner:
                json.dump({'schema': 1, 'launcherPid': proc.pid, 'port': port, 'metadataPath': str(Path(metadata).resolve()), 'createdAtUTC': now()}, owner)
            wait_ready(proc, port, timeout=manifest['limits']['readinessSeconds'])
        log = Path(block['config']['output']).parent / 'measure.log'
        with log.open('x') as stream:
            subprocess.run(block['measureArgv'], cwd=block['cwd'], env=environment,
                           stdout=stream, stderr=subprocess.STDOUT, check=True, timeout=manifest['limits']['samplerSeconds'])
    except BaseException as failure:
        error = failure
    finally:
        # Always settle BOTH launchers even if the first shutdown or sampler fails.
        for proc, stream, command in reversed(running):
            try:
                outcome = stop(proc)
                metadata = command['argv'][command['argv'].index('--metadata') + 1]
                exit_record = plan.load(metadata + '.exit.json')
                good = not outcome['forcedKill'] and outcome['launcherExitCode'] == 0 and exit_record['code'] == 0 and exit_record['signal'] is None
                cleanup.append({'label': command['label'], **outcome, 'clean': good})
                if not good and error is None:
                    error = RuntimeError('Benchmark server did not exit cleanly')
            except BaseException as failure:
                cleanup.append({'label': command['label'], 'clean': False})
                if error is None:
                    error = failure
            finally:
                stream.close()
    entry = {'block': block['block'], 'endedAtUTC': now(), 'cleanup': cleanup, 'valid': error is None}
    protocol['blocks'].append(entry)
    if error is not None:
        # Exception strings can contain third-party diagnostic text; keep the
        # public protocol static. Private launcher/server/measure logs are retained.
        raise RuntimeError('Attempt invalid; raw evidence retained, no replacement or favorable stopping') from None


def execute(plan_file, manifest_file):
    specification = plan.load(plan_file)
    manifest = plan.load(manifest_file)
    plan.require(specification['schema'] == 2, 'Superseded plan schema')
    plan.require(specification['manifestSha256'] == plan.digest(manifest_file), 'Manifest changed since planning')
    plan.require(specification['planScriptSha256'] == plan.digest(plan.__file__), 'Planner changed since generation')
    for relative, expected in specification['harnessFiles'].items():
        plan.require(plan.digest(plan.ROOT / relative) == expected, 'Harness changed since generation')
    plan.require(manifest['limits'] == {'readinessSeconds': 30, 'samplerSeconds': 120, 'shutdownSeconds': 30}, 'Changed reviewed executor deadlines')
    active_profiles = [p for p in manifest['profiles'] if specification['phase'] != 'smoke' or p['name'] == 'i18n-enabled']
    for profile in active_profiles:
        for arm in ('A', 'B'):
            revision = manifest['baselineRevision' if arm == 'A' else 'candidateRevision']
            plan.require(plan.record(profile['artifacts'][arm], revision, profile['features']) == specification['artifacts'][profile['name']][arm], 'Build input changed since planning')
    for role, evidence in specification['inputEvidence'].items():
        plan.require(evidence['path'] == manifest[role] and plan.digest(evidence['path']) == evidence['sha256'], 'Manual prerequisites changed')
    selected = None
    if specification['phase'] == 'confirmatory':
        import analysis
        selected, _, _, _ = analysis.frozen_selection(manifest, manifest_file)
    expected = [(profile['name'], phase) for profile in active_profiles for phase, _, _ in plan.phase_schedule(specification['phase'], None if selected is None else selected[profile['name']])]
    plan.require([(block['profile'], block['phase']) for block in specification['blocks']] == expected, 'Plan does not contain the complete frozen phase order')
    protocol_file = Path(plan_file).resolve().parent / 'protocol.json'
    with protocol_file.open('x') as f:
        json.dump({'schema': 2, 'complete': False}, f)
    protocol = {'schema': 2, 'phase': specification['phase'], 'startedAtUTC': now(), 'planSha256': plan.digest(plan_file), 'executorSha256': plan.digest(__file__), 'complete': False, 'valid': False, 'blocks': []}
    write_protocol(protocol_file, protocol)
    try:
        for block in specification['blocks']:
            run_block(block, manifest, protocol)
            write_protocol(protocol_file, protocol)
    except BaseException:
        protocol['endedAtUTC'] = now()
        write_protocol(protocol_file, protocol)
        raise
    protocol.update({'endedAtUTC': now(), 'complete': True, 'valid': True})
    write_protocol(protocol_file, protocol)


def install_termination_handler():
    def interrupted(_signal, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)


def main():
    install_termination_handler()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan')
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--execute-reviewed-plan', action='store_true', required=True)
    args = parser.parse_args()
    execute(args.plan, args.manifest)


if __name__ == '__main__':
    main()
