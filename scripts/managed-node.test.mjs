// Check download integrity and offline reuse without touching the real distribution.
// 验证下载完整性与离线复用，不触碰真实发行目录。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { managedNodeVersions, prepareManagedNode, verifyDownload } from './managed-node.mjs';
import { buildInputPaths } from './build-input-paths.mjs';

// Corrupted bytes must never reach archive extraction for either upstream checksum format.
// 两种上游校验格式下，损坏字节都不得进入归档解压。
test('rejects modified Node and pnpm downloads', () => {
  const bytes = Buffer.from('verified runtime');
  for (const [algorithm, encoding] of [['sha256', 'hex'], ['sha512', 'base64']]) {
    const expected = createHash(algorithm).update(bytes).digest(encoding);
    verifyDownload(bytes, expected, algorithm, encoding);
    assert.throws(() => verifyDownload(Buffer.from('corrupt runtime'), expected, algorithm, encoding), /checksum mismatch/);
  }
});

// Reusing a valid installation must require no registry access; mismatches remain explicit errors.
// 复用有效安装不得访问注册表；版本不匹配必须显式报错。
test('reuses exact manifests offline and rejects a changed runtime identity', async t => {
  const root = await mkdtemp(path.join(tmpdir(), 'vulcan-managed-node-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const platform = `${{win32: 'windows', linux: 'linux', darwin: 'macos'}[process.platform]}-${process.arch}`;
  // Cache and distribution are separate trees so rebuilding output must reuse persistent inputs offline.
  // 缓存与发行物是独立目录树，重建 output 必须离线复用持久输入。
  const distribution = path.join(root, 'output');
  const cache = buildInputPaths(root).managedNode;
  const node = path.join(cache, `node-${managedNodeVersions.node_version}-${platform}`);
  const pnpm = path.join(cache, `pnpm-${managedNodeVersions.pnpm_version}`);
  const executable = process.platform === 'win32' ? 'node.exe' : 'bin/node';
  for (const [directory, runtime, version, targetPlatform, entry] of [
    [node, 'node', managedNodeVersions.node_version, platform, executable],
    [pnpm, 'pnpm', managedNodeVersions.pnpm_version, 'any', 'bin/pnpm.cjs'],
  ]) {
    await mkdir(path.dirname(path.join(directory, entry)), { recursive: true });
    await writeFile(path.join(directory, entry), `${runtime}-fixture`);
    await writeFile(path.join(directory, 'runtime-manifest.json'), JSON.stringify({ schema_version: 1, runtime, version, platform: targetPlatform, executable: entry }));
  }
  t.mock.method(globalThis, 'fetch', () => { throw new Error('Unexpected network request'); });
  // Published paths must remain under output and preserve the cached executable bytes.
  // 发布路径必须仍位于 output 下，并保留缓存中的程序字节。
  const published = {
    executable: path.join(distribution, 'dependencies/runtimes/node', path.basename(node), executable),
    pnpm: path.join(distribution, 'dependencies/runtimes/node', path.basename(pnpm), 'bin/pnpm.cjs'),
  };
  assert.deepEqual(await prepareManagedNode(distribution, root), published);
  assert.equal(await readFile(published.executable, 'utf8'), 'node-fixture');
  assert.equal(await readFile(published.pnpm, 'utf8'), 'pnpm-fixture');
  await rm(distribution, { recursive: true });
  assert.deepEqual(await prepareManagedNode(distribution, root), published);
  assert.equal(await readFile(published.executable, 'utf8'), 'node-fixture');
  await writeFile(path.join(path.dirname(published.pnpm), '..', 'runtime-manifest.json'), '{}');
  await assert.rejects(prepareManagedNode(distribution, root), /Invalid managed runtime/);
  await rm(distribution, { recursive: true });
  await writeFile(path.join(node, 'runtime-manifest.json'), '{}');
  await assert.rejects(prepareManagedNode(distribution, root), /Invalid managed runtime/);
});
