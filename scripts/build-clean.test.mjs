// Exercise profile-only cleanup and persistent downloads with real Cargo in an isolated checkout.
// 在隔离检出中使用真实 Cargo 验证按模式清理和下载输入持久性。
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { buildInputPaths } from './build-input-paths.mjs';

// Run executable with args in root; env selects only fixture-owned target paths and return its exit result.
// 在 root 中运行 executable 与 args；env 仅选择夹具持有的目标路径，返回退出结果。
function invoke(root, executable, args, env) {
  // result preserves native diagnostics, including permission failures, without any real repository cleanup.
  // result 保留原生诊断，包括权限错误，不清理真实仓库。
  const result = spawnSync(executable, args, { cwd: root, env, encoding: 'utf8', timeout: 30_000 });
  assert.equal(result.error, undefined);
  return result;
}

for (const release of [false, true]) {
  for (const route of ['make', 'powershell']) {
    test(`${route} clean ${release ? 'release' : 'debug'} preserves the other profile, output and download caches`,
      { skip: process.platform === 'win32' && route === 'make' }, t => {
        // root and artifacts are exact temporary paths; no inherited Cargo target may escape the fixture.
        // root 与 artifacts 是精确临时路径；不允许继承的 Cargo 目标越过夹具边界。
        const root = mkdtempSync(path.join(tmpdir(), 'vulcan clean-'));
        assert.equal(path.dirname(root), path.resolve(tmpdir()));
        t.after(() => rmSync(root, { recursive: true, force: true }));
        const artifacts = path.join(root, 'cargo artifacts');
        const source = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
        for (const directory of ['scripts', 'src', '.cargo']) mkdirSync(path.join(root, directory));
        for (const name of ['Makefile', 'make.sh', 'make.ps1']) {
          writeFileSync(path.join(root, name), readFileSync(path.join(source, name), 'utf8').replace(/\r\n/g, '\n'));
        }
        writeFileSync(path.join(root, 'scripts/check-build-environment.cjs'), 'throw new Error("Cleanup must not check Node/pnpm prerequisites");\n');
        writeFileSync(path.join(root, 'Cargo.toml'), '[package]\nname="cleanup-fixture"\nversion="0.1.0"\nedition="2021"\n[workspace]\n');
        writeFileSync(path.join(root, 'src/lib.rs'), '');
        writeFileSync(path.join(root, '.cargo/config.toml'), '[build]\ntarget-dir="cargo artifacts"\n');
        // Retained files represent complete download directories and the separate published application.
        // 保留文件代表完整下载目录与独立的已发布应用。
        const retained = [...Object.values(buildInputPaths(root)), path.join(root, 'output'), path.join(root, 'output-release'), path.join(artifacts, 'unrelated')];
        for (const directory of [...retained, path.join(artifacts, 'debug'), path.join(artifacts, 'release')]) {
          mkdirSync(directory, { recursive: true });
          writeFileSync(path.join(directory, 'keep.bin'), 'preserve');
        }
        // Make exercises Cargo config; direct PowerShell additionally exercises CARGO_TARGET_DIR override.
        // Make 验证 Cargo 配置；直接 PowerShell 调用还验证 CARGO_TARGET_DIR 覆盖。
        const env = { ...process.env, CARGO_TARGET_DIR: route === 'powershell' ? artifacts : '', CARGO_BUILD_TARGET: '' };
        if (route === 'make') delete env.CARGO_TARGET_DIR;
        delete env.CARGO_BUILD_TARGET;
        const args = ['clean', ...(release ? ['release'] : [])];
        const result = route === 'make' ? invoke(root, 'make', args, env)
          : invoke(root, 'pwsh', ['-NoProfile', '-File', path.join(root, 'make.ps1'), ...args], env);
        assert.equal(result.status, 0, result.stdout + result.stderr);
        assert.equal(existsSync(path.join(artifacts, release ? 'release' : 'debug', 'keep.bin')), false);
        assert.equal(readFileSync(path.join(artifacts, release ? 'debug' : 'release', 'keep.bin'), 'utf8'), 'preserve');
        for (const directory of retained) assert.equal(readFileSync(path.join(directory, 'keep.bin'), 'utf8'), 'preserve');
        // Even bare cargo clean must leave Chromium, other persistent inputs and output intact.
        // 即使直接执行 cargo clean，也必须保留 Chromium、其他持久输入和 output。
        const complete = invoke(root, 'cargo', ['clean'], env);
        assert.equal(complete.status, 0, complete.stdout + complete.stderr);
        for (const directory of retained.filter(directory => !directory.startsWith(artifacts + path.sep))) {
          assert.equal(readFileSync(path.join(directory, 'keep.bin'), 'utf8'), 'preserve');
        }
      });
  }
}
