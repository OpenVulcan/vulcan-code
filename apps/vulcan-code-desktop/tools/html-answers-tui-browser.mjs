// Consume only the synthetic TUI link from stdin; never log URL-bearing browser errors.
// 仅消费标准输入中的合成 TUI 链接；绝不记录含地址的浏览器错误。
import assert from 'node:assert/strict';
import { realpath } from 'node:fs/promises';
import { chromium } from 'playwright-core';

// Keep the one-use capability in memory and launch the explicitly selected installed browser.
// 一次性能力仅保留于内存，启动显式选择的已安装浏览器。
const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const viewerUrl = Buffer.concat(chunks).toString('utf8');
let browser;
try {
  browser = await chromium.launch({ executablePath: await realpath(process.argv[2]), headless: true });
  const page = await browser.newPage();
  await page.goto(viewerUrl);
  await page.frameLocator('iframe').locator('#run').click();
  assert.equal(await page.frameLocator('iframe').locator('#value').textContent(), '1');
  assert.equal(new URL(page.url()).hash, '');
  assert.equal(await page.locator('iframe').getAttribute('sandbox'), 'allow-scripts');
  await page.goto('about:blank');
  process.stdout.write('TUI -> Core HTTP -> isolated browser: passed\n');
} catch {
  process.stderr.write('Synthetic TUI browser acceptance failed (capability details withheld)\n');
  process.exitCode = 1;
} finally {
  if (browser) await browser.close();
}
