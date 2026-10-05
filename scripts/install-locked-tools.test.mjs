import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { resolveToolPlatform } from './install-locked-tools.mjs';

// Resolve every declared host against the actual lock; missing assets cannot silently choose another host.
// 将每个声明的宿主与真实锁文件匹配；缺失资产不能静默选择其他宿主。
test('all supported hosts have exactly one locked tool archive', async () => {
  // lock supplies the identities under test rather than duplicating version constants.
  // lock 提供待测身份，避免重复版本常量。
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  for (const os of ['win32', 'linux', 'darwin']) {
    for (const cpu of ['x64', 'arm64']) {
      const host = resolveToolPlatform(os, cpu);
      assert.equal(lock.tools.sg.assets.filter(asset => asset.name === `app-${host.target}.zip`).length, 1);
      assert.equal(lock.tools.rg.assets.filter(asset => asset.name === `ripgrep-${lock.tools.rg.tag_name}-${host.rgTarget}.${host.archiveKind}`).length, 1);
      assert.equal(lock.protoc.assets.filter(asset => asset.name === `protoc-${lock.protoc.version}-${host.protoc}.zip`).length, 1);
    }
  }
  assert.throws(() => resolveToolPlatform('linux', 'ia32'), /Unsupported/);
  assert.throws(() => resolveToolPlatform('freebsd', 'x64'), /Unsupported/);
  assert.equal(lock.nextest.assets.length, 1);
  assert.equal(lock.nextest.assets[0].name, `cargo-nextest-${lock.nextest.version}-x86_64-pc-windows-msvc.zip`);
  assert.match(lock.nextest.assets[0].sha256, /^[a-f0-9]{64}$/);
});
