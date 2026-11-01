"""Fixture finalization outcomes, with no subprocess or live server."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch, Mock
sys.path.insert(0, str(Path(__file__).resolve().parent))
spec = importlib.util.spec_from_file_location('owned_serve', Path(__file__).with_name('owned-serve.py'))
owned = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owned)


class FixtureFinalization(unittest.TestCase):
    def test_clean_exit_requires_clean_launcher_and_actual_child_exit_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata = Path(directory) / 'server.json'
            Path(str(metadata) + '.exit.json').write_text(json.dumps({'code': 0, 'signal': None}))
            with patch.object(owned.execute, 'stop', return_value={'forcedKill': False, 'launcherExitCode': 0}) as stop:
                owned.finalize_fixture(object(), metadata)
                stop.assert_called_once()
                for child in ({'code': 1, 'signal': None}, {'code': None, 'signal': 'SIGKILL'}):
                    Path(str(metadata) + '.exit.json').write_text(json.dumps(child))
                    with self.assertRaisesRegex(ValueError, 'child shutdown'):
                        owned.finalize_fixture(object(), metadata)

    def test_normal_nonzero_wait_propagates_even_if_final_cleanup_reports_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata = Path(directory) / 'server.json'
            record = {'revision': 'a'*40, 'features': [], 'targetDir': '/unused', 'sourceDir': '/unused'}
            proc = Mock(pid=1234)
            proc.wait.return_value = 7
            argv = ['owned-serve.py', '--build-record', '/unused', '--data-dir', directory, '--metadata', str(metadata)]
            with patch.object(sys, 'argv', argv), patch.object(owned.plan, 'load', return_value=record), patch.object(owned.plan, 'record', return_value=record), patch.object(owned.execute, 'require_free_ports'), patch.object(owned.execute, 'wait_ready'), patch.object(owned.subprocess, 'Popen', return_value=proc), patch.object(owned, 'finalize_fixture') as finalize, patch.object(owned.execute.signal, 'signal'), patch('builtins.print'):
                with self.assertRaisesRegex(ValueError, 'launcher exited nonzero'):
                    owned.main()
                finalize.assert_called_once_with(proc, metadata.resolve())

    def test_sigterm_during_readiness_unwinds_to_owned_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata = Path(directory) / 'server.json'
            record = {'revision': 'a'*40, 'features': [], 'targetDir': '/unused', 'sourceDir': '/unused'}
            proc = Mock(pid=1234)
            argv = ['owned-serve.py', '--build-record', '/unused', '--data-dir', directory, '--metadata', str(metadata)]
            handlers = {}
            def register(number, handler): handlers[number] = handler
            def launch(*_args, **_kwargs):
                self.assertIn(owned.execute.signal.SIGTERM, handlers)
                return proc
            def wait_ready(*_args): handlers[owned.execute.signal.SIGTERM](owned.execute.signal.SIGTERM, None)
            with patch.object(sys, 'argv', argv), patch.object(owned.plan, 'load', return_value=record), patch.object(owned.plan, 'record', return_value=record), patch.object(owned.execute, 'require_free_ports'), patch.object(owned.execute, 'wait_ready', side_effect=wait_ready), patch.object(owned.subprocess, 'Popen', side_effect=launch), patch.object(owned, 'finalize_fixture') as finalize, patch.object(owned.execute.signal, 'signal', side_effect=register):
                owned.main()
                finalize.assert_called_once_with(proc, metadata.resolve())
                proc.wait.assert_not_called()

    def test_forced_or_nonzero_launcher_cannot_label_fixture_clean(self):
        for cleanup in ({'forcedKill': True, 'launcherExitCode': 0}, {'forcedKill': False, 'launcherExitCode': 1}):
            with patch.object(owned.execute, 'stop', return_value=cleanup):
                with self.assertRaisesRegex(ValueError, 'launcher cleanup'):
                    owned.finalize_fixture(object(), '/not-read-on-unclean-launcher')


if __name__ == '__main__': unittest.main()
