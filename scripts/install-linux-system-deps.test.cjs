// Verify the Linux installer through isolated commands without package changes or network access.
// 通过隔离命令验证 Linux 安装器，不修改软件包、不访问网络。
'use strict';
const assert = require('node:assert/strict');
const { test } = require('node:test');
const { spawnSync } = require('node:child_process');
const { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync, existsSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');

// commandFixture implements only the external interfaces of the package manager and native probes.
// commandFixture 仅实现包管理器与原生探针的外部接口。
const commandFixture = `#!/bin/sh
case "\${0##*/}" in
  uname) printf '%s\\n' "$MOCK_KERNEL" ;;
  id) printf '%s\\n' "$MOCK_UID" ;;
  sudo)
    [ "$MOCK_SUDO" = 1 ] || exit 1
    shift
    if [ "$1" = true ]; then exit 0; fi
    exec "$@"
    ;;
  pkg-config)
    for argument do
      case "$argument" in
        ayatana-appindicator3-0.1) [ "$MOCK_INDICATOR" = ayatana ]; exit $? ;;
        appindicator3-0.1) [ "$MOCK_INDICATOR" = legacy ]; exit $? ;;
      esac
    done
    [ "$MOCK_METADATA_READY" = 1 ] || [ -f "$MOCK_INSTALLED" ] || exit 1
    ;;
  cc)
    if [ "$MOCK_LINK_READY" != 1 ] && [ ! -f "$MOCK_INSTALLED" ]; then
      echo 'xdo.h: No such file or directory' >&2
      exit 1
    fi
    while [ "$#" -gt 0 ]; do
      if [ "$1" = -o ]; then
        shift
        printf '#!/bin/sh\\nexit 91\\n' > "$1"
        break
      fi
      shift
    done
    ;;
  apt-get)
    printf '%s\\n' "$*" >> "$MOCK_APT_LOG"
    [ "$DEBIAN_FRONTEND" = noninteractive ] || exit 97
    case "$1" in
      update) exit "$MOCK_UPDATE_CODE" ;;
      install)
        [ "$MOCK_INSTALL_CODE" = 0 ] || exit "$MOCK_INSTALL_CODE"
        if [ "$MOCK_POST_UNREADY" != 1 ]; then : > "$MOCK_INSTALLED"; fi
        ;;
      *) exit 98 ;;
    esac
    ;;
  c++|make) exit 0 ;;
  *) exit 99 ;;
esac
`;

// exercise runs the real script with controlled probe/package commands; t owns cleanup and options select host behavior.
// exercise 使用受控探针和包管理命令运行真实脚本；t 管理清理，options 选择宿主行为。
// Return the exit result and observed package operations so callers assert behavior rather than source text.
// 返回退出结果与实际软件包操作，使调用方验证行为而非源码文本。
function exercise(t, options) {
  // temporary owns every fixture and probe; bin isolates all tools from the real host package manager.
  // temporary 拥有全部夹具与探针；bin 将所有工具与真实宿主包管理器隔离。
  const temporary = mkdtempSync(path.join(tmpdir(), 'vulcan native deps-'));
  assert.equal(path.dirname(path.resolve(temporary)), path.resolve(tmpdir()));
  t.after(() => rmSync(temporary, { recursive: true, force: true }));
  const bin = path.join(temporary, 'bin');
  const fixture = path.join(temporary, 'command.sh');
  const aptLog = path.join(temporary, 'apt.log');
  mkdirSync(bin);
  writeFileSync(fixture, commandFixture, { mode: 0o755 });
  for (const command of ['env', 'mktemp', 'rm', 'rmdir', 'cat']) {
    // location is resolved before PATH isolation; only ordinary file/probe utilities may reach the host.
    // location 在隔离 PATH 前解析；只有普通文件与探针辅助工具可以使用宿主命令。
    const location = spawnSync('/bin/sh', ['-c', 'command -v "$1"', 'lookup', command], { encoding: 'utf8' });
    assert.equal(location.status, 0, location.stderr);
    symlinkSync(location.stdout.trim(), path.join(bin, command));
  }
  for (const command of ['uname', 'id', 'sudo', 'pkg-config', 'cc', 'c++', 'make', ...(options.noApt ? [] : ['apt-get'])]) {
    symlinkSync(fixture, path.join(bin, command));
  }
  const result = spawnSync('/bin/sh', [path.join(__dirname, 'install_linux_system_deps.sh')], {
    encoding: 'utf8', timeout: 10000,
    env: {
      ...process.env, PATH: bin, TMPDIR: temporary,
      MOCK_KERNEL: 'Linux', MOCK_UID: '0', MOCK_SUDO: '0', MOCK_INDICATOR: 'ayatana',
      MOCK_METADATA_READY: '0', MOCK_LINK_READY: '0', MOCK_UPDATE_CODE: '0', MOCK_INSTALL_CODE: '0',
      MOCK_POST_UNREADY: '0', ...options.env,
      MOCK_INSTALLED: path.join(temporary, 'installed'), MOCK_APT_LOG: aptLog,
    },
  });
  assert.equal(result.error, undefined);
  return { ...result, apt: existsSync(aptLog) ? readFileSync(aptLog, 'utf8').trim().split('\n') : [] };
}

// These cases distinguish idempotence, supported indicator providers, privilege refusal and genuine installation failures.
// 这些用例区分幂等性、受支持的托盘实现、权限不足和真实安装失败。
for (const scenario of [
  { name: 'ready Ayatana dependencies need no administrator or package operations',
    env: { MOCK_UID: '1000', MOCK_METADATA_READY: '1', MOCK_LINK_READY: '1' }, code: 0, installs: false },
  { name: 'ready legacy AppIndicator dependencies are also supported',
    env: { MOCK_METADATA_READY: '1', MOCK_LINK_READY: '1', MOCK_INDICATOR: 'legacy' }, code: 0, installs: false },
  { name: 'missing development metadata is installed and then verified', env: {}, code: 0, installs: true },
  { name: 'missing libxdo headers trigger installation even with GTK metadata present',
    env: { MOCK_METADATA_READY: '1' }, code: 0, installs: true },
  { name: 'available noninteractive sudo can install missing packages',
    env: { MOCK_UID: '1000', MOCK_SUDO: '1' }, code: 0, installs: true },
  { name: 'ordinary users without sudo fail before running apt',
    env: { MOCK_UID: '1000' }, code: 1, installs: false },
  { name: 'unsupported package managers fail explicitly without invoking another manager',
    env: {}, noApt: true, code: 1, installs: false },
  { name: 'apt update failure stops before package installation',
    env: { MOCK_UPDATE_CODE: '23' }, code: 23, apt: ['update'] },
  { name: 'apt install failure preserves its exit code',
    env: { MOCK_INSTALL_CODE: '24' }, code: 24, installs: true },
  { name: 'an apparently successful installation cannot hide missing development metadata',
    env: { MOCK_POST_UNREADY: '1' }, code: 1, installs: true },
  { name: 'non-Linux hosts skip the Linux dependency installer',
    env: { MOCK_KERNEL: 'Darwin' }, code: 0, installs: false },
]) {
  test(scenario.name, { skip: process.platform !== 'linux' }, t => {
    // result is the observed process outcome; expected package roles remain independent from installer implementation.
    // result 是实际进程结果；期望的软件包职责独立于安装器实现。
    const result = exercise(t, scenario);
    assert.equal(result.status, scenario.code, result.stdout + result.stderr);
    assert.deepEqual(result.apt, scenario.apt || (scenario.installs ? [
      'update', 'install -y --no-install-recommends build-essential pkg-config libgtk-3-dev libxdo-dev libayatana-appindicator3-dev',
    ] : []));
    assert.doesNotMatch(result.stderr, /probe\.log.*(?:No such file|not found)/);
  });
}
