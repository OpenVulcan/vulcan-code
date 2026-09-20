// Verify the adapted tuple and reject mismatched local browser packages without network access.
// 离线验证适配组合，并拒绝版本不匹配的本地浏览器包。
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { browserVersions, verifyBrowserDependencies } from './browser-runtime.mjs';

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
