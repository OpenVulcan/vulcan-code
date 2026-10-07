// Exercise compact drafts through the real Core capability transport and the built viewer.
// 通过真实 Core 授权传输及已构建查看器验证精简草稿。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { realpath, writeFile } from 'node:fs/promises';
import { parseArgs } from 'node:util';
import { chromium } from 'playwright-core';

// Explicit inputs restrict this test to an owned synthetic service, never live user sessions.
// 显式输入将测试限定于自有合成服务，绝不访问用户真实会话。
const { values } = parseArgs({ options: { fixture: { type: 'string' }, bundle: { type: 'string' }, chromium: { type: 'string' }, report: { type: 'string' }, screenshot: { type: 'string' } } });
assert.ok(values.fixture && values.bundle, 'fixture and bundle paths are required');
// The process is always hidden and reaped when the test exits.
// 进程始终隐藏，并在测试退出时回收。
const child = spawn(values.fixture, [values.bundle, '--draft'], { windowsHide: true, stdio: ['ignore', 'pipe', 'inherit'] });
// Bound fixture startup so a broken build cannot leave a local acceptance waiting indefinitely.
// 限定夹具启动时间，避免损坏构建使本地验收无限等待。
const ready = new Promise((resolve, reject) => {
  const lines = createInterface({ input: child.stdout });
  const timer = setTimeout(() => { lines.close(); reject(new Error('Draft fixture readiness timeout')); }, 20_000);
  child.once('error', error => { clearTimeout(timer); lines.close(); reject(error); });
  child.once('exit', () => { clearTimeout(timer); lines.close(); reject(new Error('Draft fixture exited before readiness')); });
  lines.once('line', line => {
    clearTimeout(timer);
    lines.close();
    try { resolve(JSON.parse(line)); } catch { reject(new Error('Invalid fixture readiness response')); }
  });
});
let browser;
try {
  const { viewer } = await ready;
  browser = await chromium.launch({ executablePath: await realpath(values.chromium ?? chromium.executablePath()), headless: true });
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 } });
  const errors = [];
  page.on('pageerror', () => errors.push('uncaught page error'));
  const issued = await fetch(`${viewer}/fixture/issue`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify('en-US') });
  assert.equal(issued.status, 200);
  const link = await issued.json();
  // Navigation errors can include the one-use fragment; expose only a fixed failure message.
  // 导航错误可能包含单次片段；仅暴露固定的失败说明。
  await page.goto(link.viewerUrl).catch(() => { throw new Error('Draft viewer navigation failed'); });
  const result = page.frameLocator('iframe');
  await result.getByRole('tab', { name: 'Dedicated', exact: true }).waitFor();
  assert.equal(new URL(page.url()).hash, '');
  assert.equal(await page.locator('iframe').getAttribute('sandbox'), 'allow-scripts');
  await result.getByRole('tab', { name: 'Dedicated', exact: true }).click();
  assert.equal(await result.getByRole('tab', { name: 'Dedicated', exact: true }).getAttribute('aria-selected'), 'true');
  assert.equal(await result.getByRole('tabpanel').count(), 1);
  await result.getByRole('tab', { name: 'Dedicated', exact: true }).press('ArrowLeft');
  assert.equal(await result.getByRole('tab', { name: 'Shared', exact: true }).getAttribute('aria-selected'), 'true');
  await result.locator('summary').press('Enter');
  assert.equal(await result.locator('details').getAttribute('open'), '');
  const isolation = await result.locator('body').evaluate(() => {
    let parentBlocked = false;
    try { void parent.document.body; } catch { parentBlocked = true; }
    return { parentBlocked, bridge: typeof window.vulcanDesktop, node: typeof window.require };
  });
  assert.deepEqual(isolation, { parentBlocked: true, bridge: 'undefined', node: 'undefined' });
  const htmlBytes = await result.locator('html').evaluate(element => new TextEncoder().encode(element.outerHTML).length);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
  assert.equal(await result.locator('html').evaluate(element => element.scrollWidth <= window.innerWidth), true, 'Narrow layout must not overflow the viewport');
  if (values.screenshot) await page.screenshot({ path: values.screenshot });
  await page.getByRole('button', { name: 'Source', exact: true }).click();
  assert.equal(await page.locator('iframe').count(), 0);
  const source = await page.locator('pre code').textContent();
  assert.ok(source.includes('```ui-tabs'));
  assert.ok(!source.includes('<style>'));
  const sourceBytes = Buffer.byteLength(source, 'utf8');
  assert.ok(sourceBytes < htmlBytes, 'Draft output omits generated layout and script bytes');
  await page.getByRole('button', { name: 'Preview', exact: true }).click();
  await result.getByRole('tab', { name: 'Shared', exact: true }).waitFor();
  assert.equal(await result.getByRole('tab', { name: 'Shared', exact: true }).getAttribute('aria-selected'), 'true');
  assert.equal(await result.locator('details').getAttribute('open'), null);
  assert.deepEqual(errors, []);
  const evidence = { browser: await browser.version(), draftBytes: sourceBytes, renderedDomBytes: htmlBytes, sourceReductionPercent: Number(((1 - sourceBytes / htmlBytes) * 100).toFixed(1)), tabsMouseAndKeyboard: true, detailsKeyboard: true, narrowDarkLayout: true, sourcePreserved: true, remountResetsState: true, isolation, pageErrors: errors.length };
  if (values.report) await writeFile(values.report, `${JSON.stringify(evidence, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(evidence)}\n`);
} finally {
  try {
    if (browser) await browser.close();
  } finally {
    if (child.pid && child.exitCode === null && child.signalCode === null) {
      await new Promise(resolve => { child.once('exit', resolve); child.kill(); });
    }
  }
}
