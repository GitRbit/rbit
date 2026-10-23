// Read-only checks for the public upstream and its submission paths.
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

export const requiredChecks = [
  'Format and clippy', 'Licenses and advisories', 'Tests (ubuntu-latest)',
  'Tests (windows-latest)', 'Front-end build', 'Release build',
];

export function validateConfiguration({ repository, reporting, rules, labels }) {
  assert.equal(repository.private, false, 'Upstream must be public');
  assert.equal(repository.archived, false, 'Upstream must accept contributions');
  assert.equal(repository.default_branch, 'main', 'Documentation assumes main');
  assert.equal(repository.has_issues, true, 'Issue forms require Issues');
  assert.equal(repository.has_discussions, true, 'Community links require Discussions');
  assert.equal(repository.homepage, 'https://rbit.ori.click', 'Homepage needs its HTTPS URL');
  assert.equal(reporting.enabled, true, 'Private vulnerability reporting is disabled');
  for (const name of ['bug', 'enhancement']) {
    assert.ok(labels.some(label => label.name === name), `Missing issue-form label: ${name}`);
  }
  for (const type of ['deletion', 'non_fast_forward', 'required_linear_history', 'pull_request']) {
    assert.ok(rules.some(rule => rule.type === type), `Missing active main rule: ${type}`);
  }
  const review = rules.find(rule => rule.type === 'pull_request').parameters;
  assert.equal(review.required_review_thread_resolution, true, 'Review threads must be resolved');
  const statusRules = rules.filter(rule => rule.type === 'required_status_checks');
  assert.ok(statusRules.some(rule => rule.parameters.strict_required_status_checks_policy),
    'Required checks must run against an up-to-date branch');
  const checks = statusRules.flatMap(rule => rule.parameters.required_status_checks);
  for (const context of requiredChecks) {
    assert.ok(checks.some(check => check.context === context && check.integration_id === 15368),
      `Missing required GitHub Actions check: ${context}`);
  }
}

export function validateRelease(release) {
  assert.equal(release.draft, false);
  assert.equal(release.prerelease, false);
  const version = release.tag_name.replace(/^v/, '');
  const names = new Set(release.assets.map(asset => asset.name));
  for (const target of ['aarch64-apple-darwin', 'x86_64-apple-darwin',
    'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc']) {
    const extension = target.includes('windows') ? 'zip' : 'tar.gz';
    assert.ok(names.has(`rbit-${version}-${target}.${extension}`), `Missing release target: ${target}`);
  }
  assert.ok(names.has('SHA256SUMS'), 'Release needs checksums');
}

async function run() {
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
  const admin = process.argv.includes('--admin');
  if (admin) assert.ok(token, '--admin requires GH_TOKEN or GITHUB_TOKEN');
  const headers = { Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' };
  if (token) headers.Authorization = `Bearer ${token}`;
  async function api(path) {
    const response = await fetch(`https://api.github.com/repos/GitRbit/rbit${path}`, {
      headers, signal: AbortSignal.timeout(30_000),
    });
    assert.ok(response.ok, `GitHub ${path || '/'}: HTTP ${response.status}`);
    return response.status === 204 ? null : response.json();
  }
  const [repository, reporting, rules, labels, release] = await Promise.all([
    api(''), api('/private-vulnerability-reporting'), api('/rules/branches/main'),
    api('/labels?per_page=100'), api('/releases/latest'),
  ]);
  validateConfiguration({ repository, reporting, rules, labels });
  console.log('PASS public metadata, private reporting, labels and active main protection');
  validateRelease(release);
  console.log(`PASS ${release.tag_name} has five platform archives and checksums`);

  // Inspect actual public entry points without creating issues, discussions or reports.
  for (const [path, marker] of [
    ['security/advisories', 'Report a vulnerability'],
    ['discussions', 'Discussions'],
    ['issues/new/choose', 'Sign in'],
    ['security/advisories/new', 'Sign in'],
  ]) {
    const response = await fetch(`https://github.com/GitRbit/rbit/${path}`, {
      signal: AbortSignal.timeout(30_000),
    });
    assert.ok(response.ok, `${path}: HTTP ${response.status}`);
    assert.ok((await response.text()).includes(marker), `${path}: missing ${marker}`);
    console.log(`PASS anonymous entry point: ${path}`);
  }
  const sums = release.assets.find(asset => asset.name === 'SHA256SUMS');
  const download = await fetch(sums.browser_download_url, { signal: AbortSignal.timeout(30_000) });
  assert.ok(download.ok, `Anonymous checksum download: HTTP ${download.status}`);
  const checksumText = await download.text();
  for (const asset of release.assets.filter(asset => asset.name !== 'SHA256SUMS')) {
    const digest = asset.digest?.replace(/^sha256:/, '');
    assert.match(digest ?? '', /^[a-f0-9]{64}$/, `Missing SHA-256 digest: ${asset.name}`);
    assert.ok(checksumText.split(/\r?\n/).some(line => line === `${digest}  ${asset.name}`),
      `Published checksum disagrees with GitHub asset digest: ${asset.name}`);
  }
  console.log('PASS anonymous checksums match all five published asset digests');
  const registryTokenResponse = await fetch('https://ghcr.io/token?scope=repository:gitrbit/rbit:pull&service=ghcr.io', {
    signal: AbortSignal.timeout(30_000),
  });
  assert.ok(registryTokenResponse.ok, `Anonymous GHCR token: HTTP ${registryTokenResponse.status}`);
  const registryToken = await registryTokenResponse.json();
  const imageResponse = await fetch('https://ghcr.io/v2/gitrbit/rbit/manifests/latest', {
    headers: { Authorization: `Bearer ${registryToken.token}`,
      Accept: 'application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json' },
    signal: AbortSignal.timeout(30_000),
  });
  assert.ok(imageResponse.ok, `Anonymous GHCR manifest: HTTP ${imageResponse.status}`);
  const image = await imageResponse.json();
  for (const architecture of ['amd64', 'arm64']) {
    assert.ok(image.manifests?.some(manifest => manifest.platform?.os === 'linux'
      && manifest.platform.architecture === architecture), `Missing public Linux image: ${architecture}`);
  }
  console.log('PASS anonymous container access for linux/amd64 and linux/arm64');
  if (admin) {
    assert.equal(repository.allow_merge_commit, false, 'Contribution policy requires linear history');
    for (const feature of ['secret_scanning', 'secret_scanning_push_protection', 'dependabot_security_updates']) {
      assert.equal(repository.security_and_analysis?.[feature]?.status, 'enabled', `Disabled: ${feature}`);
    }
    await api('/vulnerability-alerts');
    assert.equal((await api('/automated-security-fixes')).enabled, true);
    const permissions = await api('/actions/permissions/workflow');
    assert.equal(permissions.default_workflow_permissions, 'read');
    assert.equal(permissions.can_approve_pull_request_reviews, false);
    const subscription = await api('/subscription');
    assert.equal(subscription.subscribed, true, 'Auditing maintainer must watch the repository');
    assert.equal(subscription.ignored, false);
    const tagRulesets = (await api('/rulesets')).filter(ruleset =>
      ruleset.target === 'tag' && ruleset.enforcement === 'active');
    const tagRules = await Promise.all(tagRulesets.map(ruleset => api(`/rulesets/${ruleset.id}`)));
    assert.ok(tagRules.some(ruleset => ruleset.conditions?.ref_name?.include?.includes('refs/tags/v*')
      && ruleset.conditions.ref_name.exclude.length === 0
      && ruleset.rules.some(rule => rule.type === 'deletion')
      && ruleset.rules.some(rule => rule.type === 'non_fast_forward')), 'Release tags need active protection');
    console.log('PASS administrator security settings and least-privilege workflow defaults');
  }
  console.log('Notification delivery and conduct-reporting contact require separate maintainer verification.');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  run().catch(error => { console.error(`FAIL ${error.message}`); process.exitCode = 1; });
}
