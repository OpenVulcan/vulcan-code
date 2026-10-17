// Verify environment diagnostics without downloading tools, running Cargo, or changing user settings.
// 验证环境诊断，不下载工具、不运行Cargo、不修改用户设置。
'use strict';
const assert = require('node:assert/strict');
const { test } = require('node:test');
const { spawnSync } = require('node:child_process');
const { copyFileSync, mkdirSync, mkdtempSync, existsSync, readFileSync, writeFileSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { checkBuildEnvironment, versionAtLeast } = require('./check-build-environment.cjs');
const versions = require('../crates/vulcan-service-runtime/managed-runtimes.json');

// Return declared valid tool versions; command identifies the probe and options describe process flags.
// 返回声明的有效工具版本；command标识探针，options描述进程参数。
function validProbe(command, argumentsList, options) {
  assert.deepEqual(argumentsList, command === 'cmd.exe' ? ['/d', '/s', '/c', 'pnpm --version'] : ['--version']);
  assert.ok(Number.isFinite(options.timeout) && options.timeout > 0);
  return { status: 0, stdout: ['pnpm', 'cmd.exe'].includes(command) ? versions.pnpm_version : 'PowerShell 7.0.0\n' };
}

// Stable newer versions are compatible; older or malformed versions must fail explicitly.
// 较新的稳定版本保持兼容；旧版或格式错误的版本必须明确失败。
test('compares stable tool versions numerically', () => {
  assert.equal(versionAtLeast('14.17.0', versions.node_version), false);
  assert.equal(versionAtLeast(versions.node_version, versions.node_version), true);
  assert.equal(versionAtLeast('100.0.0', versions.node_version), true);
  assert.equal(versionAtLeast('not-a-version', versions.node_version), false);
});

// All supported host families share one version policy and preserve Windows pnpm launch semantics.
// 所有宿主平台族共用版本策略，并保留Windows启动pnpm的语义。
test('accepts valid prerequisites on Windows, Linux, and macOS', () => {
  for (const platform of ['win32', 'linux', 'darwin']) {
    assert.deepEqual(checkBuildEnvironment({ mode: 'init-system', platform, nodeVersion: versions.node_version,
      probe: (command, argumentsList, options) => {
        assert.equal(options.shell, false);
        assert.equal(command === 'cmd.exe', platform === 'win32' && argumentsList.length > 1);
        return validProbe(command, argumentsList, options);
      } }), []);
  }
});

// The real server's legacy version must be diagnosed without importing fs/promises.cp.
// 真实服务器的旧版本必须被诊断，且不会导入fs/promises.cp。
test('reports the observed Node 14 incompatibility before build modules load', () => {
  const issues = checkBuildEnvironment({ mode: 'br', platform: 'linux', nodeVersion: '14.17.0', probe: validProbe });
  assert.deepEqual(issues.map(issue => [issue.tool, issue.code]), [['node', 'unsupported_version']]);
});

// A fresh server receives all missing prerequisites together instead of failing one import at a time.
// 新服务器会一次收到全部缺失前置项，而非逐次在模块导入处失败。
test('reports missing pnpm and PowerShell together during initialization', () => {
  const issues = checkBuildEnvironment({ mode: 'init-system', platform: 'linux', nodeVersion: versions.node_version,
    probe: () => ({ error: { code: 'ENOENT' } }) });
  assert.deepEqual(issues.map(issue => [issue.tool, issue.code]), [['pnpm', 'missing'], ['pwsh', 'missing']]);
});

// Executable permission errors are not reported as absent tools or unsupported Node APIs.
// 可执行权限错误不能被报告成工具缺失或Node API不支持。
test('distinguishes permission failures from missing commands', () => {
  const issues = checkBuildEnvironment({ mode: 'br', platform: 'linux', nodeVersion: versions.node_version,
    probe: () => ({ error: { code: 'EACCES' } }) });
  assert.deepEqual(issues.map(issue => [issue.tool, issue.code]), [['pnpm', 'not_executable']]);
});

// Version text cannot hide a failing process, and an older successful process still fails validation.
// 版本文本不能掩盖进程失败；成功退出的旧版进程也必须校验失败。
test('rejects old versions and unsuccessful version probes', () => {
  for (const [status, stdout, code] of [[0, '0.0.0', 'unsupported_version'], [1, versions.pnpm_version, 'probe_failed']]) {
    const issues = checkBuildEnvironment({ mode: 'br', platform: 'linux', nodeVersion: versions.node_version,
      probe: () => ({ status, stdout }) });
    assert.equal(issues[0].code, code);
  }
});

// Rust-only setup and unknown routes retain their original dispatch without new Node/pnpm probes.
// 纯Rust初始化与未知入口保留原有分发行为，不新增Node或pnpm探测。
test('does not block Rust-only setup or replace unknown-command diagnostics', () => {
  for (const options of [{ mode: 'deps', variant: 'rust-tools' }, { mode: 'unknown' }]) {
    assert.deepEqual(checkBuildEnvironment({ ...options, nodeVersion: '14.17.0',
      probe: () => { throw new Error('Unexpected tool probe'); } }), []);
  }
});

// Exercise the real host router in an isolated checkout; prerequisite failure must prevent build execution.
// 在隔离检出中执行真实宿主路由；前置检查失败必须阻止构建执行。
test('make router stops before the build script when a prerequisite cannot run', t => {
  const temporary = mkdtempSync(path.join(tmpdir(), 'vulcan make prerequisites-'));
  assert.ok(path.resolve(temporary).startsWith(path.resolve(tmpdir()) + path.sep));
  t.after(() => rmSync(temporary, { recursive: true, force: true }));
  const repository = path.resolve(__dirname, '..');
  const windows = process.platform === 'win32';
  const entry = windows ? 'make.ps1' : 'make.sh';
  mkdirSync(path.join(temporary, 'scripts'), { recursive: true });
  mkdirSync(path.join(temporary, 'crates/vulcan-service-runtime'), { recursive: true });
  for (const name of [entry, 'scripts/check-build-environment.cjs', 'crates/vulcan-service-runtime/managed-runtimes.json']) {
    copyFileSync(path.join(repository, name), path.join(temporary, name));
  }
  writeFileSync(path.join(temporary, windows ? 'pnpm.cmd' : 'pnpm'), windows ? '@exit /b 1\r\n' : '#!/bin/sh\nexit 1\n', { mode: 0o755 });
  writeFileSync(path.join(temporary, 'scripts', windows ? 'build_tauri.ps1' : 'build_tauri.sh'),
    windows ? "Set-Content -LiteralPath (Join-Path $PSScriptRoot '../build-was-started') -Value started\n" : '#!/bin/sh\ntouch build-was-started\n');
  const result = spawnSync(windows ? 'pwsh' : 'sh', windows ? ['-NoProfile', '-File', path.join(temporary, entry), 'br'] : [path.join(temporary, entry), 'br'],
    { cwd: temporary, encoding: 'utf8', timeout: 30000, env: { ...process.env, PATH: temporary + path.delimiter + process.env.PATH } });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.equal(existsSync(path.join(temporary, 'build-was-started')), false);
});

// Exercise the complete Unix initialization dispatch with offline child fixtures; t owns cleanup and failures stop later steps.
// 使用离线子程序夹具验证完整 Unix 初始化分发；t 管理清理，失败必须阻止后续步骤。
for (const failure of ['', 'linux-system', 'protoc']) {
  test(`Unix initialization preserves dependency order and stops on ${failure || 'no failure'}`,
    { skip: process.platform === 'win32' }, t => {
      // temporary and log are owned by this test; native installers and downloads never run.
      // temporary 与 log 归本测试拥有；不运行真实安装器或下载。
      const temporary = mkdtempSync(path.join(tmpdir(), 'vulcan init order-'));
      assert.equal(path.dirname(path.resolve(temporary)), path.resolve(tmpdir()));
      t.after(() => rmSync(temporary, { recursive: true, force: true }));
      const log = path.join(temporary, 'initialization.log');
      const repository = path.resolve(__dirname, '..');
      mkdirSync(path.join(temporary, 'scripts'), { recursive: true });
      mkdirSync(path.join(temporary, 'crates/vulcan-service-runtime'), { recursive: true });
      for (const name of ['make.sh', 'scripts/check-build-environment.cjs', 'crates/vulcan-service-runtime/managed-runtimes.json']) {
        copyFileSync(path.join(repository, name), path.join(temporary, name));
      }
      // Version probes succeed, while actual PowerShell dispatch records its explicit dependency kind.
      // 版本探针成功；实际 PowerShell 分发则记录其明确的依赖类别。
      writeFileSync(path.join(temporary, 'pnpm'), `#!/bin/sh\necho '${versions.pnpm_version}'\n`, { mode: 0o755 });
      writeFileSync(path.join(temporary, 'pwsh'), '#!/bin/sh\nif [ "$1" = "--version" ]; then echo "PowerShell 7.0.0"; exit 0; fi\nprevious=\nfor argument do\nif [ "$previous" = "deps" ]; then printf "%s\\n" "$argument" >> "$VULCAN_INIT_LOG"; exit 0; fi\nprevious=$argument\ndone\nexit 9\n', { mode: 0o755 });
      for (const [script, label] of [['install_linux_system_deps.sh', 'linux-system'], ['install_rust_dev_tools.sh', 'rust-tools'], ['install_bundled_tools.sh', 'tools']]) {
        writeFileSync(path.join(temporary, 'scripts', script), `#!/bin/sh\nprintf '%s\\n' '${label}' >> "$VULCAN_INIT_LOG"\nif [ "$VULCAN_FAIL_STEP" = '${label}' ]; then exit 17; fi\n`);
      }
      writeFileSync(path.join(temporary, 'scripts/managed-node.mjs'), 'import fs from "node:fs"; fs.appendFileSync(process.env.VULCAN_INIT_LOG, "managed-node\\n");\n');
      writeFileSync(path.join(temporary, 'scripts/install-locked-tools.mjs'), 'import fs from "node:fs"; if (process.argv[2] !== "protoc") process.exit(9); fs.appendFileSync(process.env.VULCAN_INIT_LOG, "protoc\\n"); if (process.env.VULCAN_FAIL_STEP === "protoc") process.exit(17);\n');
      const result = spawnSync('sh', [path.join(temporary, 'make.sh'), 'init-system'], {
        cwd: temporary, encoding: 'utf8', timeout: 30000,
        env: { ...process.env, PATH: temporary + path.delimiter + process.env.PATH,
          VULCAN_INIT_LOG: log, VULCAN_FAIL_STEP: failure },
      });
      assert.equal(result.error, undefined);
      assert.equal(result.status, failure ? 17 : 0, result.stdout + result.stderr);
      // steps describes successful dispatch; a failing prerequisite remains the last recorded step.
      // steps 描述成功分发顺序；失败的前置项必须成为最后一个被记录的步骤。
      const steps = ['linux-system', 'rust-tools', 'host', 'tools', 'lua', 'protoc', 'managed-node'];
      assert.deepEqual(readFileSync(log, 'utf8').trim().split('\n'), failure ? steps.slice(0, steps.indexOf(failure) + 1) : steps);
    });
}
