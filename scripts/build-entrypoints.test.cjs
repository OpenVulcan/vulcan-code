// Exercise actual Make/Shell/PowerShell routing with isolated, offline child commands.
// 使用隔离的离线子命令验证真实 Make、Shell 和 PowerShell 分发。
'use strict';
const assert = require('node:assert/strict');
const { test } = require('node:test');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } = require('node:fs');
const os = require('node:os');
const path = require('node:path');

// Create a test-owned checkout; t registers cleanup and the return value names the checkout/log.
// 创建测试独占的检出；t 登记清理，返回值指定检出与日志。
function fixture(t) {
  // Each fixture owns one exact temporary directory, checked before recursive cleanup.
  // 每个夹具拥有一个精确临时目录，并在递归清理前检查归属。
  const root = mkdtempSync(path.join(os.tmpdir(), 'vulcan-entrypoints-'));
  assert.equal(path.dirname(root), path.resolve(os.tmpdir()));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(path.join(root, 'scripts'));
  for (const name of ['Makefile', 'make.sh', 'make.ps1']) {
    writeFileSync(path.join(root, name), readFileSync(path.join(__dirname, '..', name), 'utf8').replace(/\r\n/g, '\n'));
  }
  // The prerequisite checker has separate behavioral tests; these fixtures isolate task dispatch.
  // 前置检查器有独立行为测试；此处夹具专门隔离任务分发。
  writeFileSync(path.join(root, 'scripts/check-build-environment.cjs'), 'process.exit(0);\n');
  for (const name of ['desktop-runtime.mjs', 'core-cli.mjs']) {
    writeFileSync(path.join(root, 'scripts', name), `import fs from 'node:fs';
fs.appendFileSync(process.env.ENTRYPOINT_LOG, ${JSON.stringify(name)} + ':' + process.argv.slice(2).join(':') + '\\n');
process.exit(Number(process.env.FAIL_RUNTIME || 0));\n`);
  }
  writeFileSync(path.join(root, 'scripts/build_tauri.sh'), '#!/bin/sh\nprintf "build:%s\\n" "$*" >> "$ENTRYPOINT_LOG"\nexit "${FAIL_BUILD:-0}"\n');
  writeFileSync(path.join(root, 'scripts/run_tauri.sh'), '#!/bin/sh\nprintf "desktop:%s\\n" "$*" >> "$ENTRYPOINT_LOG"\n');
  writeFileSync(path.join(root, 'scripts/build_tauri.ps1'), 'param([switch]$Release,[switch]$Restart)\nAdd-Content -LiteralPath $env:ENTRYPOINT_LOG -Value "build:$Release`:$Restart"\nexit ([int]$env:FAIL_BUILD)\n');
  // PowerShell-owned scripts deliberately fail if a Unix route reaches them.
  // Unix 入口若误入 PowerShell 所属脚本，夹具会明确失败。
  writeFileSync(path.join(root, 'scripts/core_debug_cycle.ps1'), 'throw "Unexpected Windows Core route"\n');
  writeFileSync(path.join(root, 'scripts/run_tauri.ps1'), 'throw "Unexpected Windows desktop route"\n');
  return { root, log: path.join(root, 'commands.log') };
}

// Run one real entrypoint in an isolated fixture; args select the public target and extra supplies failures.
// 在隔离夹具中执行真实入口；args 选择公开目标，extra 注入失败状态。
// Return the process result and recorded children, without starting an actual application.
// 返回进程结果及记录的子命令，不启动实际应用。
function invoke(owned, executable, args, extra = {}) {
  // The process receives only the fixture-owned log and optional failure switches.
  // 进程仅接收夹具拥有的日志，以及可选失败开关。
  const result = spawnSync(executable, args, {
    cwd: owned.root, encoding: 'utf8', timeout: 30_000,
    env: { ...process.env, ENTRYPOINT_LOG: owned.log, FAIL_BUILD: '0', ...extra },
  });
  assert.equal(result.error, undefined);
  // No command log is a valid result when argument validation rejects dispatch.
  // 参数校验拒绝分发时，没有命令日志也是合法结果。
  let commands = [];
  try { commands = readFileSync(owned.log, 'utf8').trim().split(/\r?\n/); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  return { ...result, commands };
}

for (const release of [false, true]) {
  test(`make bt builds before TUI (${release ? 'release' : 'debug'})`, { skip: process.platform === 'win32' }, t => {
    // The release companion must not trigger a second full build.
    // release 伴随目标不能触发第二次完整构建。
    const owned = fixture(t);
    const result = invoke(owned, 'make', release ? ['bt', 'release'] : ['bt']);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(result.commands, [`build:${release ? '--release' : ''}`, `desktop-runtime.mjs:tui:${release ? 'release' : 'debug'}`]);
  });
}

test('failed full build prevents TUI startup', { skip: process.platform === 'win32' }, t => {
  const owned = fixture(t);
  const result = invoke(owned, 'sh', ['make.sh', 'bt'], { FAIL_BUILD: '17' });
  assert.equal(result.status, 17, result.stderr);
  assert.deepEqual(result.commands, ['build:']);
});

test('TUI failure is returned to the invoking shell', { skip: process.platform === 'win32' }, t => {
  const owned = fixture(t);
  const result = invoke(owned, 'sh', ['make.sh', 'bt'], { FAIL_RUNTIME: '23' });
  assert.equal(result.status, 23, result.stderr);
});

for (const [target, command] of [['core-build', 'build'], ['core-restart', 'restart']]) {
  test(`Unix ${target} uses native Node coordination`, { skip: process.platform === 'win32' }, t => {
    const owned = fixture(t);
    const result = invoke(owned, 'make', [target]);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(result.commands, [`core-cli.mjs:${command}`]);
  });
}

for (const target of ['build', 'run', 'core-build', 'core-restart', 'bt']) {
  test(`explicit Unix PowerShell ${target} delegates to the native entry`, { skip: process.platform === 'win32' }, t => {
    const owned = fixture(t);
    const result = invoke(owned, 'pwsh', ['-NoProfile', '-File', path.join(owned.root, 'make.ps1'), target]);
    assert.equal(result.status, 0, result.stderr);
    assert.ok(result.commands.length > 0);
    assert.equal(result.commands.some(command => command.startsWith('build:True')), false);
  });
}

test('Windows bt completes its restart-capable build before TUI', { skip: process.platform !== 'win32' }, t => {
  const owned = fixture(t);
  const result = invoke(owned, 'pwsh', ['-NoProfile', '-File', path.join(owned.root, 'make.ps1'), 'bt']);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.commands, ['build:False:True', 'desktop-runtime.mjs:tui:debug']);
});

test('unsupported bt profile rejects dispatch', { skip: process.platform === 'win32' }, t => {
  const owned = fixture(t);
  const result = invoke(owned, 'sh', ['make.sh', 'bt', 'invalid']);
  assert.notEqual(result.status, 0);
  assert.deepEqual(result.commands, []);
});
