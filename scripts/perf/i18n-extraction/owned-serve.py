#!/usr/bin/env python3
"""Own a fixture server for explicit setup; never invoked during draft preparation."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import execute
import plan


def finalize_fixture(proc, metadata):
    cleanup = execute.stop(proc)
    plan.require(cleanup['forcedKill'] is False and cleanup['launcherExitCode'] == 0,
                 'Fixture setup failed: launcher cleanup was not clean')
    child = plan.load(str(metadata) + '.exit.json')
    plan.require(child['code'] == 0 and child['signal'] is None,
                 'Fixture setup failed: child shutdown was not clean')


def main():
    execute.install_termination_handler()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-record', required=True)
    parser.add_argument('--data-dir', required=True)
    parser.add_argument('--metadata', required=True)
    parser.add_argument('--port', type=int, default=3005)
    args = parser.parse_args()
    record = plan.load(args.build_record)
    record = plan.record(args.build_record, record['revision'], record['features'])
    plan.require(record['features'] == [], 'Fixture setup needs the default compiled i18n implementation')
    execute.require_free_ports([args.port])
    metadata = Path(args.metadata).resolve()
    metadata.parent.mkdir(parents=True, exist_ok=True)
    argv = ['node', 'scripts/perf/serve.mjs', '--profile', 'release', '--port', str(args.port), '--target-dir', record['targetDir'], '--data-dir', str(Path(args.data_dir).resolve()), '--metadata', str(metadata), '--source-dir', record['sourceDir'], '--source-revision', record['revision']]
    environment = {k: v for k, v in os.environ.items() if not k.startswith('RBIT_')}
    environment.update({'RUST_LOG': 'warn', 'RBIT_ACTIONS__ENABLED': 'false', 'RBIT_I18N__ENABLED': 'false', 'RBIT_SSH__ENABLED': 'false', 'RBIT_UPDATES__CHECK_FOR_UPDATES': 'false', 'RBIT_SECURITY__RATE_LIMIT': 'false', 'RBIT_MAIL__TRANSPORT': 'log'})
    with metadata.with_suffix('.launcher.log').open('x') as stream:
        proc = subprocess.Popen(argv, cwd=plan.ROOT, env=environment, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            with Path(str(metadata) + '.owner.json').open('x') as f:
                json.dump({'schema': 1, 'launcherPid': proc.pid, 'port': args.port, 'metadataPath': str(metadata), 'createdAtUTC': datetime.now(timezone.utc).isoformat()}, f)
            execute.wait_ready(proc, args.port)
            print(f'Owned fixture server ready; metadata {metadata}. No benchmark running.', flush=True)
            plan.require(proc.wait() == 0, 'Fixture setup failed: launcher exited nonzero')
        except KeyboardInterrupt:
            pass
        finally:
            finalize_fixture(proc, metadata)


if __name__ == '__main__':
    main()
