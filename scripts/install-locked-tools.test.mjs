import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { cp, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { JUDGE_RUNTIME_MANIFEST, bundledToolIds, installLockedTools, resolveToolPlatform, verifyJudgeRuntime } from './install-locked-tools.mjs';

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
  // The CI test runner is locked for exactly the Windows and Linux x64 runners.
  // CI 测试运行器只为 Windows 与 Linux x64 运行器锁定。
  for (const os of ['win32', 'linux']) {
    const host = resolveToolPlatform(os, 'x64');
    const matches = lock.nextest.assets.filter(asset => asset.name === `cargo-nextest-${lock.nextest.version}-${host.target}.${host.archiveKind}`);
    assert.equal(matches.length, 1);
    assert.match(matches[0].sha256, /^[a-f0-9]{64}$/);
  }
});

// RTK ships only where its lock names an archive or a source build, and each named archive is one locked asset.
// RTK 仅在锁文件为其命名归档或源码构建的宿主随附，且每个命名归档恰好对应一个锁定资产。
test('rtk targets resolve to exactly one locked archive or the source build and join the bundled set only there', async () => {
  // lock supplies the declared targets rather than a duplicated host list.
  // lock 提供已声明目标，避免重复宿主清单。
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  const supported = new Set(['win32', 'linux', 'darwin'].flatMap(os => ['x64', 'arm64'].map(cpu => resolveToolPlatform(os, cpu).target)));
  for (const [target, archive] of Object.entries(lock.tools.rtk.targets)) {
    assert.ok(supported.has(target), `undeclared host target ${target}`);
    const matches = lock.tools.rtk.assets.filter(asset => asset.name === archive);
    assert.equal(matches.length, 1, `${target} archive ${archive}`);
    assert.match(matches[0].digest, /^sha256:[a-f0-9]{64}$/);
    assert.ok(matches[0].browser_download_url.startsWith('https://'));
  }
  // A host is either archive-installed or source-built, never both, so the installer has one answer per host.
  // 宿主要么安装归档、要么源码构建，不会两者兼有，使安装器对每个宿主只有一个答案。
  for (const target of lock.tools.rtk.source_targets) {
    assert.ok(supported.has(target), `undeclared source target ${target}`);
    assert.equal(lock.tools.rtk.targets[target], undefined, `${target} is both archive and source`);
  }
  assert.ok(lock.tools.rtk.source.url.startsWith('https://'));
  assert.match(lock.tools.rtk.source.sha256, /^[a-f0-9]{64}$/);
  assert.ok(Array.isArray(lock.tools.rtk.source.build_args));
  assert.ok(lock.tools.rtk.licenses.length > 0);
  for (const target of supported) {
    const ids = bundledToolIds(lock, target);
    assert.deepEqual(ids.slice(0, 2), ['sg', 'rg']);
    const declared = Boolean(lock.tools.rtk.targets[target]) || lock.tools.rtk.source_targets.includes(target);
    assert.equal(ids.includes('rtk'), declared, target);
  }
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

test('an installation that still matches the lock is kept and an incomplete one is installed again', async t => {
  // The locked bytes come from this checkout's verified cache, so the test runs offline.
  // 锁定字节取自本检出已校验的缓存，因此测试离线运行。
  const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const cache = path.join(repository, 'third_party', 'build-input-cache');
  const root = await mkdtemp(path.join(tmpdir(), 'vulcan-locked-tools-'));
  try {
    if (existsSync(cache)) await cp(cache, path.join(root, 'third_party', 'build-input-cache'), { recursive: true });
    try {
      await installLockedTools('protoc', root, true);
    } catch (error) {
      if (!/Offline build input is absent/.test(error.message)) throw error;
      t.skip('the protoc archive is not in this checkout\'s build input cache (CI downloads it before the tests)');
      return;
    }
    const directory = path.join(root, '.cargo', 'protoc');
    const binary = path.join(directory, 'bin', `protoc${resolveToolPlatform().suffix}`);
    const installed = (await stat(binary)).mtimeMs;
    await installLockedTools('protoc', root, true);
    assert.equal((await stat(binary)).mtimeMs, installed, 'a current installation must not be replaced');
    const descriptor = path.join(directory, 'include', 'google', 'protobuf', 'descriptor.proto');
    await rm(descriptor);
    await installLockedTools('protoc', root, true);
    assert.ok(existsSync(descriptor), 'an incomplete installation must be installed again');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
