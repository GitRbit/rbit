import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtemp, mkdir, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PassThrough } from 'node:stream';
import test from 'node:test';
import { cargoProcess } from '../workspace/build.mjs';
import { cargoTreeMode, verifiedCargoCompletion } from '../workspace/cargo-tree.mjs';
import { leased } from '../workspace/lease.mjs';
import { runtimeEnvironment } from '../workspace/ports.mjs';
import { runBinary } from '../workspace/runtime.mjs';

const environment = () => Object.freeze({
  RBIT_BUILD_MANAGED_RETENTION: 'off', rbit_build_cache_bytes: '1024',
  RBIT_WORKSPACE_JSON: '1', RbIt_WoRkSpAcE_FuTuRe: 'private-control',
  RBIT_SERVER__PUBLIC_URL: 'https://example.invalid', RBIT_UNKNOWN_APP_FIELD: 'preserved',
  RBIT_BUILDING: 'real-app-name', RUSTFLAGS: '-Ctarget-cpu=x86-64',
  CARGO_TARGET_DIR: '/selected-target', CARGO_BUILD_BUILD_DIR: '/selected-intermediates',
  LIBSQLITE3_FLAGS: '-DSQLITE_ENABLE_QPSG', PATH: 'toolchain-path', CUSTOM_SECRET: 'preserved',
});
function assertChildEnv(actual, original) {
  const expected = Object.fromEntries(Object.entries(original).filter(([name]) => !/^RBIT_(BUILD|WORKSPACE)_/i.test(name)));
  assert.deepEqual(actual, expected);
  assert.notEqual(actual, original);
}
function completedCargo() {
  const child = new EventEmitter();
  child.pid = 1234;
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  setImmediate(() => {
    child.stdout.end('{"reason":"build-finished","success":true}\n');
    child.stderr.end();
    setImmediate(() => child.emit('close', 0, null));
  });
  return child;
}
async function fixture(t) {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'rbit-child-env-')));
  t.after(() => rm(root, { recursive: true, force: true }));
  const ctx = { root, checkout: root, cache: join(root, 'cache'), state: join(root, '.worktree-state', 'fixture'), id: 'fixture' };
  await mkdir(ctx.cache);
  return ctx;
}

test('actual Cargo consumer filters only reserved controls at spawn and keeps helper JSON mode', async () => {
  const env = Object.freeze({ ...environment(), RBIT_WORKSPACE_CARGO_TREE: "off" });
  let output = '';
  await cargoProcess({ checkout: '.' }, ['test'], env, () => {}, {
    spawnProcess(name, args, options) {
      assert.equal(name, 'cargo');
      assert.deepEqual(args, ['test']);
      assertChildEnv(options.env, env);
      return completedCargo();
    },
    output: { write: text => { output += text; } }, diagnostics: { write: () => assert.fail('unexpected diagnostic') },
  });
  assert.equal(JSON.parse(output).reason, 'build-finished', 'original helper JSON control is still honored');
  assert.equal(env.RBIT_BUILD_MANAGED_RETENTION, 'off', 'input environment was not mutated');
});

test('Default and explicit Windows ownership are selected before filtering the spawn environment',
  { skip: process.platform !== 'win32' }, async t => {
    const ctx = await fixture(t);
    for (const controls of [{}, { RBIT_WORKSPACE_CARGO_TREE: 'windows-job' }]) {
      const env = Object.freeze({ ...environment(), ...controls });
      assert.equal(cargoTreeMode(env), 'windows-job');
      await leased(join(ctx.cache, '.lease'), async lease => {
        let launched = false;
        const receipt = await cargoProcess(ctx, ['check'], env, () => {}, {
          lease, native: {
            resolve: async (_ctx, original) => { assert.equal(original, env); return 'cargo.exe'; },
            launch(_executable, _args, options) { launched = true; assertChildEnv(options.env, env); return completedCargo(); },
            verify: async () => true,
          },
          output: { write() {} }, diagnostics: { write() {} },
        });
        assert.ok(launched);
        assert.ok(verifiedCargoCompletion(receipt, lease));
      });
    }
  });

test('runtime environment omits helper controls and preserves real application overrides', async t => {
  const ctx = await fixture(t);
  const original = environment();
  const actual = await runtimeEnvironment(ctx, original);
  assert.equal(Object.keys(actual).some(name => /^RBIT_(BUILD|WORKSPACE)_/i.test(name)), false);
  for (const [name, value] of Object.entries(original)) {
    if (!/^RBIT_(BUILD|WORKSPACE)_/i.test(name)) assert.equal(actual[name], value);
  }
  assert.equal(actual.RBIT_SSH__ENABLED, 'false');
  assert.ok(actual.RBIT_STORAGE__DATA_DIR.startsWith(ctx.state));
  assert.equal(original.RBIT_WORKSPACE_JSON, '1');
});

test('direct managed runtime launch also filters controls without weakening application config rejection', async t => {
  const ctx = await fixture(t);
  const executable = join(ctx.state, 'bin', 'fixture', 'rbit.exe');
  await mkdir(join(ctx.state, 'bin', 'fixture'), { recursive: true });
  await writeFile(executable, 'fixture only');
  const env = environment();
  const child = new EventEmitter();
  child.pid = process.pid;
  const running = await runBinary(ctx, executable, ['serve'], env, {
    launch(_executable, args, options) {
      assert.deepEqual(args, ['serve']);
      assertChildEnv(options.env, env);
      assert.equal(options.env.RBIT_UNKNOWN_APP_FIELD, 'preserved', 'strict app validation remains authoritative');
      setImmediate(() => child.emit('spawn'));
      return child;
    }, verify: async () => true,
  });
  child.emit('close', 0, null);
  await running.runtimeCompletion;
});
