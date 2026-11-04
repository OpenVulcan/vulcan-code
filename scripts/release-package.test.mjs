// Release archive naming, contents, and checksum index behavior on synthetic distributions.
// 基于合成发行物验证发行归档的命名、内容与校验索引行为。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { chmod, mkdir, mkdtemp, readFile, readdir, readlink, rm, stat, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { checksumFileName, createReleaseArchive, releaseAssetName, requiredGlibc, writeChecksums } from './release-package.mjs';

// targets is every platform the release supports, with the archive suffix each must get.
// targets 是发行支持的全部平台及各自应得的归档后缀。
const targets = [
  ['win32', 'x64', 'windows-x64.zip'],
  ['darwin', 'x64', 'macos-x64.tar.gz'],
  ['darwin', 'arm64', 'macos-arm64.tar.gz'],
  ['linux', 'x64', 'linux-x64.tar.gz'],
  ['linux', 'arm64', 'linux-arm64.tar.gz'],
];

// withRoot gives action an isolated directory and always removes it.
// withRoot 向 action 提供隔离目录，并始终移除它。
async function withRoot(action) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-release-'));
  try { await action(root); } finally { await rm(root, { recursive: true, force: true }); }
}

// writeDistribution creates a minimal release tree for platform and arch with an executable and a relative link.
// writeDistribution 为 platform 与 arch 创建最小 release 目录树，含一个可执行文件与一个相对链接。
async function writeDistribution(root, platform, architecture, profile = 'release') {
  await mkdir(path.join(root, 'bin'), { recursive: true });
  await mkdir(path.join(root, 'desktop/runtime'), { recursive: true });
  await writeFile(path.join(root, 'runtime-layout.json'), JSON.stringify({ schemaVersion: 4, profile, platform, architecture }, null, 2));
  await writeFile(path.join(root, 'bin/vulcan-code-service'), '#!/bin/sh\necho service\n');
  await chmod(path.join(root, 'bin/vulcan-code-service'), 0o755);
  await writeFile(path.join(root, 'desktop/runtime/payload.txt'), 'payload');
  if (process.platform !== 'win32') await symlink('payload.txt', path.join(root, 'desktop/runtime/link.txt'));
}

// sha256 hashes one file for comparison with the index.
// sha256 计算单个文件的哈希，用于与索引比对。
async function sha256(file) {
  return createHash('sha256').update(await readFile(file)).digest('hex');
}

test('every supported platform gets a distinct archive name and an unsupported one is refused', () => {
  const names = targets.map(([platform, architecture, suffix]) => {
    const name = releaseAssetName({ platform, architecture }, '0.1.0');
    assert.equal(name, `vulcan-code-0.1.0-${suffix}`);
    return name;
  });
  assert.equal(new Set(names).size, targets.length);
  assert.throws(() => releaseAssetName({ platform: 'freebsd', architecture: 'x64' }, '0.1.0'));
  assert.throws(() => releaseAssetName({ platform: 'linux', architecture: 'x64' }, '0.1'));
});

test('a debug distribution is never packaged', () => withRoot(async root => {
  await writeDistribution(path.join(root, 'dist'), 'linux', 'x64', 'debug');
  await assert.rejects(createReleaseArchive(path.join(root, 'dist'), path.join(root, 'out'), '0.1.0'), /release/);
  await assert.rejects(readdir(path.join(root, 'out')), { code: 'ENOENT' });
}));

test('a Unix archive holds the distribution at its root with links and executable bits, indexed by checksum', () => withRoot(async root => {
  const dist = path.join(root, 'dist');
  const out = path.join(root, 'out');
  await writeDistribution(dist, 'linux', 'arm64');
  const archive = await createReleaseArchive(dist, out, '0.1.0');
  assert.equal(path.basename(archive), 'vulcan-code-0.1.0-linux-arm64.tar.gz');
  assert.deepEqual((await readdir(out)).sort(), [checksumFileName, path.basename(archive)].sort());

  const extracted = path.join(root, 'extracted');
  await mkdir(extracted);
  execFileSync(process.platform === 'win32' ? path.join(process.env.SystemRoot, 'System32', 'tar.exe') : 'tar', ['-xzf', archive, '-C', extracted]);
  assert.deepEqual((await readdir(extracted)).sort(), ['bin', 'desktop', 'runtime-layout.json']);
  assert.equal(await readFile(path.join(extracted, 'desktop/runtime/payload.txt'), 'utf8'), 'payload');
  if (process.platform !== 'win32') {
    assert.equal(await readlink(path.join(extracted, 'desktop/runtime/link.txt')), 'payload.txt');
    assert.equal((await stat(path.join(extracted, 'bin/vulcan-code-service'))).mode & 0o111, 0o111);
  }

  const index = await readFile(path.join(out, checksumFileName), 'utf8');
  assert.equal(index, `${await sha256(archive)}  ${path.basename(archive)}\n`);
}));

test('the index covers every platform archive of one version, and refuses mixed versions', () => withRoot(async root => {
  const out = path.join(root, 'out');
  for (const [platform, architecture] of targets.filter(([platform]) => platform !== 'win32')) {
    const dist = path.join(root, `${platform}-${architecture}`);
    await writeDistribution(dist, platform, architecture);
    await createReleaseArchive(dist, out, '0.2.0-rc.1');
  }
  await writeFile(path.join(out, 'notes.txt'), 'not indexed');
  const indexed = await writeChecksums(out);
  assert.equal(indexed.length, 4);
  const lines = (await readFile(path.join(out, checksumFileName), 'utf8')).trim().split('\n');
  for (const line of lines) {
    const [digest, name] = line.split('  ');
    assert.equal(digest, await sha256(path.join(out, name)));
  }
  assert.deepEqual(lines.map(line => line.split('  ')[1]), indexed);

  await writeFile(path.join(out, 'vulcan-code-0.2.1-linux-x64.tar.gz'), 'other version');
  await assert.rejects(writeChecksums(out), /one version/);
}));

// Only bsdtar writes zip archives; Windows always ships it, so the Windows format is exercised there.
// 只有 bsdtar 能写 zip；Windows 始终自带它，因此在 Windows 上验证 Windows 格式。
test('a Windows archive is a zip with the distribution at its root', { skip: process.platform !== 'win32' }, () => withRoot(async root => {
  const dist = path.join(root, 'dist');
  await writeDistribution(dist, 'win32', 'x64');
  const archive = await createReleaseArchive(dist, path.join(root, 'out'), '0.1.0');
  assert.equal((await readFile(archive)).subarray(0, 4).toString('latin1'), 'PK\u0003\u0004');
  const listing = execFileSync(path.join(process.env.SystemRoot, 'System32', 'tar.exe'), ['-tf', archive], { encoding: 'utf8' })
    .split(/\r?\n/).filter(Boolean);
  assert.ok(listing.includes('runtime-layout.json'));
  assert.ok(listing.includes('desktop/runtime/payload.txt'));
}));

test('the glibc floor is the newest symbol version any binary references', () => withRoot(async root => {
  const bodies = ['GLIBC_2.17\0GLIBC_2.34\0', 'GLIBC_2.9\0GLIBC_2.28\0', 'no versions'];
  const files = bodies.map((_, index) => path.join(root, `elf-${index}`));
  await Promise.all(files.map((file, index) => writeFile(file, bodies[index])));
  assert.equal(await requiredGlibc(files), '2.34');
  assert.equal(await requiredGlibc([files[2]]), null);
}));
