// Measure which navigation schemes Playwright routing actually intercepts in pinned Chromium.
// 测量固定版本 Chromium 中 Playwright 路由实际拦截哪些导航协议。
import { mkdtemp, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { chromium } from 'playwright-core';

// root separates the probe profile and file from all real user data.
// root 将探针画像与文件同真实用户数据隔离。
const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-navigation-probe-'));
// file is a harmless local target used only to detect whether file navigation is intercepted.
// file 是仅用于检测文件导航是否被拦截的无害本地目标。
const file = path.join(root, 'probe.html');
await writeFile(file, '<title>local probe</title>');
// context uses the exact pinned browser while recording every Playwright route callback.
// context 使用精确锁定的浏览器并记录每次 Playwright 路由回调。
const context = await chromium.launchPersistentContext(path.join(root, 'profile'), {
  executablePath: chromium.executablePath(), channel: 'chromium', headless: true,
});
try {
  // intercepted collects schemes before any policy is applied.
  // intercepted 在实施策略前收集路由实际看到的协议。
  const intercepted = [];
  await context.route('**/*', async route => {
    intercepted.push(route.request().url());
    await route.continue();
  });
  const page = context.pages()[0] ?? await context.newPage();
  const results = [];
  for (const target of [
    'data:text/html,<title>data probe</title>',
    pathToFileURL(file).href,
  ]) {
    try {
      await page.goto(target, { timeout: 5000 });
      results.push({ target, title: await page.title(), error: null });
    } catch (error) {
      results.push({ target, title: null, error: String(error) });
    }
  }
  console.log(JSON.stringify({ root, intercepted, results }));
} finally {
  await context.close();
}
