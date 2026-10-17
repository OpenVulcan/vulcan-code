// Check host prerequisites before loading build modules that require modern Node APIs.
// 加载需要新版Node API的构建模块之前，先检查宿主前置条件。
'use strict';

// CommonJS and basic standard APIs keep diagnostics runnable on the observed Node 14 server.
// 使用CommonJS与基础标准API，使诊断也能在已发现的Node 14服务器上运行。
const { readFileSync } = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');
// Runtime pins remain the sole source for Node and pnpm version requirements.
// 运行时锁定清单仍是Node与pnpm版本要求的唯一来源。
const versions = JSON.parse(readFileSync(path.join(__dirname, '../crates/vulcan-service-runtime/managed-runtimes.json'), 'utf8'));

// Compare stable numeric versions; actual and minimum are strings, and the result is a boolean.
// 比较稳定数字版本；actual与minimum为字符串，返回布尔值。
function versionAtLeast(actual, minimum) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(actual);
  if (!match) return false;
  const required = minimum.split('.').map(Number);
  for (let index = 0; index < required.length; index += 1) {
    const difference = Number(match[index + 1]) - required[index];
    if (difference !== 0) return difference > 0;
  }
  return true;
}

// Inspect the selected route without installing packages; options inject process probes for tests.
// 检查所选入口但不安装软件包；options允许测试注入进程探针。
// options.mode/variant select the route; optional platform/nodeVersion/probe describe the inspected host.
// options.mode与variant选择入口；可选platform、nodeVersion及probe描述被检查的宿主。
// Return structured issues so missing, old, and non-executable tools remain distinct failures.
// 返回结构化问题，明确区分工具缺失、版本过旧与不可执行。
function checkBuildEnvironment(options = {}) {
  const mode = options.mode || '';
  const variant = options.variant || '';
  const platform = options.platform || process.platform;
  const nodeVersion = options.nodeVersion || process.versions.node;
  const probe = options.probe || spawnSync;
  const issues = [];
  const nodeModes = ['', 'build', 'br', 'bt', 'release', 'sync', 'run', 'service', 'vite', 'check', 'test', 'init-system', 'deps', 'core-build', 'core-restart', 'check-deps', 'licenses'];
  if (!nodeModes.includes(mode) || (mode === 'deps' && variant === 'rust-tools')) return issues;
  if (!versionAtLeast(nodeVersion, versions.node_version)) {
    issues.push({ tool: 'node', code: 'unsupported_version', message: `Node.js ${nodeVersion} is too old; install Node.js >= ${versions.node_version}.` });
  }
  const commands = [];
  if (['', 'build', 'br', 'bt', 'release', 'sync', 'run', 'check', 'init-system'].includes(mode)
      || (mode === 'vite' && variant === 'start')) {
    commands.push({ tool: 'pnpm', minimum: versions.pnpm_version });
  }
  if (platform === 'win32' || ['init-system', 'licenses', 'check-deps'].includes(mode)
      || (mode === 'deps' && !['tools', 'rust-tools'].includes(variant))) {
    commands.push({ tool: 'pwsh', minimum: '7.0.0' });
  }
  for (const command of commands) {
    // Windows pnpm is a cmd shim; pass only a fixed command to cmd.exe, never interpolated arguments.
    // Windows的pnpm是cmd包装脚本；仅向cmd.exe传固定命令，不插入动态参数。
    const commandProcessor = platform === 'win32' && command.tool === 'pnpm';
    const result = probe(commandProcessor ? 'cmd.exe' : command.tool,
      commandProcessor ? ['/d', '/s', '/c', 'pnpm --version'] : ['--version'],
      { encoding: 'utf8', timeout: 15000, shell: false, windowsHide: true });
    if (result.error) {
      const code = result.error.code === 'ENOENT' ? 'missing' : result.error.code === 'EACCES' ? 'not_executable' : 'probe_failed';
      issues.push({ tool: command.tool, code, message: `${command.tool}: ${code} (${result.error.code || result.error.message}).` });
      continue;
    }
    const actual = (result.stdout || '').trim().replace(/^PowerShell\s+/, '');
    if (result.status !== 0 || !versionAtLeast(actual, command.minimum)) {
      issues.push({ tool: command.tool, code: result.status === 0 ? 'unsupported_version' : 'probe_failed',
        message: `${command.tool} >= ${command.minimum} is required; probe returned ${actual || '(no version)'} (exit ${result.status}).` });
    }
  }
  return issues;
}

module.exports = { checkBuildEnvironment, versionAtLeast };

// The CLI reports every discovered prerequisite issue before any modern build module is imported.
// 命令行会在导入任何新版构建模块之前报告所有已发现的前置问题。
if (require.main === module) {
  const issues = checkBuildEnvironment({ mode: process.argv[2], variant: process.argv[3] });
  if (issues.length > 0) {
    console.error('Build prerequisites are not ready:\n' + issues.map(issue => '  - ' + issue.message).join('\n'));
    console.error('Install or activate these host tools, reopen the terminal, then run make init-system and make sync before building.');
    process.exitCode = 1;
  }
}
