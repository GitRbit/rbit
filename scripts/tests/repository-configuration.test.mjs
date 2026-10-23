import assert from 'node:assert/strict';
import test from 'node:test';
import { requiredChecks, validateConfiguration, validateRelease } from '../audit-repository.mjs';

function configuration() {
  return {
    repository: { private: false, archived: false, default_branch: 'main', has_issues: true,
      has_discussions: true, homepage: 'https://rbit.ori.click', allow_merge_commit: false },
    reporting: { enabled: true }, labels: [{ name: 'bug' }, { name: 'enhancement' }],
    rules: [
      ...['deletion', 'non_fast_forward', 'required_linear_history'].map(type => ({ type })),
      { type: 'pull_request', parameters: { required_review_thread_resolution: true } },
      { type: 'required_status_checks', parameters: { strict_required_status_checks_policy: true,
        required_status_checks: requiredChecks.map(context => ({ context, integration_id: 15368 })) } },
    ],
  };
}

test('accepts configured upstream', () => validateConfiguration(configuration()));
test('detects reporting disabled even when the policy exists', () => {
  const input = configuration();
  input.reporting.enabled = false;
  assert.throws(() => validateConfiguration(input), /Private vulnerability reporting/);
});
test('rejects missing branch protection and untrusted status producers', () => {
  const input = configuration();
  input.rules = [];
  assert.throws(() => validateConfiguration(input), /Missing active main rule/);
  const other = configuration();
  other.rules.at(-1).parameters.required_status_checks[0].integration_id = null;
  assert.throws(() => validateConfiguration(other), /Missing required GitHub Actions check/);
});
test('rejects missing issue labels and broken homepage presentation', () => {
  const input = configuration();
  input.labels = [];
  assert.throws(() => validateConfiguration(input), /Missing issue-form label/);
  input.repository.homepage = 'rbit.ori.click';
  assert.throws(() => validateConfiguration(input), /Homepage needs its HTTPS URL/);
});
test('requires every documented release target and checksums', () => {
  const assets = ['aarch64-apple-darwin.tar.gz', 'x86_64-apple-darwin.tar.gz',
    'aarch64-unknown-linux-gnu.tar.gz', 'x86_64-unknown-linux-gnu.tar.gz',
    'x86_64-pc-windows-msvc.zip'].map(target => ({ name: `rbit-0.1.3-${target}` }));
  const release = { draft: false, prerelease: false, tag_name: 'v0.1.3', assets };
  assert.throws(() => validateRelease(release), /Release needs checksums/);
  assets.push({ name: 'SHA256SUMS' });
  validateRelease(release);
  assets.shift();
  assert.throws(() => validateRelease(release), /Missing release target/);
});
