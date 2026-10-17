// Test terminal coordination with disposable processes and fixtures, never real user services.
// 使用可丢弃进程及夹具测试终端协调，绝不访问真实用户服务。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { buildCore, ensureHeadlessCore, stopLinuxCore } from './core-cli.mjs';
import { checkBuildEnvironment } from './check-build-environment.cjs';
import { createRequire } from 'node:module';

// Read the current version authority instead of hard-coding a second prerequisite version.
// 读取当前版本权威，避免另行硬编码前置版本。
const require = createRequire(import.meta.url);
const versions = require('../crates/vulcan-service-runtime/managed-runtimes.json');

// Create one owned temporary directory; t releases it after any child cleanup registered later.
// 创建一个自有临时目录；t 在后续登记的子进程清理之后释放目录。
async function directory(t) {
  // The exact path is verified before registering recursive removal.
  // 登记递归删除前验证精确路径。
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-core-cli-'));
  assert.equal(path.dirname(root), path.resolve(os.tmpdir()));
  t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}

test('Unix Core-only routes need neither pnpm nor PowerShell; bt retains full-build prerequisites', () => {
  for (const platform of ['linux', 'darwin']) {
    for (const mode of ['core-build', 'core-restart']) {
      assert.deepEqual(checkBuildEnvironment({ platform, mode, nodeVersion: versions.node_version,
        probe: () => { throw new Error('Unexpected GUI/PowerShell prerequisite'); } }), []);
    }
    assert.deepEqual(checkBuildEnvironment({ platform, mode: 'bt', nodeVersion: versions.node_version,
      probe: () => ({ error: { code: 'ENOENT' } }) }).map(issue => issue.tool), ['pnpm']);
  }
});

for (const scenario of ['valid', 'failed', 'missing-artifact', 'malformed']) {
  test(`Cargo artifact stream: ${scenario}`, { skip: process.platform !== 'linux' }, async t => {
    // The fake Cargo never compiles or downloads; it emits the real message protocol shape.
    // 假 Cargo 不编译或下载；它仅输出真实消息协议形状。
    const root = await directory(t);
    const artifact = path.join(root, 'reported-core');
    await writeFile(artifact, 'artifact');
    const messages = [];
    if (scenario !== 'missing-artifact') messages.push({ reason: 'compiler-artifact', target: { name: 'vulcan-code-service', kind: ['bin'] }, executable: artifact });
    messages.push({ reason: 'build-finished', success: scenario !== 'failed' });
    await writeFile(path.join(root, 'cargo'), `#!/usr/bin/env node\n${scenario === 'malformed' ? "console.log('invalid-json');" : messages.map(message => `console.log(${JSON.stringify(JSON.stringify(message))});`).join('\n')}\n`, { mode: 0o755 });
    // PATH changes are restored even when protocol parsing fails.
    // 即使协议解析失败，也恢复 PATH 改动。
    const previous = process.env.PATH;
    process.env.PATH = `${root}${path.delimiter}${previous}`;
    try {
      if (scenario === 'valid') assert.equal(await buildCore(root), await realpath(artifact));
      else await assert.rejects(buildCore(root));
    } finally { process.env.PATH = previous; }
  });
}

// The health reader is injected at this coordination boundary; production imports the packaged reader.
// 健康读取器在此协调边界注入；生产环境导入打包后的读取器。
test('a starting owner is awaited and then reused without spawning another Core', async () => {
  let probes = 0;
  const instance = { pid: 123, status: 'ready' };
  const context = { description: { discoveryPath: 'fixture-only' }, launcher: {
    readLiveServiceInstance: async () => ++probes < 3 ? { ...instance, status: 'starting' } : instance,
  } };
  assert.deepEqual(await ensureHeadlessCore(context), instance);
  assert.equal(probes, 3);
});

test('failed owner is not reused or replaced by a competing process', async () => {
  const context = { description: { discoveryPath: 'fixture-only' }, launcher: {
    readLiveServiceInstance: async () => ({ pid: 123, status: 'failed' }),
  } };
  await assert.rejects(ensureHeadlessCore(context), /failed/);
});

test('an absent service executable produces a bounded launch failure', async t => {
  const root = await directory(t);
  const context = { root, servicePath: path.join(root, 'absent-core'), env: process.env,
    description: { discoveryPath: path.join(root, 'discovery.json') },
    launcher: { readLiveServiceInstance: async () => null } };
  await assert.rejects(ensureHeadlessCore(context), /exited before readiness/);
});

test('new terminal owners receive only headless launch flags', { skip: process.platform !== 'linux' }, async t => {
  const root = await directory(t);
  const servicePath = path.join(root, 'core');
  const argumentsPath = path.join(root, 'arguments');
  await writeFile(servicePath, '#!/bin/sh\nprintf "%s\\n" "$@" > "$CORE_ARGUMENTS_PATH"\nexit 17\n', { mode: 0o755 });
  const context = { root, servicePath, env: { ...process.env, CORE_ARGUMENTS_PATH: argumentsPath },
    description: { discoveryPath: path.join(root, 'discovery.json') },
    launcher: { readLiveServiceInstance: async () => null } };
  await assert.rejects(ensureHeadlessCore(context), /exited before readiness/);
  assert.deepEqual((await readFile(argumentsPath, 'utf8')).trim().split('\n'), ['--headless', '--no-debug-console']);
});

for (const foreign of [false, true]) {
  test(`Linux stop ${foreign ? 'rejects a foreign executable' : 'targets only its exact Core owner'}`, { skip: process.platform !== 'linux' }, async t => {
    const root = await directory(t);
    // This child is disposable and inherits no user configuration or service inputs.
    // 此子进程可丢弃，不继承用户配置或服务输入。
    const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' });
    const exited = new Promise(resolve => child.once('exit', resolve));
    await new Promise((resolve, reject) => { child.once('spawn', resolve); child.once('error', reject); });
    const discoveryPath = path.join(root, 'discovery.json');
    await writeFile(discoveryPath, JSON.stringify({ pid: child.pid }));
    const context = { description: { discoveryPath }, servicePath: foreign ? path.join(root, 'other-core') : await realpath(process.execPath) };
    try {
      if (foreign) {
        await assert.rejects(stopLinuxCore(context), /Another executable/);
        assert.equal(child.exitCode, null);
        assert.equal(child.signalCode, null);
      } else {
        await stopLinuxCore(context);
        await exited;
      }
    } finally {
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
      await exited;
    }
    await delay(0);
  });
}
