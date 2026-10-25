// Run the real install.sh against synthetic release archives with faked host identity commands.
// 以伪造的宿主身份命令，针对合成发行归档运行真实 install.sh。
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { appendFile, chmod, copyFile, lstat, mkdir, mkdtemp, readFile, readdir, readlink, realpath, rm, stat, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { checksumFileName, createReleaseArchive, installerFileName, writeChecksums } from './release-package.mjs';

const installer = path.join(path.dirname(fileURLToPath(import.meta.url)), installerFileName);
// The installer is a POSIX shell script; Windows uses the zip archive instead.
// 安装脚本是 POSIX shell 脚本；Windows 改用 zip 归档。
const skip = process.platform === 'win32';

// workingDescribe and brokenDescribe stand in for a loadable build and one this system's loader rejects.
// workingDescribe 与 brokenDescribe 分别代表可加载构建与被本机加载器拒绝的构建。
const workingDescribe = `printf '{\\n  "profile": "release"\\n}\\n'`;
const brokenDescribe = `echo "version 'GLIBC_9.99' not found" >&2; exit 1`;

// withHome gives action an isolated home with faked uname and sysctl ahead of the real PATH.
// withHome 向 action 提供隔离主目录，并在真实 PATH 之前放置伪造的 uname 与 sysctl。
async function withHome(action) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-install-'));
  const fakes = path.join(root, 'fakes');
  await mkdir(fakes);
  await writeFile(path.join(fakes, 'uname'), '#!/bin/sh\ncase "$1" in -s) echo "$FAKE_OS" ;; -m) echo "$FAKE_MACHINE" ;; *) exit 64 ;; esac\n');
  await writeFile(path.join(fakes, 'sysctl'), '#!/bin/sh\necho "$FAKE_TRANSLATED"\n');
  await chmod(path.join(fakes, 'uname'), 0o755);
  await chmod(path.join(fakes, 'sysctl'), 0o755);
  const home = path.join(root, 'home');
  await mkdir(home);
  const context = {
    root, home,
    prefix: path.join(home, 'app'), bin: path.join(home, 'bin'), log: path.join(root, 'calls.log'),
    env: { FAKE_OS: 'Linux', FAKE_MACHINE: 'x86_64', FAKE_TRANSLATED: '0' },
    // install runs the real script with this home and returns its exit status and output.
    // install 以该主目录运行真实脚本，返回退出状态与输出。
    install(args, env = {}) {
      return spawnSync('sh', [installer, ...args], {
        encoding: 'utf8',
        env: {
          ...process.env, ...context.env, ...env, HOME: home, XDG_CONFIG_HOME: path.join(home, '.config'),
          PATH: `${fakes}:${process.env.PATH}`, VULCAN_TEST_LOG: context.log, TMPDIR: root,
        },
      });
    },
    // installFrom installs from source into the context's own prefix and launcher directory.
    // installFrom 从 source 安装到上下文自身的前缀与启动器目录。
    installFrom(source, env = {}, extra = []) {
      return context.install(['--from', source, '--prefix', context.prefix, '--bin-dir', context.bin, ...extra], env);
    },
    async calls() {
      return (await readFile(context.log, 'utf8').catch(() => '')).trim().split('\n').filter(Boolean);
    },
  };
  try { await action(context); } finally { await rm(root, { recursive: true, force: true }); }
}

// writeSource publishes one version's archives for platforms into a fresh source directory, as a release would.
// writeSource 将一个版本在各平台的归档发布到新的来源目录，与正式发行一致。
async function writeSource(root, version, platforms, describe = workingDescribe) {
  const source = path.join(root, `source-${version}-${platforms.join('-')}`);
  for (const platform of platforms) {
    const [system, architecture] = platform.split('-');
    const dist = path.join(root, `dist-${version}-${platform}`);
    await mkdir(path.join(dist, 'bin'), { recursive: true });
    await mkdir(path.join(dist, 'desktop/runtime'), { recursive: true });
    await writeFile(path.join(dist, 'runtime-layout.json'), JSON.stringify({
      schemaVersion: 3, profile: 'release',
      serviceExecutable: 'bin/vulcan-code-service', desktopExecutable: 'desktop/runtime/electron', tuiExecutable: 'bin/vulcan-tui',
      platform: system === 'macos' ? 'darwin' : system, architecture,
    }, null, 2));
    const scripts = {
      'bin/vulcan-code-service': `echo "service ${platform} ${version} $*" >> "$VULCAN_TEST_LOG"\nif [ "$1" = --describe ]; then ${describe}; fi\n`,
      'bin/vulcan-tui': `echo "tui ${platform} ${version} $*"\n`,
      'desktop/runtime/electron': `echo "desktop ${platform} ${version}"\n`,
    };
    for (const [relative, body] of Object.entries(scripts)) {
      await writeFile(path.join(dist, relative), `#!/bin/sh\n${body}`);
      await chmod(path.join(dist, relative), 0o755);
    }
    await createReleaseArchive(dist, source, version);
  }
  await copyFile(installer, path.join(source, installerFileName));
  await writeChecksums(source);
  return source;
}

// archiveGeneration derives the expected installation identity from the fixture archive's exact bytes.
// archiveGeneration 根据夹具归档的精确字节派生期望的安装身份。
async function archiveGeneration(source, version, platform) {
  // The suffix is a content identity, not a fixed fixture snapshot.
  // 后缀是内容身份，不是固定夹具快照。
  const name = `vulcan-code-${version}-${platform}`;
  const digest = createHash('sha256').update(await readFile(path.join(source, `${name}.tar.gz`))).digest('hex');
  return `${name}-${digest.slice(0, 12)}`;
}

// run executes one installed launcher and returns its trimmed standard output.
// run 执行一个已安装启动器，返回去除首尾空白的标准输出。
function run(launcher, ...args) {
  const result = spawnSync(launcher, args, { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}

test('the archive matching the host operating system and architecture is installed behind launchers', { skip }, () => withHome(async context => {
  const source = await writeSource(context.root, '0.1.0', ['linux-x64', 'linux-arm64', 'macos-x64', 'macos-arm64']);
  const cases = [
    { FAKE_OS: 'Linux', FAKE_MACHINE: 'x86_64', expected: 'linux-x64' },
    { FAKE_OS: 'Linux', FAKE_MACHINE: 'aarch64', expected: 'linux-arm64' },
    { FAKE_OS: 'Darwin', FAKE_MACHINE: 'arm64', expected: 'macos-arm64' },
    { FAKE_OS: 'Darwin', FAKE_MACHINE: 'x86_64', expected: 'macos-x64' },
    // A Rosetta shell on Apple silicon still receives the native build.
    // Apple 芯片上的 Rosetta shell 仍获得原生构建。
    { FAKE_OS: 'Darwin', FAKE_MACHINE: 'x86_64', FAKE_TRANSLATED: '1', expected: 'macos-arm64' },
  ];
  for (const { expected, ...identity } of cases) {
    const prefix = path.join(context.home, expected + (identity.FAKE_TRANSLATED ?? ''));
    const bin = path.join(prefix, 'bin');
    const result = context.install(['--from', source, '--prefix', prefix, '--bin-dir', bin], identity);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(await readlink(path.join(prefix, 'current')), `versions/${await archiveGeneration(source, '0.1.0', expected)}`);
    assert.equal(run(path.join(bin, 'vulcan-tui'), '--theme', 'dark'), `tui ${expected} 0.1.0 --theme dark`);
    assert.equal(run(path.join(bin, 'vulcan-code')), `desktop ${expected} 0.1.0`);
    assert.equal(result.stdout.includes('--install-browser-dependencies'), expected.startsWith('linux'));
  }
}));

test('upgrades switch the active version, keep the previous one, and prune older ones', { skip }, () => withHome(async context => {
  // Retention expectations follow the actual installed generation identities.
  // 保留预期跟随实际安装的代际身份。
  const installed = [];
  for (const version of ['0.1.0', '0.1.1', '0.1.2']) {
    const source = await writeSource(context.root, version, ['linux-x64']);
    const result = context.installFrom(source);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(run(path.join(context.bin, 'vulcan-tui')), `tui linux-x64 ${version}`);
    installed.push(await archiveGeneration(source, version, 'linux-x64'));
  }
  assert.deepEqual((await readdir(path.join(context.prefix, 'versions'))).sort(),
    installed.slice(-2).sort());
}));

test('tampered archives, unloadable builds, and unknown hosts leave the active version untouched', { skip }, () => withHome(async context => {
  assert.equal(context.installFrom(await writeSource(context.root, '0.1.0', ['linux-x64'])).status, 0);
  // Capture the active content identity before probing failed installations.
  // 在验证失败安装之前捕获当前内容身份。
  const active = await readlink(path.join(context.prefix, 'current'));
  const assertUnchanged = async () => {
    assert.equal(await readlink(path.join(context.prefix, 'current')), active);
    assert.deepEqual(await readdir(path.join(context.prefix, 'versions')), [path.basename(active)]);
  };

  const tampered = await writeSource(context.root, '0.1.1', ['linux-x64']);
  await appendFile(path.join(tampered, 'vulcan-code-0.1.1-linux-x64.tar.gz'), 'tampered');
  const mismatch = context.installFrom(tampered);
  assert.notEqual(mismatch.status, 0);
  assert.match(mismatch.stderr, /checksum mismatch/);
  await assertUnchanged();

  const unloadable = context.installFrom(await writeSource(context.root, '0.1.2', ['linux-x64'], brokenDescribe));
  assert.notEqual(unloadable.status, 0);
  assert.match(unloadable.stderr, /cannot run on this system[\s\S]*GLIBC_9\.99/);
  await assertUnchanged();

  const missing = context.installFrom(await writeSource(context.root, '0.1.3', ['linux-x64']), { FAKE_MACHINE: 'aarch64' });
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /expected one linux-arm64 archive/);
  const unknown = context.installFrom(await writeSource(context.root, '0.1.4', ['linux-x64']), { FAKE_MACHINE: 'riscv64' });
  assert.notEqual(unknown.status, 0);
  assert.match(unknown.stderr, /unsupported architecture riscv64/);
  await assertUnchanged();
  assert.deepEqual((await readdir(context.root)).filter(name => name.startsWith('vulcan-code-install.')), []);
}));

test('only this prefix\'s user service is re-registered and removed, and foreign launchers survive', { skip }, () => withHome(async context => {
  const unit = path.join(context.home, '.config/systemd/user/vulcan-code.service');
  await mkdir(path.dirname(unit), { recursive: true });
  await writeFile(unit, 'ExecStart="/opt/other/bin/vulcan-code-service" --headless\n');
  await mkdir(context.bin, { recursive: true });
  await writeFile(path.join(context.bin, 'vulcan-tui'), 'mine');

  const first = context.installFrom(await writeSource(context.root, '0.1.0', ['linux-x64']));
  assert.equal(first.status, 0, first.stderr);
  assert.match(first.stderr, /Skipped .*vulcan-tui/);
  assert.equal(await readFile(path.join(context.bin, 'vulcan-tui'), 'utf8'), 'mine');
  assert.ok(!(await context.calls()).some(call => call.includes('user-service')));

  // The unit now names this prefix's real version path, as --install-user-service writes it.
  // 单元现在指向本前缀的真实版本路径，与 --install-user-service 写入的一致。
  await writeFile(unit, `ExecStart="${await realpath(path.join(context.prefix, 'current'))}/bin/vulcan-code-service" --headless\n`);
  const upgrade = context.installFrom(await writeSource(context.root, '0.1.1', ['linux-x64']), {}, ['--with-browser-dependencies']);
  assert.equal(upgrade.status, 0, upgrade.stderr);
  const calls = await context.calls();
  assert.ok(calls.includes('service linux-x64 0.1.1 --install-user-service'));
  assert.ok(calls.includes('service linux-x64 0.1.1 --install-browser-dependencies'));

  const removed = context.install(['--uninstall', '--prefix', context.prefix, '--bin-dir', context.bin]);
  assert.equal(removed.status, 0, removed.stderr);
  assert.ok((await context.calls()).includes('service linux-x64 0.1.1 --uninstall-user-service'));
  assert.deepEqual(await readdir(context.bin), ['vulcan-tui']);
  await assert.rejects(readdir(context.prefix), { code: 'ENOENT' });
}));

// Unowned prefixes and their preexisting version directories must survive both entrypoints.
// 非安装器拥有的前缀及其中既有版本目录必须在两个入口下均保持原样。
test('installation and uninstall reject nonempty unowned prefixes without deleting files', { skip }, () => withHome(async context => {
  // A preexisting versions directory is ordinary user data until ownership is established.
  // 归属确立前，既有 versions 目录只是普通用户数据。
  const foreignFile = path.join(context.prefix, 'versions', 'personal', 'keep.txt');
  await mkdir(path.dirname(foreignFile), { recursive: true });
  await writeFile(foreignFile, 'keep');
  for (const args of [
    ['--uninstall', '--prefix', context.prefix, '--bin-dir', context.bin],
    ['--from', await writeSource(context.root, '0.1.0', ['linux-x64']), '--prefix', context.prefix, '--bin-dir', context.bin],
    ['--uninstall', '--prefix', context.home, '--bin-dir', context.bin],
  ]) {
    assert.notEqual(context.install(args).status, 0);
    assert.equal(await readFile(foreignFile, 'utf8'), 'keep');
  }
}));

// Ownership covers installation entries, not later user documents under the same prefix.
// 归属只覆盖安装条目，不覆盖后来放入同一前缀的用户文档。
test('uninstall keeps additional files in an owned prefix', { skip }, () => withHome(async context => {
  assert.equal(context.installFrom(await writeSource(context.root, '0.1.0', ['linux-x64'])).status, 0);
  await writeFile(path.join(context.prefix, 'keep.txt'), 'keep');
  // The installer removes its own namespace and leaves the additional file untouched.
  // 安装器移除自身命名空间，并保留额外文件。
  const result = context.install(['--uninstall', '--prefix', context.prefix, '--bin-dir', context.bin]);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(await readdir(context.prefix), ['keep.txt']);
  assert.equal(await readFile(path.join(context.prefix, 'keep.txt'), 'utf8'), 'keep');
}));

// Identical bytes do not rename any installed entry; rebuilt bytes retain the preceding generation.
// 相同字节不重命名任何安装条目；重新构建的字节保留前一代际。
test('same-version archives reuse identical generations and retain rebuilt predecessors', { skip }, () => withHome(async context => {
  // Preserve the old directory and link identities as observable evidence for existing processes.
  // 保留旧目录与链接身份，作为既有进程可观察的证据。
  const source = await writeSource(context.root, '0.1.0', ['linux-x64']);
  assert.equal(context.installFrom(source).status, 0);
  const current = path.join(context.prefix, 'current');
  const originalRoot = await realpath(current);
  const originalInode = (await stat(originalRoot, { bigint: true })).ino;
  const linkInode = (await lstat(current, { bigint: true })).ino;
  const originalService = await readFile(path.join(originalRoot, 'bin/vulcan-code-service'));
  const originalCalls = await context.calls();
  assert.equal(context.installFrom(source).status, 0);
  assert.equal((await stat(originalRoot, { bigint: true })).ino, originalInode);
  assert.equal((await lstat(current, { bigint: true })).ino, linkInode);
  assert.deepEqual(await context.calls(), originalCalls);

  // This archive has the same version but a deliberately different executable body.
  // 此归档具有相同版本，但可执行文件正文有意不同。
  const rebuilt = await writeSource(path.join(context.root, 'rebuilt'), '0.1.0', ['linux-x64'], `${workingDescribe}\n:`);
  const result = context.installFrom(rebuilt);
  assert.equal(result.status, 0, result.stderr);
  assert.notEqual(await realpath(current), originalRoot);
  assert.equal((await stat(originalRoot, { bigint: true })).ino, originalInode);
  assert.deepEqual(await readFile(path.join(originalRoot, 'bin/vulcan-code-service')), originalService);
  assert.deepEqual((await readdir(path.join(context.prefix, 'versions'))).sort(), [path.basename(originalRoot), await archiveGeneration(rebuilt, '0.1.0', 'linux-x64')].sort());
}));

// An existing abbreviated digest directory must also carry the matching complete digest.
// 既有缩写摘要目录还必须携带匹配的完整摘要。
test('an existing generation with mismatching digest metadata is never overwritten', { skip }, () => withHome(async context => {
  // Corrupt only this fixture's marker to simulate an incomplete generation or prefix collision.
  // 仅损坏此夹具的标记，模拟不完整代际或前缀碰撞。
  const source = await writeSource(context.root, '0.1.0', ['linux-x64']);
  assert.equal(context.installFrom(source).status, 0);
  const current = await realpath(path.join(context.prefix, 'current'));
  const service = await readFile(path.join(current, 'bin/vulcan-code-service'));
  await writeFile(path.join(current, '.vulcan-archive-sha256'), 'different archive\n');
  assert.notEqual(context.installFrom(source).status, 0);
  assert.equal(await realpath(path.join(context.prefix, 'current')), current);
  assert.deepEqual(await readFile(path.join(current, 'bin/vulcan-code-service')), service);
}));

// The default source is a URL, so the download path is exercised against a local HTTP server.
// 默认来源是 URL，因此使用本地 HTTP 服务验证下载路径。
const downloader = ['curl', 'wget'].some(tool => spawnSync('sh', ['-c', `command -v ${tool}`]).status === 0);
test('a release is installed over HTTP', { skip: process.platform !== 'linux' || !downloader }, () => withHome(async context => {
  const source = await writeSource(context.root, '0.1.0', ['linux-x64', 'linux-arm64']);
  const server = createServer((request, response) => {
    const name = decodeURIComponent(new URL(request.url, 'http://localhost').pathname).replace(/^\/release\//, '');
    if (name.includes('/') || ![checksumFileName, installerFileName].includes(name) && !name.startsWith('vulcan-code-')) {
      response.writeHead(404).end();
      return;
    }
    createReadStream(path.join(source, name)).on('error', () => response.writeHead(404).end()).pipe(response);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const url = `http://127.0.0.1:${server.address().port}/release`;
    // Run asynchronously: a synchronous child would block this process's own HTTP server.
    // 异步运行：同步子进程会阻塞本进程自身的 HTTP 服务。
    const status = await new Promise((resolve, reject) => {
      const child = spawn('sh', [installer, '--from', url, '--prefix', context.prefix, '--bin-dir', context.bin], {
        env: { ...process.env, HOME: context.home, PATH: process.env.PATH, VULCAN_TEST_LOG: context.log, TMPDIR: context.root },
        stdio: 'inherit',
      });
      child.once('error', reject);
      child.once('exit', resolve);
    });
    assert.equal(status, 0);
    const machine = spawnSync('uname', ['-m'], { encoding: 'utf8' }).stdout.trim();
    const expected = ['aarch64', 'arm64'].includes(machine) ? 'linux-arm64' : 'linux-x64';
    assert.equal(run(path.join(context.bin, 'vulcan-tui')), `tui ${expected} 0.1.0`);
  } finally {
    server.close();
  }
}));
