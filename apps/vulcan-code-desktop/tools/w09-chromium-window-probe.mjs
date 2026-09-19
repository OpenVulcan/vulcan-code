// Probe native Chromium window hiding, restoring, and screenshot behavior on this platform.
// 检验当前平台原生 Chromium 窗口隐藏、恢复和截图行为。
import assert from 'node:assert/strict';
import { mkdtemp, readFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { chromium } from 'playwright-core';

// root is an isolated profile that never touches the user's browser data.
// root 是绝不接触用户浏览器数据的隔离画像目录。
const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-window-probe-'));
// context starts headed Chromium minimized so a real page window can be examined.
// context 以最小化状态启动有界面的 Chromium，供检验真实页面窗口。
const context = await chromium.launchPersistentContext(path.join(root, 'profile'), {
  executablePath: chromium.executablePath(), channel: 'chromium', headless: false,
  args: ['--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1', '--start-minimized'],
  viewport: { width: 1280, height: 800 },
});
try {
  // page and targetId identify the same live native Chromium page.
  // page 与 targetId 标识同一真实原生 Chromium 页面。
  const page = context.pages()[0] ?? await context.newPage();
  await page.goto('data:text/html,<title>Native window probe</title><h1>Visible page</h1>');
  const target = await context.newCDPSession(page);
  const { targetInfo } = await target.send('Target.getTargetInfo');
  const portFile = (await readFile(path.join(root, 'profile', 'DevToolsActivePort'), 'utf8')).split(/\r?\n/);
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${portFile[0]}`);
  try {
    const cdp = await browser.newBrowserCDPSession();
    const { windowId, bounds: initial } = await cdp.send('Browser.getWindowForTarget', { targetId: targetInfo.targetId });
    await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
    const minimized = await cdp.send('Browser.getWindowBounds', { windowId });
    const image = await page.screenshot({ type: 'jpeg', timeout: 5000 });
    assert.ok(image.length > 100);
    await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'normal' } });
    const restored = await cdp.send('Browser.getWindowBounds', { windowId });
    console.log(JSON.stringify({ result: 'passed', root, initial, minimized, restored, screenshotBytes: image.length }));
  } finally {
    await browser.close();
  }
} finally {
  await context.close();
}
