// Check download integrity and offline reuse without touching the real distribution.
// 验证下载完整性与离线复用，不触碰真实发行目录。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { managedNodeVersions, prepareManagedNode, verifyDownload } from './managed-node.mjs';

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
  const node = path.join(root, 'dependencies/runtimes/node', `node-${managedNodeVersions.node_version}-${platform}`);
  const pnpm = path.join(root, 'dependencies/runtimes/node', `pnpm-${managedNodeVersions.pnpm_version}`);
  const executable = process.platform === 'win32' ? 'node.exe' : 'bin/node';
  for (const [directory, runtime, version, targetPlatform, entry] of [
    [node, 'node', managedNodeVersions.node_version, platform, executable],
    [pnpm, 'pnpm', managedNodeVersions.pnpm_version, 'any', 'bin/pnpm.cjs'],
  ]) {
    await mkdir(path.dirname(path.join(directory, entry)), { recursive: true });
    await writeFile(path.join(directory, entry), '');
    await writeFile(path.join(directory, 'runtime-manifest.json'), JSON.stringify({ schema_version: 1, runtime, version, platform: targetPlatform, executable: entry }));
  }
  t.mock.method(globalThis, 'fetch', () => { throw new Error('Unexpected network request'); });
  assert.deepEqual(await prepareManagedNode(root), { executable: path.join(node, executable), pnpm: path.join(pnpm, 'bin/pnpm.cjs') });
  await writeFile(path.join(node, 'runtime-manifest.json'), '{}');
  await assert.rejects(prepareManagedNode(root), /Invalid managed runtime/);
});
