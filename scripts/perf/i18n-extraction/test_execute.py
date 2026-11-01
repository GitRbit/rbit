"""Owned-process lifecycle tests use mock launchers; no real binaries or servers."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import execute as e
import plan


class FakeProcess:
    def __init__(self, metadata, pid):
        self.metadata = metadata
        self.pid = pid
        self.returncode = None
        self.interrupt_once = False
        self.killed = False
    def poll(self): return self.returncode
    def send_signal(self, _signal): pass
    def wait(self, timeout=None):
        if self.interrupt_once:
            self.interrupt_once = False
            raise KeyboardInterrupt()
        self.returncode = -9 if self.killed else 0
        Path(self.metadata + '.exit.json').write_text(json.dumps({'code': 0, 'signal': None}))
        return self.returncode
    def kill(self): self.killed = True


class OwnerLifecycle(unittest.TestCase):
    def test_sampler_failure_stops_both_owners_and_preserves_failed_attempt_without_secret_diagnostics(self):
        self.run_failure(interrupt=False)
    def test_single_interrupt_during_shutdown_still_kills_reaps_that_owner_and_settles_both(self):
        self.run_failure(interrupt=True)

    def run_failure(self, interrupt):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            seed = root / 'seed'; seed.mkdir(); (seed / 'data').write_text('same seed')
            seed_manifest = root / 'seed.json'; seed_manifest.write_text(json.dumps(plan.inventory(seed)))
            commands = []
            for i, label in enumerate(('baseline', 'candidate')):
                metadata = str(root / f'{label}.server.json')
                commands.append({'label': label, 'seedCopy': {'source': str(seed), 'destination': str(root / (label + '.data')), 'verifyCopiedManifest': str(seed_manifest), 'verificationRecord': metadata + '.seed-before-start.json'}, 'argv': ['node', 'scripts/perf/serve.mjs', '--port', str(3045+i), '--metadata', metadata], 'cwd': str(root), 'envOverrides': {}})
            block = {'block': 'synthetic', 'commands': commands, 'config': {'output': str(root / 'samples.json')}, 'measureArgv': ['node', 'inert-test-only.mjs'], 'cwd': str(root)}
            processes = []
            def launch(argv, **kwargs):
                metadata = argv[argv.index('--metadata')+1]
                # The copied full inventory must be durable evidence before any
                # process could bind a port or receive a fixture credential.
                self.assertEqual(plan.load(metadata + '.seed-before-start.json'), plan.load(seed_manifest))
                process = FakeProcess(metadata, 1000 + len(processes))
                processes.append(process)
                if interrupt and len(processes) == 2: process.interrupt_once = True
                return process
            def kill_group(pid, _signal):
                next(p for p in processes if p.pid == pid).killed = True
            protocol = {'blocks': []}
            with patch.object(e, 'require_free_ports') as free, patch.object(e, 'wait_ready'), patch.object(e.subprocess, 'Popen', side_effect=launch), patch.object(e.subprocess, 'run', side_effect=RuntimeError('UNIT_PRIVATE_COOKIE_DIAGNOSTIC')), patch.object(e.os, 'killpg', side_effect=kill_group):
                with self.assertRaisesRegex(RuntimeError, 'Attempt invalid') as error:
                    e.run_block(block, {'limits': {'readinessSeconds': 30, 'samplerSeconds': 120}}, protocol)
                free.assert_called_once_with([3045, 3046])
            self.assertEqual(len(processes), 2)
            self.assertTrue(all(p.returncode is not None for p in processes))
            self.assertEqual(len(protocol['blocks'][0]['cleanup']), 2)
            self.assertFalse(protocol['blocks'][0]['valid'])
            self.assertNotIn('UNIT_PRIVATE_COOKIE_DIAGNOSTIC', json.dumps(protocol))
            self.assertTrue(error.exception.__suppress_context__)
            if interrupt: self.assertTrue(processes[1].killed)

    def test_executor_cli_installs_sigterm_unwind_before_owned_work(self):
        handlers = {}
        def register(number, handler): handlers[number] = handler
        def begin(*_args):
            self.assertIn(e.signal.SIGTERM, handlers)
            handlers[e.signal.SIGTERM](e.signal.SIGTERM, None)
        with patch.object(__import__('sys'), 'argv', ['execute.py', '/unused', '--manifest', '/unused', '--execute-reviewed-plan']), patch.object(e.signal, 'signal', side_effect=register), patch.object(e, 'execute', side_effect=begin):
            with self.assertRaises(KeyboardInterrupt): e.main()

    def test_occupied_port_refuses_before_any_launcher_or_credentials(self):
        with patch.object(e.socket, 'socket') as socket:
            socket.return_value.bind.side_effect = OSError('occupied')
            with self.assertRaisesRegex(RuntimeError, 'occupied'):
                e.require_free_ports([3045, 3046])
            socket.return_value.close.assert_called_once()


if __name__ == '__main__': unittest.main()
