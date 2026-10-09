import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { JUDGE_RUNTIME_MANIFEST, resolveToolPlatform, verifyJudgeRuntime } from './install-locked-tools.mjs';

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

// Every declared judge runtime installs both of its libraries from pinned HTTPS archives.
// 每个已声明的判定执行库都从固定 HTTPS 归档安装其两个库。
test('declared judge runtimes map both libraries to locked archive members', async () => {
  // lock supplies the declared targets rather than a duplicated host list.
  // lock 提供已声明目标，避免重复宿主清单。
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  const supported = new Set(['win32', 'linux', 'darwin'].flatMap(os => ['x64', 'arm64'].map(cpu => resolveToolPlatform(os, cpu).target)));
  const targets = Object.entries(lock.judge_runtime.targets);
  assert.ok(targets.length > 0);
  for (const [target, runtime] of targets) {
    assert.ok(supported.has(target), `undeclared host target ${target}`);
    // destinations lists every installed path so each library must come from exactly one member.
    // destinations 列出每个安装路径，因此每个库都必须恰好来自一个成员。
    const destinations = runtime.archives.flatMap(archive => Object.values(archive.files));
    assert.equal(new Set(destinations).size, destinations.length, `duplicate destinations in ${target}`);
    for (const library of [runtime.core_library, runtime.webgpu_library]) {
      assert.equal(destinations.filter(destination => destination === library).length, 1, `${target} lacks ${library}`);
    }
    for (const archive of runtime.archives) {
      assert.match(archive.sha256, /^[a-f0-9]{64}$/);
      assert.ok(archive.url.startsWith('https://'));
      for (const member of Object.keys(archive.files)) assert.ok(!member.startsWith('./') && !member.includes('..'), member);
    }
  }
});

// Synchronization accepts an installed judge runtime only while every recorded file still matches its digest.
// 同步仅在每个已记录文件仍与其摘要一致时接受已安装的判定执行库。
test('judge runtime verification rejects missing and altered files', async () => {
  // lock supplies the declared destinations of this host; undeclared hosts have nothing to verify.
  // lock 提供本宿主声明的目标路径；未声明的宿主无需校验。
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  const target = lock.judge_runtime.targets[resolveToolPlatform().target];
  const root = await mkdtemp(path.join(tmpdir(), 'vulcan-judge-runtime-'));
  try {
    if (!target) {
      assert.equal(await verifyJudgeRuntime(root), null);
      return;
    }
    const directory = path.join(root, 'third_party', 'judge-runtime', resolveToolPlatform().target);
    await assert.rejects(verifyJudgeRuntime(root), /make deps tools/);
    // files holds synthetic contents whose digests the manifest records like the installer does.
    // files 保存合成内容，清单像安装器一样记录其摘要。
    const files = {};
    for (const destination of target.archives.flatMap(archive => Object.values(archive.files))) {
      const installed = path.join(directory, ...destination.split('/'));
      await mkdir(path.dirname(installed), { recursive: true });
      await writeFile(installed, destination);
      files[destination] = createHash('sha256').update(destination).digest('hex');
    }
    await writeFile(path.join(directory, JUDGE_RUNTIME_MANIFEST), JSON.stringify({
      schema_version: 1, onnxruntime: lock.judge_runtime.onnxruntime, webgpu_plugin: lock.judge_runtime.webgpu_plugin,
      core_library: target.core_library, webgpu_library: target.webgpu_library, files,
    }));
    assert.equal(await verifyJudgeRuntime(root), directory);
    await writeFile(path.join(directory, ...target.core_library.split('/')), 'altered');
    await assert.rejects(verifyJudgeRuntime(root), /make deps tools/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
