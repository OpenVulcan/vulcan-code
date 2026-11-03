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
    printf '%s\\n' "$*" >> "$MOCK_SUDO_LOG"
    case "$1" in
      -v)
        [ "$MOCK_SUDO" = prompt ] || exit 1
        [ "$MOCK_SUDO_VALIDATION_CODE" = 0 ] || exit "$MOCK_SUDO_VALIDATION_CODE"
        : > "$MOCK_AUTHORIZED"
        exit 0
        ;;
      -n) shift ;;
      *) exit 99 ;;
    esac
    [ "$MOCK_SUDO" = 1 ] || [ -f "$MOCK_AUTHORIZED" ] || exit 1
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
    if [ "$MOCK_METADATA_READY" != 1 ] && [ ! -f "$MOCK_INSTALLED" ]; then
      echo 'E_TEST_MISSING_GTK_METADATA' >&2
      exit 1
    fi
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
  ldconfig)
    [ "$1" = -p ] || exit 96
    if [ "$MOCK_LIBRARIES_READY" = 1 ] || [ -f "$MOCK_INSTALLED" ]; then
      for library in $MOCK_LIBRARY_CACHE; do printf '\\t%s (libc6,x86-64) => /lib/%s\\n' "$library" "$library"; done
    fi
    ;;
  apt-cache)
    [ "$1" = policy ] || exit 95
    case "$2" in
      *t64) [ "$MOCK_NO_T64" = 1 ] && printf '  Candidate: (none)\\n' || printf '  Candidate: 1.0\\n' ;;
      *) printf '  Candidate: 1.0\\n' ;;
    esac
    ;;
  c++|make) exit 0 ;;
  *) exit 99 ;;
esac
`;

// chromiumLibraries are the sonames a ready host's linker cache lists for the bundled Chromium.
// If the installer starts probing another library, add it here; the ready scenarios then pass again.
// chromiumLibraries 是就绪宿主的链接器缓存为随包 Chromium 列出的 soname。
// 若安装器开始探测新的库，请在此补充；就绪场景随之恢复通过。
const chromiumLibraries = 'libnss3.so libnssutil3.so libsmime3.so libnspr4.so libasound.so.2 libatk-1.0.so.0 libatk-bridge-2.0.so.0 libatspi.so.0 libcups.so.2 libdbus-1.so.3 libexpat.so.1 libgbm.so.1 libglib-2.0.so.0 libgobject-2.0.so.0 libgio-2.0.so.0 libpango-1.0.so.0 libcairo.so.2 libudev.so.1 libX11.so.6 libxcb.so.1 libXcomposite.so.1 libXdamage.so.1 libXext.so.6 libXfixes.so.3 libxkbcommon.so.0 libXrandr.so.2';

// developmentPackages begins every install command; the resolved Chromium runtime packages follow it.
// developmentPackages 位于每条安装命令开头，其后是已解析的 Chromium 运行时软件包。
const developmentPackages = 'install -y --no-install-recommends build-essential pkg-config libgtk-3-dev libxdo-dev libayatana-appindicator3-dev';

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
  const sudoLog = path.join(temporary, 'sudo.log');
  mkdirSync(bin);
  writeFileSync(fixture, commandFixture, { mode: 0o755 });
  for (const command of ['env', 'mktemp', 'rm', 'rmdir', 'cat', 'awk', 'tr', 'grep']) {
    // location is resolved before PATH isolation; only ordinary file/probe utilities may reach the host.
    // location 在隔离 PATH 前解析；只有普通文件与探针辅助工具可以使用宿主命令。
    const location = spawnSync('/bin/sh', ['-c', 'command -v "$1"', 'lookup', command], { encoding: 'utf8' });
    assert.equal(location.status, 0, location.stderr);
    symlinkSync(location.stdout.trim(), path.join(bin, command));
  }
  for (const command of ['uname', 'id', 'pkg-config', 'cc', 'c++', 'make', 'ldconfig', 'apt-cache',
    ...(options.noSudo ? [] : ['sudo']), ...(options.noApt ? [] : ['apt-get'])]) {
    symlinkSync(fixture, path.join(bin, command));
  }
  // A real pseudoterminal exercises the installer's terminal checks without real sudo or APT.
  // 使用真实伪终端验证安装器的终端判断，不调用真实 sudo 或 APT。
  const installer = path.join(__dirname, 'install_linux_system_deps.sh');
  let executable = '/bin/sh';
  let argumentsList = [installer];
  if (options.interactive) {
    const terminal = spawnSync('/bin/sh', ['-c', 'command -v script'], { encoding: 'utf8' });
    assert.equal(terminal.status, 0, 'Terminal tests require the util-linux script command.');
    executable = terminal.stdout.trim();
    // Quote every fixed path as one shell argument, including temporary paths containing spaces or apostrophes.
    // 将每个固定路径引用为一个 shell 参数，覆盖包含空格或单引号的临时路径。
    const command = ['/bin/sh', installer].map(argument => "'" + argument.replaceAll("'", "'\\''") + "'").join(' ');
    argumentsList = ['-q', '-e', '-c', command, '/dev/null'];
  }
  const result = spawnSync(executable, argumentsList, {
    encoding: 'utf8', timeout: 10000, input: '',
    env: {
      ...process.env, PATH: bin, TMPDIR: temporary, SHELL: '/bin/sh',
      MOCK_KERNEL: 'Linux', MOCK_UID: '0', MOCK_SUDO: '0', MOCK_INDICATOR: 'ayatana',
      MOCK_METADATA_READY: '0', MOCK_LINK_READY: '0', MOCK_UPDATE_CODE: '0', MOCK_INSTALL_CODE: '0',
      MOCK_POST_UNREADY: '0', MOCK_SUDO_VALIDATION_CODE: '0', MOCK_LIBRARIES_READY: '0', MOCK_NO_T64: '0',
      MOCK_LIBRARY_CACHE: chromiumLibraries, ...options.env,
      MOCK_INSTALLED: path.join(temporary, 'installed'), MOCK_APT_LOG: aptLog,
      MOCK_AUTHORIZED: path.join(temporary, 'authorized'), MOCK_SUDO_LOG: sudoLog,
    },
  });
  assert.equal(result.error, undefined);
  return {
    ...result, output: result.stdout + result.stderr,
    apt: existsSync(aptLog) ? readFileSync(aptLog, 'utf8').trim().split('\n') : [],
    sudo: existsSync(sudoLog) ? readFileSync(sudoLog, 'utf8').trim().split('\n') : [],
  };
}

// These cases distinguish idempotence, supported indicator providers, privilege refusal and genuine installation failures.
// 这些用例区分幂等性、受支持的托盘实现、权限不足和真实安装失败。
for (const scenario of [
  { name: 'ready Ayatana dependencies need no administrator or package operations',
    env: { MOCK_UID: '1000', MOCK_METADATA_READY: '1', MOCK_LINK_READY: '1', MOCK_LIBRARIES_READY: '1' }, code: 0, installs: false },
  { name: 'ready legacy AppIndicator dependencies are also supported',
    env: { MOCK_METADATA_READY: '1', MOCK_LINK_READY: '1', MOCK_LIBRARIES_READY: '1', MOCK_INDICATOR: 'legacy' }, code: 0, installs: false },
  { name: 'missing Chromium runtime libraries trigger installation even with development files present',
    env: { MOCK_METADATA_READY: '1', MOCK_LINK_READY: '1' }, code: 0, installs: true, diagnostic: /libnss3\.so/ },
  { name: 'releases without 64-bit time_t package names install the original names',
    env: { MOCK_NO_T64: '1' }, code: 0, installs: true, runtime: 'libasound2' },
  { name: 'missing development metadata is installed and then verified',
    env: {}, code: 0, installs: true, diagnostic: /E_TEST_MISSING_GTK_METADATA/ },
  { name: 'missing libxdo headers trigger installation even with GTK metadata present',
    env: { MOCK_METADATA_READY: '1' }, code: 0, installs: true, diagnostic: /xdo\.h/ },
  { name: 'available noninteractive sudo can install missing packages',
    env: { MOCK_UID: '1000', MOCK_SUDO: '1' }, code: 0, installs: true },
  { name: 'terminal initialization requests sudo once and continues after authentication',
    interactive: true, env: { MOCK_UID: '1000', MOCK_SUDO: 'prompt' }, code: 0, installs: true, authorizations: 1 },
  { name: 'denied terminal sudo authentication prevents all package operations',
    interactive: true, env: { MOCK_UID: '1000', MOCK_SUDO: 'prompt', MOCK_SUDO_VALIDATION_CODE: '31' },
    code: 31, installs: false, authorizations: 1 },
  { name: 'noninteractive initialization never requests a sudo password',
    env: { MOCK_UID: '1000', MOCK_SUDO: 'prompt' }, code: 1, installs: false },
  { name: 'ready dependencies do not request terminal sudo authorization',
    interactive: true, env: { MOCK_UID: '1000', MOCK_SUDO: 'prompt', MOCK_METADATA_READY: '1', MOCK_LINK_READY: '1', MOCK_LIBRARIES_READY: '1' },
    code: 0, installs: false },
  { name: 'cached sudo in a terminal needs no interactive authorization',
    interactive: true, env: { MOCK_UID: '1000', MOCK_SUDO: '1' }, code: 0, installs: true },
  { name: 'a missing sudo executable fails before invoking the package manager',
    noSudo: true, env: { MOCK_UID: '1000' }, code: 1, installs: false },
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
    assert.equal(result.status, scenario.code, result.output);
    assert.equal(result.sudo.filter(operation => operation === '-v').length, scenario.authorizations || 0);
    if (scenario.diagnostic) assert.match(result.output, scenario.diagnostic);
    if (scenario.installs) {
      // The development set comes first, then each Chromium runtime entry resolved to its first installable name.
      // 先是开发软件包集合，其后是解析为首个可安装名称的各 Chromium 运行时条目。
      assert.equal(result.apt.length, 2, result.apt.join('\n'));
      assert.equal(result.apt[0], 'update');
      assert.ok(result.apt[1].startsWith(`${developmentPackages} `), result.apt[1]);
      const runtime = result.apt[1].slice(developmentPackages.length).trim().split(/\s+/);
      assert.ok(runtime.includes(scenario.runtime || 'libasound2t64'), result.apt[1]);
    } else {
      assert.deepEqual(result.apt, scenario.apt || []);
    }
    assert.doesNotMatch(result.stderr, /probe\.log.*(?:No such file|not found)/);
  });
}
