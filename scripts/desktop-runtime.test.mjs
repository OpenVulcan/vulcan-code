// Targeted tests for the package input checks used by desktop-runtime.mjs.
// 针对 desktop-runtime.mjs 使用的打包输入检查编写定向测试。
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmod, mkdtemp, mkdir, readFile, rm, symlink, unlink, writeFile } from 'node:fs/promises';
import test from 'node:test';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  packageRelativePath, runtimeLayoutSchemaVersion, validateRuntimeLayoutIdentity, verifyPackagedLayout,
} from './desktop-runtime.mjs';

// Recovery must reject invalid identity even when all declared files still exist.
// 即使全部声明文件仍存在，恢复流程也必须拒绝无效身份。
test('recovery identity validation rejects unknown profiles and foreign schemas or hosts', () => {
  // layout mirrors the current fixture contract; each mutation changes one identity dimension.
  // layout 复现当前夹具契约；每次变更只改变一个身份维度。
  const layout = { schemaVersion: runtimeLayoutSchemaVersion, profile: 'debug', platform: process.platform, architecture: process.arch };
  assert.doesNotThrow(() => validateRuntimeLayoutIdentity(layout, layout.profile));
  for (const changed of [{ schemaVersion: 0 }, { profile: 'invalid' }, { platform: 'invalid' }, { architecture: 'invalid' }]) {
    // candidate is evaluated using its own profile, as recovery of an earlier generation requires.
    // candidate 使用自身模式验证，符合恢复先前一代的要求。
    const candidate = { ...layout, ...changed };
    assert.throws(() => validateRuntimeLayoutIdentity(candidate, candidate.profile), /does not match/);
  }
});

// Exercise the actual read-only layout CLI used by Core restart, including incompatible identities.
// 验证核心重启使用的真实只读布局命令，包含不匹配的布局身份。
// The test owns its isolated root and returns after checking each command's exit status and output.
// 测试独占隔离目录，检查每次命令退出状态和输出后返回。
test('core restart layout reader shares current schema and rejects mismatched identities', async (t) => {
  // All fixtures stay beneath the same target boundary enforced by the production CLI.
  // 所有夹具均位于生产命令强制要求的同一目标目录边界内。
  const script = fileURLToPath(new URL('./desktop-runtime.mjs', import.meta.url));
  const target = path.resolve(path.dirname(script), '..', 'target');
  await mkdir(target, { recursive: true });
  const root = await mkdtemp(path.join(target, 'core-layout-test-'));
  t.after(() => rm(root, { recursive: true }));
  // The current emitted schema needs no browser or desktop artifacts for this metadata read.
  // 当前产出的布局架构在该元数据读取中无需浏览器或桌面制品。
  const layout = {
    schemaVersion: runtimeLayoutSchemaVersion, profile: 'debug', platform: process.platform, architecture: process.arch,
    serviceExecutable: 'bin/vulcan-code-service.exe',
  };
  for (const override of [{}, { schemaVersion: runtimeLayoutSchemaVersion - 1 }, { profile: 'release' }, { platform: 'invalid' }, { architecture: 'invalid' }]) {
    // Each candidate has exactly one identity mismatch, except the first valid control case.
    // 除首个有效对照场景外，每个候选均只有一个身份字段不匹配。
    const candidate = { ...layout, ...override };
    await writeFile(path.join(root, 'runtime-layout.json'), JSON.stringify(candidate));
    const result = spawnSync(process.execPath, [script, 'layout', 'debug'], {
      env: { ...process.env, VULCAN_DESKTOP_DISTRIBUTION_ROOT: root }, encoding: 'utf8',
    });
    if (Object.keys(override).length === 0) {
      assert.equal(result.status, 0, result.stderr);
      assert.deepEqual(JSON.parse(result.stdout), layout);
    } else {
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /does not match the requested profile, platform, or architecture/);
    }
  }
});

test('rejects an isolated distribution root outside generated target artifacts', () => {
  const script = fileURLToPath(new URL('./desktop-runtime.mjs', import.meta.url));
  const attemptedRoot = path.parse(script).root;
  const result = spawnSync(process.execPath, [script, 'package', 'release'], {
    env: { ...process.env, VULCAN_DESKTOP_DISTRIBUTION_ROOT: attemptedRoot },
    encoding: 'utf8',
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must be inside the repository target directory/);
});

// Debug and release must resolve to different sibling roots so neither can stop or overwrite the other.
// 调试版与正式版必须解析到不同的同级目录，使任一方都无法停止或覆盖另一方。
test('each profile resolves its own sibling distribution root', () => {
  const script = fileURLToPath(new URL('./desktop-runtime.mjs', import.meta.url));
  // The override is removed so the default per-profile directories are what the CLI reports.
  // 移除覆盖变量，使命令报告的是各模式的默认目录。
  const { VULCAN_DESKTOP_DISTRIBUTION_ROOT: _ignored, ...environment } = process.env;
  const resolve = (...args) => {
    const result = spawnSync(process.execPath, [script, 'root', ...args], { env: environment, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout;
  };
  const repository = path.resolve(path.dirname(script), '..');
  assert.equal(resolve(), path.join(repository, 'output'));
  assert.equal(resolve('debug'), path.join(repository, 'output'));
  const release = resolve('release');
  assert.equal(release, path.join(repository, 'output-release'));
  // A sibling shares the parent but never nests inside, so process ownership by prefix cannot overlap.
  // 同级目录共享父目录但互不嵌套，因此按前缀判定的进程归属不会重叠。
  assert.equal(path.dirname(release), path.dirname(resolve('debug')));
  assert.ok(!release.startsWith(resolve('debug') + path.sep));
});

test('rejects an unknown profile instead of falling back to the debug distribution', () => {
  const script = fileURLToPath(new URL('./desktop-runtime.mjs', import.meta.url));
  const result = spawnSync(process.execPath, [script, 'root', 'staging'], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Usage: node scripts\/desktop-runtime\.mjs/);
});

test('prints one validated isolated root and refuses a linked cleanup boundary', async (t) => {
  // script and target derive from the same repository entry used by the actual build scripts.
  // script 与 target 来自真实构建脚本使用的同一仓库入口。
  const script = fileURLToPath(new URL('./desktop-runtime.mjs', import.meta.url));
  const target = path.resolve(path.dirname(script), '..', 'target');
  await mkdir(target, { recursive: true });
  // ownedRoot and outside are unique test-owned paths; the link is removed before either tree.
  // ownedRoot 与 outside 是测试独占路径；清理任一目录树前先移除目录链接。
  const ownedRoot = await mkdtemp(path.join(target, 'desktop-root-test-'));
  const outside = await mkdtemp(path.join(os.tmpdir(), 'desktop-root-outside-'));
  const linkedRoot = path.join(ownedRoot, 'linked');
  await symlink(outside, linkedRoot, process.platform === 'win32' ? 'junction' : 'dir');
  t.after(async () => {
    await unlink(linkedRoot);
    await rm(ownedRoot, { recursive: true });
    await rm(outside, { recursive: true });
  });
  // valid confirms the CLI prints the exact root consumed by Unix sync and build scripts.
  // valid 确认命令输出 Unix 同步与构建脚本实际使用的精确根目录。
  const valid = spawnSync(process.execPath, [script, 'root'], {
    env: { ...process.env, VULCAN_DESKTOP_DISTRIBUTION_ROOT: ownedRoot },
    encoding: 'utf8',
  });
  assert.equal(valid.status, 0);
  assert.equal(valid.stdout, ownedRoot);
  // escaped must fail before a recursive sync can follow an existing junction or symlink.
  // escaped 必须在递归同步跟随已有联接或符号链接之前失败。
  const escaped = spawnSync(process.execPath, [script, 'root'], {
    env: { ...process.env, VULCAN_DESKTOP_DISTRIBUTION_ROOT: path.join(linkedRoot, 'child') },
    encoding: 'utf8',
  });
  assert.notEqual(escaped.status, 0);
  assert.match(escaped.stderr, /contains a link or non-directory/);
});

// The fixture mirrors the relative entries emitted by the package layout.
// 测试夹具复现打包清单生成的相对路径条目。
const fixtureLayout = Object.freeze({
  profile: 'debug',
  platform: 'win32',
  architecture: 'x64',
  serviceExecutable: 'bin/vulcan-code-service.exe',
  desktopExecutable: 'desktop/runtime/electron.exe',
  desktopEntry: 'desktop/runtime/resources/app/gui/main.js',
  tuiExecutable: 'bin/vulcan-tui.exe',
  browserHostEntry: 'browser/runtime/chromium-host/main.js',
  browserHostExecutable: 'dependencies/runtimes/node/node-24.18.0-windows-x64/node.exe',
  chromiumExecutable: 'browser/chromium/chrome-win64/chrome.exe',
  webuiRoot: 'webui',
});

// Release fixtures include the runtime resources that are intentionally optional in debug mode.
// Release 夹具包含调试模式有意可选、但发布模式必须存在的运行时资源。
function releaseLayout(chromium = true) {
  return {
    ...fixtureLayout,
    profile: 'release',
    capabilities: chromium ? ['desktop', 'browser-host', 'webui'] : ['desktop', 'webui'],
    chromiumExecutable: chromium ? fixtureLayout.chromiumExecutable : null,
  };
}

// Create a complete detached distribution fixture with files and directories of the right kind.
// 创建包含正确文件和目录类型的完整脱离源码发行夹具。
async function createFixture() {
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-desktop-runtime-'));
  const files = [
    fixtureLayout.serviceExecutable,
    fixtureLayout.desktopExecutable,
    fixtureLayout.desktopEntry,
    fixtureLayout.tuiExecutable,
    'desktop/runtime/resources/app/preload.cjs',
    fixtureLayout.browserHostEntry,
    fixtureLayout.browserHostExecutable,
    fixtureLayout.chromiumExecutable,
  ];
  for (const relative of files) {
    const filename = path.join(root, relative);
    await mkdir(path.dirname(filename), { recursive: true });
    await writeFile(filename, 'fixture');
  }
  await mkdir(path.join(root, fixtureLayout.webuiRoot), { recursive: true });
  return root;
}

// Create a release-shaped fixture with an explicit managed Node manifest and Chromium inventory.
// 创建带有明确受管 Node 清单及 Chromium 文件清单的发布形状夹具。
async function createReleaseFixture({ chromium = true } = {}) {
  const root = await createFixture();
  for (const relative of ['resources', 'plugins', 'lua_runtime', 'tools']) {
    await mkdir(path.join(root, relative), { recursive: true });
  }
  const nodeRoot = path.join(root, 'dependencies/runtimes/node/node-24.18.0-windows-x64');
  await mkdir(nodeRoot, { recursive: true });
  await writeFile(path.join(nodeRoot, 'node.exe'), 'node');
  await writeFile(path.join(nodeRoot, 'runtime-manifest.json'), JSON.stringify({
    schema_version: 1, runtime: 'node', version: '24.18.0', platform: 'windows-x64', executable: 'node.exe',
  }));
  // judgeTarget is the lock's Windows x64 runtime, matching the fixture host.
  // judgeTarget 是锁文件中与夹具宿主一致的 Windows x64 执行库。
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  const judgeTarget = lock.judge_runtime.targets['x86_64-pc-windows-msvc'];
  for (const relative of ['judge-runtime.json', judgeTarget.core_library, judgeTarget.webgpu_library]) {
    await mkdir(path.dirname(path.join(root, 'judge-runtime', relative)), { recursive: true });
    await writeFile(path.join(root, 'judge-runtime', relative), 'judge');
  }
  const pythonRoot = path.join(root, 'dependencies/runtimes/python/cpython-3.14.6-windows-x64');
  await mkdir(pythonRoot, { recursive: true });
  await writeFile(path.join(pythonRoot, 'python.exe'), 'python');
  await writeFile(path.join(pythonRoot, 'runtime-manifest.json'), JSON.stringify({
    schema_version: 1, runtime: 'python', version: '3.14.6', platform: 'windows-x64', executable: 'python.exe',
  }));
  if (chromium) {
    const sidecar = 'browser/chromium/chrome-win64/chrome.dll';
    await writeFile(path.join(root, sidecar), 'sidecar');
    await writeFile(path.join(root, 'browser/chromium/package-manifest.json'), JSON.stringify({
      schemaVersion: 1,
      executable: fixtureLayout.chromiumExecutable,
      version: '153.0.8010.12',
      files: [
        { path: fixtureLayout.chromiumExecutable, bytes: 7 },
        { path: sidecar, bytes: 7 },
      ],
    }));
  }
  return root;
}

// Remove the temporary fixture after each test, including nested generated paths.
// 每个测试后删除临时夹具及其嵌套生成路径。
async function removeFixture(root) {
  await rm(root, { recursive: true, force: true });
}

// Exercise the real terminal entrypoint with disposable executables inside the allowed target boundary.
// 在允许的 target 边界内，使用可丢弃可执行文件验证真实终端入口。
// Both commands use complete layouts; no user service, database, GUI, or network provider is started.
// 两个命令均使用完整布局；不启动用户服务、数据库、GUI 或联网供应商。
for (const command of ['service', 'tui']) {
  test(`terminal ${command} launches the declared headless/terminal process only`, { skip: process.platform === 'win32' }, async t => {
    // All generated files belong to one exact temporary child of repository target.
    // 所有生成文件属于仓库 target 下一个精确临时子目录。
    const script = fileURLToPath(new URL('./desktop-runtime.mjs', import.meta.url));
    const target = path.resolve(path.dirname(script), '..', 'target');
    await mkdir(target, { recursive: true });
    const root = await mkdtemp(path.join(target, 'terminal-runtime-test-'));
    assert.equal(path.dirname(root), target);
    t.after(() => rm(root, { recursive: true, force: true }));
    // Keep the same relative path shape as the complete package fixture.
    // 保持与完整发行夹具相同的相对路径形状。
    const layout = { ...fixtureLayout, schemaVersion: runtimeLayoutSchemaVersion, platform: process.platform, architecture: process.arch };
    for (const relative of [layout.serviceExecutable, layout.desktopExecutable, layout.desktopEntry, layout.tuiExecutable,
      layout.browserHostEntry, layout.browserHostExecutable, layout.chromiumExecutable,
      path.posix.join(path.posix.dirname(layout.desktopEntry), '..', 'preload.cjs')]) {
      await mkdir(path.dirname(path.join(root, relative)), { recursive: true });
      await writeFile(path.join(root, relative), 'fixture');
    }
    await mkdir(path.join(root, layout.webuiRoot));
    await writeFile(path.join(root, 'runtime-layout.json'), JSON.stringify(layout));
    // This description follows the actual service's code-directory ownership contract.
    // 此描述遵循真实服务对 code 目录的归属契约。
    const description = { protocolVersion: 1, profile: 'debug', executablePath: path.join(root, layout.serviceExecutable),
      dataRoot: path.join(root, 'user/code'), discoveryPath: path.join(root, 'user/code/webui-service.json'), modes: ['--headless'] };
    await writeFile(path.join(root, layout.serviceExecutable), '#!/usr/bin/env node\n'
      + `const fs = require('node:fs'); const description = ${JSON.stringify(description)};\n`
      + `if (process.argv.includes('--describe')) console.log(JSON.stringify(description)); else fs.writeFileSync(${JSON.stringify(path.join(root, 'service-args.json'))}, JSON.stringify(process.argv.slice(2)));\n`, { mode: 0o755 });
    // Reuse is represented at the launcher boundary; production health identity has separate tests.
    // 在启动器边界表达实例复用；生产健康身份具有独立测试。
    await writeFile(path.join(root, path.dirname(layout.desktopEntry), 'package.json'), '{"type":"module"}');
    await writeFile(path.join(root, path.dirname(layout.desktopEntry), 'service-launcher.js'),
      `export async function readServiceLaunchDescription() { return ${JSON.stringify(description)}; }\n`
      + 'export async function readLiveServiceInstance() { return { pid: 123, status: "ready" }; }\n');
    await writeFile(path.join(root, layout.tuiExecutable), '#!/usr/bin/env node\n'
      + `require('node:fs').writeFileSync(${JSON.stringify(path.join(root, 'tui-args.json'))}, JSON.stringify(process.argv.slice(2)));\n`, { mode: 0o755 });
    // These paths already existed as layout placeholders, so chmod must set their executable bits.
    // 这些路径此前已作为布局占位文件存在，因此必须通过 chmod 设置可执行位。
    await chmod(path.join(root, layout.serviceExecutable), 0o755);
    await chmod(path.join(root, layout.tuiExecutable), 0o755);
    const result = spawnSync(process.execPath, [script, command, 'debug'], {
      env: { ...process.env, VULCAN_DESKTOP_DISTRIBUTION_ROOT: root }, encoding: 'utf8', timeout: 30_000,
    });
    assert.equal(result.status, 0, result.stderr);
    if (command === 'service') {
      assert.deepEqual(JSON.parse(await readFile(path.join(root, 'service-args.json'), 'utf8')), ['--headless']);
    } else {
      assert.deepEqual(JSON.parse(await readFile(path.join(root, 'tui-args.json'), 'utf8')), ['--service-root', path.join(root, 'user')]);
      await assert.rejects(readFile(path.join(root, 'service-args.json')), { code: 'ENOENT' });
    }
  });
}

test('accepts a complete package layout', async (t) => {
  const root = await createFixture();
  t.after(() => removeFixture(root));
  await verifyPackagedLayout(fixtureLayout, root);
});

test('rejects a GUI package that would lose its desktop bridge on reload', async (t) => {
  // A valid GUI entry cannot compensate for a missing sandbox preload.
  // 有效的 GUI 入口不能弥补缺失的沙箱预加载文件。
  const root = await createFixture();
  t.after(() => removeFixture(root));
  await rm(path.join(root, 'desktop/runtime/resources/app/preload.cjs'));
  await assert.rejects(verifyPackagedLayout(fixtureLayout, root), /Missing package input desktopPreload/);
});

test('writes portable manifest paths from Windows distribution paths', async (t) => {
  const root = await createFixture();
  t.after(() => removeFixture(root));
  const absoluteExecutable = path.join(root, fixtureLayout.chromiumExecutable);
  assert.equal(packageRelativePath(root, absoluteExecutable), fixtureLayout.chromiumExecutable);
  assert.equal(packageRelativePath(root, path.join(root, 'browser/chromium/chrome-win64/chrome.dll')),
    'browser/chromium/chrome-win64/chrome.dll');
});

test('rejects a package without built-in Chromium', async (t) => {
  const root = await createFixture();
  t.after(() => removeFixture(root));
  const layout = { ...fixtureLayout, chromiumExecutable: null };
  await rm(path.join(root, fixtureLayout.chromiumExecutable));
  await assert.rejects(verifyPackagedLayout(layout, root), /Missing package input chromiumExecutable/);
});

test('accepts a complete release package with managed assets', async (t) => {
  const root = await createReleaseFixture();
  t.after(() => removeFixture(root));
  await verifyPackagedLayout({ ...releaseLayout() }, root);
});

test('rejects a release package without built-in Chromium', async (t) => {
  const root = await createReleaseFixture({ chromium: false });
  t.after(() => removeFixture(root));
  await assert.rejects(verifyPackagedLayout(releaseLayout(false), root), /Missing package input chromiumExecutable/);
});

test('keeps debug layout checks independent from release-only assets', async (t) => {
  const root = await createFixture();
  t.after(() => removeFixture(root));
  await verifyPackagedLayout(fixtureLayout, root);
});

for (const relative of ['resources', 'plugins', 'lua_runtime', 'tools']) {
  test(`rejects a release package missing ${relative}`, async (t) => {
    const root = await createReleaseFixture();
    t.after(() => removeFixture(root));
    await rm(path.join(root, relative), { recursive: true, force: true });
    await assert.rejects(verifyPackagedLayout(releaseLayout(), root), new RegExp(`Missing package input ${relative}`));
  });
}

test('rejects a release package without the declared judge runtime', async (t) => {
  const root = await createReleaseFixture();
  t.after(() => removeFixture(root));
  await rm(path.join(root, 'judge-runtime'), { recursive: true, force: true });
  await assert.rejects(verifyPackagedLayout(releaseLayout(), root), /Missing package input judgeRuntime/);
});

test('rejects a release package without the matching managed Node runtime', async (t) => {
  const root = await createReleaseFixture();
  t.after(() => removeFixture(root));
  await rm(path.join(root, 'dependencies/runtimes/node'), { recursive: true, force: true });
  await assert.rejects(verifyPackagedLayout(releaseLayout(), root), /Missing package input browserHostExecutable/);
});

test('rejects a release package without the matching managed Python runtime', async (t) => {
  const root = await createReleaseFixture();
  t.after(() => removeFixture(root));
  await rm(path.join(root, 'dependencies/runtimes/python'), { recursive: true, force: true });
  await assert.rejects(verifyPackagedLayout(releaseLayout(), root), /Missing package input managedPythonRuntimeRoot/);
});

test('rejects a declared Chromium package without component evidence', async (t) => {
  const root = await createReleaseFixture();
  t.after(() => removeFixture(root));
  await rm(path.join(root, 'browser/chromium/package-manifest.json'), { force: true });
  await assert.rejects(verifyPackagedLayout(releaseLayout(), root), /Missing package input chromiumPackageManifest/);
});

test('rejects a Chromium component whose copied size changed', async (t) => {
  const root = await createReleaseFixture();
  t.after(() => removeFixture(root));
  await writeFile(path.join(root, 'browser/chromium/chrome-win64/chrome.dll'), 'changed-size');
  await assert.rejects(verifyPackagedLayout(releaseLayout(), root), /Chromium component size changed/);
});

// Every required entry must report its manifest field and exact relative filename.
// 每个必需条目都必须报告清单字段和准确的相对文件名。
const requiredEntries = [
  ['serviceExecutable', fixtureLayout.serviceExecutable],
  ['desktopExecutable', fixtureLayout.desktopExecutable],
  ['desktopEntry', fixtureLayout.desktopEntry],
  ['tuiExecutable', fixtureLayout.tuiExecutable],
  ['browserHostEntry', fixtureLayout.browserHostEntry],
  ['browserHostExecutable', fixtureLayout.browserHostExecutable],
  ['webuiRoot', fixtureLayout.webuiRoot],
  ['chromiumExecutable', fixtureLayout.chromiumExecutable],
];

for (const [name, relative] of requiredEntries) {
  test(`rejects a missing ${name}`, async (t) => {
    const root = await createFixture();
    t.after(() => removeFixture(root));
    await rm(path.join(root, relative), { recursive: true, force: true });
    await assert.rejects(
      verifyPackagedLayout(fixtureLayout, root),
      error => {
        assert.match(error.message, new RegExp(`Missing package input ${name} .*${relative.replaceAll('\\', '\\\\')}`));
        return true;
      },
    );
  });
}

test('rejects an entry with the wrong filesystem kind', async (t) => {
  const root = await createFixture();
  t.after(() => removeFixture(root));
  await rm(path.join(root, fixtureLayout.desktopEntry));
  await mkdir(path.join(root, fixtureLayout.desktopEntry), { recursive: true });
  await assert.rejects(
    verifyPackagedLayout(fixtureLayout, root),
    { message: `Invalid package input desktopEntry ${fixtureLayout.desktopEntry}: expected a file.` },
  );
});
