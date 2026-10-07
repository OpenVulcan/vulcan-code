// Real browser acceptance of production Markdown cards, modal and HTTP transport using synthetic source.
// 以合成源码对正式 Markdown 卡片、弹窗及 HTTP 传输执行真实浏览器验收。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { realpath, writeFile } from 'node:fs/promises';
import { parseArgs } from 'node:util';
import { chromium } from 'playwright-core';

// Callers select exact local artifacts; no live Core or user project is discovered.
// 调用方选择精确本地产物；不发现运行中的 Core 或用户项目。
const { values } = parseArgs({ options: { fixture: { type: 'string' }, bundle: { type: 'string' }, chromium: { type: 'string' },
  report: { type: 'string' }, screenshot: { type: 'string' } } });
assert.ok(values.fixture && values.bundle, 'fixture and bundle required');
const child = spawn(values.fixture, [values.bundle], { windowsHide: true, stdio: ['ignore', 'pipe', 'inherit'] });
const ready = new Promise((resolve, reject) => {
  const lines = createInterface({ input: child.stdout });
  child.once('error', reject); child.once('exit', () => reject(new Error('Synthetic fixture stopped')));
  lines.once('line', (line) => { lines.close(); resolve(JSON.parse(line)); });
});
let browser;
try {
  const { viewer } = await ready;
  browser = await chromium.launch({ executablePath: await realpath(values.chromium ?? chromium.executablePath()), headless: true });
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 } });
  const errors = [];
  page.on('pageerror', () => errors.push('uncaught page error'));
  await page.goto(`${viewer}/scripts/fixtures/html-answers.html?service=${encodeURIComponent(viewer)}`);
  const open = page.getByRole('button', { name: '打开 HTML 回答', exact: true });
  await open.waitFor();
  assert.equal(await page.locator('iframe').count(), 0, 'cards must remain inert before click');
  await open.click();
  const outer = page.frameLocator('.html-answer-viewer-frame');
  const generated = outer.frameLocator('iframe');
  await page.getByRole('dialog').evaluate(async (element) => { await Promise.allSettled(element.getAnimations().map((animation) => animation.finished)); });
  await generated.locator('#run').click();
  assert.equal(await generated.locator('#value').textContent(), '1');
  assert.equal(await generated.locator('html').evaluate(() => document.compatMode), 'CSS1Compat');
  await page.getByRole('button', { name: '最大化', exact: true }).click();
  assert.equal(await generated.locator('#value').textContent(), '1', 'maximizing must retain the running frame');
  const maximized = await page.getByRole('dialog').boundingBox();
  assert.ok(maximized && maximized.x >= 0 && maximized.x + maximized.width <= 1101, 'maximized modal must fit viewport');
  await outer.getByRole('button', { name: '源码', exact: true }).click();
  assert.equal(await outer.locator('iframe').count(), 0);
  await outer.getByRole('button', { name: '预览', exact: true }).click();
  await generated.locator('#run').waitFor();
  if (values.screenshot) await page.screenshot({ path: values.screenshot });
  // Escape must cross both real frame boundaries and return keyboard focus to the opening button.
  // Esc 必须跨越两个真实框架边界，并将键盘焦点归还打开按钮。
  await generated.locator('#run').press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  assert.equal(await page.locator('iframe').count(), 0);
  await page.waitForFunction(() => document.activeElement?.textContent === '打开 HTML 回答');
  // Pace full grant lifecycles within Core's real per-client rate limit; never disable admission for testing.
  // 按真实 Core 客户端速率限制安排完整授权生命周期；绝不为测试禁用准入。
  for (let cycle = 0; cycle < 20; cycle += 1) {
    await new Promise((resolve) => setTimeout(resolve, 2100));
    await open.click();
    await page.getByRole('dialog').evaluate(async (element) => { await Promise.allSettled(element.getAnimations().map((animation) => animation.finished)); });
    await generated.locator('#run').click();
    assert.equal(await generated.locator('#value').textContent(), '1');
    await page.getByRole('button', { name: '关闭', exact: true }).click();
    assert.equal(await page.locator('iframe').count(), 0);
  }
  await page.setViewportSize({ width: 390, height: 780 });
  await open.click(); await generated.locator('#run').waitFor();
  const rect = await page.getByRole('dialog').boundingBox();
  assert.ok(rect && rect.x >= 0 && rect.x + rect.width <= 391, 'mobile modal must fit viewport');
  await page.getByRole('button', { name: '关闭', exact: true }).click();
  assert.deepEqual(errors, []);
  // A failed optional chunk must leave the surrounding transcript and original source readable.
  // 可选分块失败后，周围会话与原始源码必须仍可阅读。
  const failedChunk = await browser.newPage();
  await failedChunk.route('**/assets/html-answer-card-*.js', (route) => route.abort());
  await failedChunk.goto(`${viewer}/scripts/fixtures/html-answers.html?service=${encodeURIComponent(viewer)}`);
  await failedChunk.getByRole('alert').waitFor();
  assert.equal(await failedChunk.locator('iframe').count(), 0);
  assert.ok(await failedChunk.locator('details pre').textContent());
  await failedChunk.close();
  const evidence = { browser: await browser.version(), clickRequired: true, nestedInteraction: true, maximizeRetainsState: true,
    sourceRemovesExecution: true, escapeAcrossFrames: true, focusRestored: true, standardsMode: true,
    fullOpenCloseCycles: 20, narrowViewport: true, chunkFailureContained: true, pageErrors: 0 };
  if (values.report) await writeFile(values.report, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(JSON.stringify(evidence));
} finally { await browser?.close(); child.kill(); }
