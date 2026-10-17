// Verify the adapted tuple and reject mismatched local browser packages without network access.
// 离线验证适配组合，并拒绝版本不匹配的本地浏览器包。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { browserCacheRoot, browserVersions, verifyBrowserDependencies } from './browser-runtime.mjs';
import { buildInputPaths } from './build-input-paths.mjs';

// The actual Playwright registry must select a persistent directory rather than a Cargo artifact path.
// 实际 Playwright 注册表必须选择持久目录，而不是 Cargo 产物路径。
test('Chromium resolves inside the persistent browser cache', () => {
  // relative identifies the real version/platform child without freezing either version in the test.
  // relative 标识真实版本与平台子目录，不在测试中冻结任一版本。
  const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const relative = path.relative(buildInputPaths(repository).browsers, browserCacheRoot);
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative));
});

// Each fixture has only manifests: validation must never invoke a package manager or browser.
// 每个夹具仅包含清单：校验绝不能调用包管理器或浏览器。
test('rejects a mixed MCP/Playwright protocol and missing shipped dependencies', async t => {
  const root = await mkdtemp(path.join(tmpdir(), 'vulcan-browser-pins-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const [name, version] of Object.entries({ '@playwright/mcp': browserVersions.mcp, playwright: browserVersions.playwright, 'playwright-core': browserVersions.playwright })) {
    const directory = path.join(root, 'node_modules', name);
    await mkdir(directory, { recursive: true });
    await writeFile(path.join(directory, 'package.json'), JSON.stringify({ version,
      dependencies: { playwright: browserVersions.playwright, 'playwright-core': browserVersions.playwright } }));
  }
  await verifyBrowserDependencies(root);
  const driver = path.join(root, 'node_modules/playwright-core/package.json');
  await writeFile(driver, JSON.stringify({ version: '1.63.0' }));
  await assert.rejects(verifyBrowserDependencies(root), /must be/);
  await rm(driver);
  await assert.rejects(verifyBrowserDependencies(root), /ENOENT/);
});
