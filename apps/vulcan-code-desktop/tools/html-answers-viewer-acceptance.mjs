// Real Chromium acceptance of the built viewer and the production bounded capability transport.
// 已构建查看器及正式有界能力传输层的真实 Chromium 验收。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { realpath, writeFile } from 'node:fs/promises';
import { parseArgs } from 'node:util';
import { request as httpRequest } from 'node:http';
import { chromium } from 'playwright-core';

// Explicit paths keep this fixture independent of live Core discovery and user sessions.
// 显式路径让当前夹具独立于实时 Core 发现及用户会话。
const { values } = parseArgs({ options: { fixture: { type: 'string' }, bundle: { type: 'string' }, report: { type: 'string' }, screenshot: { type: 'string' }, chromium: { type: 'string' } } });
assert.ok(values.fixture && values.bundle, 'fixture and bundle paths are required');
const child = spawn(values.fixture, [values.bundle], { windowsHide: true, stdio: ['ignore', 'pipe', 'inherit'] });
const ready = new Promise((resolve, reject) => {
  const lines = createInterface({ input: child.stdout });
  child.once('error', reject);
  child.once('exit', () => reject(new Error('Viewer fixture exited before readiness')));
  lines.once('line', (line) => { lines.close(); resolve(JSON.parse(line)); });
});
let browser;
try {
  const { viewer } = await ready;
  browser = await chromium.launch({ executablePath: await realpath(values.chromium ?? chromium.executablePath()), headless: true });
  const page = await browser.newPage({ viewport: { width: 1000, height: 720 } });
  const pageErrors = [];
  page.on('pageerror', () => pageErrors.push('uncaught viewer error'));
  // Only the fixture can mint synthetic links; these never authorize a real session or RPC.
  // 只有夹具可签发合成链接；这些链接绝不授权真实会话或 RPC。
  const issued = await fetch(`${viewer}/fixture/issue`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify('zh-CN') });
  assert.equal(issued.status, 200);
  const link = await issued.json();
  await page.goto(link.viewerUrl);
  await page.frameLocator('iframe').locator('#run').waitFor();
  assert.equal(new URL(page.url()).hash, '', 'ticket must leave address bar');
  assert.equal(await page.locator('iframe').getAttribute('sandbox'), 'allow-scripts');
  await page.frameLocator('iframe').locator('#run').click();
  assert.equal(await page.frameLocator('iframe').locator('#value').textContent(), '1');
  const generated = await page.frameLocator('iframe').locator('body').evaluate(() => {
    let parentDom = 'allowed';
    try { void parent.document.body; } catch { parentDom = 'blocked'; }
    return { parentDom, node: typeof window.require, desktop: typeof window.vulcanDesktop };
  });
  assert.deepEqual(generated, { parentDom: 'blocked', node: 'undefined', desktop: 'undefined' });
  if (values.screenshot) await page.screenshot({ path: values.screenshot });
  // Twenty preview/source transitions prove frame destruction and a fresh script state on every mount.
  // 二十次预览/源码切换证明框架销毁，且每次挂载脚本状态均为全新。
  for (let cycle = 0; cycle < 20; cycle += 1) {
    await page.getByRole('button', { name: '源码', exact: true }).click();
    assert.equal(await page.locator('iframe').count(), 0);
    assert.match(await page.locator('pre code').textContent(), /<button/u);
    await page.getByRole('button', { name: '预览', exact: true }).click();
    await page.frameLocator('iframe').locator('#run').waitFor();
    assert.equal(await page.frameLocator('iframe').locator('#value').textContent(), '0');
  }
  const rendererUrl = await page.locator('iframe').getAttribute('src');
  const repeat = await browser.newPage();
  await repeat.goto(link.viewerUrl);
  await repeat.getByRole('alert').waitFor();
  assert.equal(await repeat.locator('iframe').count(), 0);
  await repeat.close();
  await page.goto('about:blank');
  // pagehide sends the release; poll the real render route until that capability becomes unusable.
  // pagehide 发送释放；轮询真实渲染路由，直到该能力不可再用。
  const deadline = Date.now() + 3000;
  let status;
  do {
    // Node fetch overwrites Sec-Fetch-Mode; use the native HTTP client for this exact transport assertion.
    // Node fetch 会覆盖 Sec-Fetch-Mode；此精确传输断言使用原生 HTTP 客户端。
    status = await new Promise((resolve, reject) => {
      const request = httpRequest(rendererUrl, { headers: { 'sec-fetch-dest': 'iframe', 'sec-fetch-mode': 'navigate' } }, (response) => {
        response.resume(); resolve(response.statusCode);
      });
      request.once('error', () => reject(new Error('Synthetic render request failed')));
      request.end();
    });
    if (status === 410) break;
    await new Promise((resolve) => setTimeout(resolve, 30));
  } while (Date.now() < deadline);
  assert.equal(status, 410, 'closing the document must release its capability');
  assert.deepEqual(pageErrors, []);
  const evidence = { browser: await browser.version(), interactive: true, fragmentRemoved: true, isolated: generated,
    remountCycles: 20, replayRejected: true, releaseStatus: status, pageErrors: 0 };
  if (values.report) await writeFile(values.report, `${JSON.stringify(evidence, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(evidence)}\n`);
} finally {
  if (browser) await browser.close();
  child.kill();
}
