// Probe the exact standalone Chromium runtime proposed for Core-owned browser sessions.
// 检验拟用于 Core 浏览器会话的独立 Chromium 运行时。
import assert from 'node:assert/strict';
import { mkdtemp, readFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { chromium } from 'playwright-core';

// root isolates the browser profile from all user browser profiles.
// root 将浏览器资料目录与所有用户浏览器资料目录隔离。
const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-chromium-probe-'));
console.error(`probe chromium=${chromium.executablePath()} root=${root}`);
// context owns the standalone Chromium process and its persistent profile.
// context 拥有独立 Chromium 进程及其持久化资料目录。
const context = await chromium.launchPersistentContext(path.join(root, 'profile'), {
  executablePath: chromium.executablePath(),
  headless: true,
  channel: 'chromium',
  args: ['--remote-debugging-port=0'],
  viewport: { width: 1280, height: 800 },
});
// onPage keeps host dialog observation armed for pages created by an external controller.
// onPage 为外部控制端新建的页面保持宿主对话框观察。
const onPage = page => page.on('dialog', () => {});
context.on('page', onPage);
for (const page of context.pages()) onPage(page);
try {
  // portFile is Chromium's authoritative record for this profile's ephemeral CDP endpoint.
  // portFile 是 Chromium 对该资料目录临时 CDP 端点的权威记录。
  const portFile = (await readFile(path.join(root, 'profile', 'DevToolsActivePort'), 'utf8')).trim().split(/\r?\n/);
  assert.equal(portFile.length, 2);
  // browser attaches as an independent external automation consumer.
  // browser 作为独立的外部自动化使用者接入。
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${portFile[0]}`);
  try {
    // pageCreate proves generic Playwright can create a page through browser-level CDP.
    // pageCreate 证明通用 Playwright 可通过浏览器级 CDP 创建页面。
    const pageCreate = await browser.contexts()[0].newPage();
    await pageCreate.goto('data:text/html,<title>Chromium probe</title>');
    assert.equal(await pageCreate.title(), 'Chromium probe');
    const cdp = await context.newCDPSession(context.pages().find(page => page.url().includes('Chromium%20probe')) ?? context.pages().at(-1));
    const targetInfo = await cdp.send('Target.getTargetInfo');
    assert.equal(targetInfo.targetInfo.type, 'page');
    // dialogAnswer proves a synchronous JavaScript prompt reaches the controller and returns text.
    // dialogAnswer 证明同步 JavaScript prompt 到达控制端并将文本返回页面。
    pageCreate.once('dialog', async dialog => {
      assert.equal(dialog.type(), 'prompt');
      await dialog.accept('来自 Core');
    });
    const dialogAnswer = await pageCreate.evaluate(() => window.prompt('Core prompt'));
    assert.equal(dialogAnswer, '来自 Core');
    console.log(JSON.stringify({ result: 'passed', root, chromium: chromium.executablePath(), cdpPort: Number(portFile[0]), targetId: targetInfo.targetInfo.targetId, newPage: true, prompt: dialogAnswer }));
  } finally {
    await browser.close();
  }
} finally {
  await context.close();
}
