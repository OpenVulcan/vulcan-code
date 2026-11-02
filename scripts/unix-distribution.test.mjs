// Exercise the real Unix build and publication chain using isolated Cargo/frontend fixtures.
// 使用隔离 Cargo 与前端夹具，执行真实 Unix 构建和发布链路。
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, copyFile, readFile, writeFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { shippedFileName, shippedRustPackages } from './shipped-rust-packages.mjs';

// cargoFixture creates only binaries selected by actual Cargo arguments; an omitted artifact simulates a broken build.
// cargoFixture 只创建实际 Cargo 参数选中的二进制；缺失制品模拟损坏构建。
const cargoFixture = `
const fs = require('node:fs');
const path = require('node:path');
const args = process.argv.slice(2);
const root = process.env.VULCAN_FIXTURE_ROOT;
const target = path.join(root, 'cargo artifacts');
if (args[0] === 'metadata') {
  process.stdout.write(JSON.stringify({ target_directory: target }));
} else {
  if (args[0] !== 'build' || !args.includes('--locked')) process.exit(91);
  const inventory = JSON.parse(fs.readFileSync(path.join(root, 'scripts/shipped-rust-packages.json'), 'utf8'));
  const profile = args.includes('--release') ? 'release' : 'debug';
  fs.mkdirSync(path.join(target, profile), { recursive: true });
  for (const item of inventory) {
    const selected = args.some((arg, index) => arg === '-p' && args[index + 1] === item.package);
    const name = item.binary ?? 'lib' + item.library + (process.platform === 'darwin' ? '.dylib' : '.so');
    if (selected && name !== process.env.VULCAN_OMIT_ARTIFACT) {
      fs.writeFileSync(path.join(target, profile, name), 'new:' + item.package);
    }
  }
}
`;

// runtimeFixture verifies the Rust inventory at the packaging boundary without Electron or a real Core process.
// runtimeFixture 在打包边界验证 Rust 清单，不运行 Electron 或真实 Core 进程。
const runtimeFixture = `
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { shippedFileName, shippedRustPackages } from './shipped-rust-packages.mjs';
// Validate the fixture layout against the requested profile; throw on mismatch and return no value.
// 按请求模式验证夹具布局；不一致时抛出异常，无返回值。
export function validateRuntimeLayoutIdentity(layout, profile) { assert.equal(layout.profile, profile); }
// Verify all current artifact contents below root; resolve without a value or reject the incomplete fixture.
// 验证 root 下全部当前制品内容；成功时完成且无值，不完整时拒绝夹具。
export async function verifyPackagedLayout(layout, root) {
  for (const item of shippedRustPackages) {
    assert.equal(await readFile(path.join(root, 'bin', shippedFileName(item)), 'utf8'), 'new:' + item.package);
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === 'root') {
    // Each profile owns a sibling root, mirroring the real resolver's one-root-per-profile contract.
    process.stdout.write(path.join(process.env.VULCAN_FIXTURE_ROOT, process.argv[3] === 'release' ? 'output-release' : 'output'));
  } else if (process.argv[2] === 'package') {
    const layout = { profile: process.argv[3] };
    const root = process.env.VULCAN_DESKTOP_DISTRIBUTION_ROOT;
    await verifyPackagedLayout(layout, root);
    await writeFile(path.join(root, 'runtime-layout.json'), JSON.stringify(layout));
  } else { process.exit(92); }
}
`;

// Each case must replace every stale executable with the selected profile's current artifact, or publish nothing.
// 每个用例必须用所选模式的当前制品替换全部旧二进制，否则不得发布任何内容。
for (const scenario of [
  { profile: 'debug', omit: '' },
  { profile: 'release', omit: '' },
  { profile: 'debug', omit: 'vulcan-tui' },
  { profile: 'debug', omit: '', electronFailure: true },
]) {
  test(`Unix ${scenario.profile} publication ${scenario.electronFailure ? 'stops before Cargo when Electron preparation fails' : scenario.omit ? 'rejects missing TUI despite stale output' : 'includes every shipped artifact'}`,
    { skip: process.platform !== 'linux' && process.platform !== 'darwin' }, async t => {
      // root owns all artifacts; sourceScripts selects only the real entry points under test.
      // root 拥有全部制品；sourceScripts 仅选择被测真实入口。
      const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan unix distribution-'));
      assert.equal(path.dirname(root), path.resolve(os.tmpdir()));
      t.after(() => rm(root, { recursive: true, force: true }));
      const sourceScripts = path.dirname(fileURLToPath(import.meta.url));
      // owned is the distribution this profile publishes into; other must stay byte-for-byte untouched.
      // owned 是该模式发布所在的发行目录；other 必须保持逐字节不变。
      const owned = scenario.profile === 'release' ? 'output-release' : 'output';
      const other = scenario.profile === 'release' ? 'output' : 'output-release';
      for (const directory of ['scripts', 'fake-bin', 'output/bin', 'output-release/bin', 'apps/vulcan-code-web/node_modules', 'apps/vulcan-code-desktop/node_modules/electron']) {
        await mkdir(path.join(root, directory), { recursive: true });
      }
      for (const script of ['build_tauri.sh', 'build-input-paths.mjs', 'publish-distribution.mjs', 'directory-publication.mjs', 'shipped-rust-packages.json', 'shipped-rust-packages.mjs']) {
        await copyFile(path.join(sourceScripts, script), path.join(root, 'scripts', script));
      }
      await writeFile(path.join(root, 'scripts/desktop-runtime.mjs'), runtimeFixture);
      await writeFile(path.join(root, 'apps/vulcan-code-desktop/node_modules/electron/install.js'),
        'process.exit(process.env.VULCAN_FAIL_ELECTRON === "1" ? 23 : 0);\n');
      await writeFile(path.join(root, 'fake-bin/cargo'), '#!' + process.execPath + '\n' + cargoFixture, { mode: 0o755 });
      await writeFile(path.join(root, 'fake-bin/pnpm'), '#!/bin/sh\nexit 0\n', { mode: 0o755 });
      for (const item of shippedRustPackages) {
        await writeFile(path.join(root, 'output/bin', shippedFileName(item)), 'old:' + item.package);
        await writeFile(path.join(root, 'output-release/bin', shippedFileName(item)), 'old:' + item.package);
      }
      const result = spawnSync('sh', [path.join(root, 'scripts/build_tauri.sh'), ...(scenario.profile === 'release' ? ['--release'] : [])], {
        cwd: root, encoding: 'utf8', timeout: 30000,
        env: { ...process.env, PATH: path.join(root, 'fake-bin') + path.delimiter + process.env.PATH,
          VULCAN_FIXTURE_ROOT: root, VULCAN_OMIT_ARTIFACT: scenario.omit, VULCAN_FAIL_ELECTRON: scenario.electronFailure ? '1' : '0' },
      });
      assert.equal(result.error, undefined);
      assert.equal(result.status, scenario.electronFailure ? 23 : scenario.omit ? 1 : 0, result.stdout + result.stderr);
      for (const item of shippedRustPackages) {
        assert.equal(await readFile(path.join(root, owned, 'bin', shippedFileName(item)), 'utf8'),
          (scenario.omit || scenario.electronFailure ? 'old:' : 'new:') + item.package);
        assert.equal(await readFile(path.join(root, other, 'bin', shippedFileName(item)), 'utf8'), 'old:' + item.package);
      }
    });
}
