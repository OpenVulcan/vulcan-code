// Consume only the synthetic TUI link from stdin; never log URL-bearing browser errors.
// 仅消费标准输入中的合成 TUI 链接；绝不记录含地址的浏览器错误。
import assert from 'node:assert/strict';
import { mkdtemp, readFile, realpath, rmdir, unlink, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { chromium, _electron as electron } from 'playwright-core';

// Keep the one-use capability in memory and launch the explicitly selected installed browser.
// 一次性能力仅保留于内存，启动显式选择的已安装浏览器。
const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const viewerUrl = Buffer.concat(chunks).toString('utf8');
let browser;
let desktop;
let temporary;
let phase = 'launch';
/** Exercise the real card in either client and compare its exact Core identity to the TUI link.
 * 在两种客户端中验证真实卡片，比较其精确 Core 身份与 TUI 链接。 */
async function exerciseCard(page) {
  // Open the same immutable result through the actual GUI/WebUI card before consuming the TUI link.
  // 消费 TUI 链接前，通过实际 GUI/WebUI 卡片打开同一不可变结果。
  const origin = new URL(viewerUrl).origin;
  const client = phase;
  phase = `${client}:load-card`;
  await page.goto(`${origin}/scripts/fixtures/html-answers.html?service=${encodeURIComponent(origin)}`);
  await page.getByRole('button', { name: '打开 HTML 回答', exact: true }).click();
  phase = `${client}:modal`;
  const modal = page.getByRole('dialog');
  await modal.evaluate(async (element) => { await Promise.allSettled(element.getAnimations().map((animation) => animation.finished)); });
  const outer = page.locator('.html-answer-viewer-frame');
  const cardViewer = await outer.getAttribute('src');
  assert.equal(new URL(cardViewer).pathname, new URL(viewerUrl).pathname, 'same Core result identity');
  const generated = page.frameLocator('.html-answer-viewer-frame').frameLocator('iframe');
  phase = `${client}:interaction`;
  await generated.locator('#run').click();
  assert.equal(await generated.locator('#value').textContent(), '1');
  assert.equal(await generated.locator('body').evaluate(() => typeof window.vulcanDesktop), 'undefined');
  await generated.locator('#run').press('Escape');
  await modal.waitFor({ state: 'hidden' });
  phase = `${client}:closed`;
}
try {
  browser = await chromium.launch({ executablePath: await realpath(process.argv[2]), headless: true });
  const page = await browser.newPage();
  phase = 'chromium';
  await exerciseCard(page);
  phase = 'tui-viewer';
  await page.goto(viewerUrl);
  await page.frameLocator('iframe').locator('#run').click();
  assert.equal(await page.frameLocator('iframe').locator('#value').textContent(), '1');
  assert.equal(new URL(page.url()).hash, '');
  assert.equal(await page.locator('iframe').getAttribute('sandbox'), 'allow-scripts');
  await page.getByRole('button', { name: /^(复制源码|Copy source)$/u }).click();
  await page.getByRole('status').filter({ hasText: /^(已复制|Copied)$/u }).waitFor();
  await page.goto('about:blank');
  // Use the installed Electron executable and the actual built preload with production trust options.
  // 使用已安装 Electron 程序及实际已构建 preload，沿用正式信任选项。
  const preload = path.resolve(import.meta.dirname, '../dist/preload.cjs');
  await readFile(preload);
  temporary = await mkdtemp(path.join(os.tmpdir(), 'vulcan-html-clients-'));
  const entry = path.join(temporary, 'main.cjs');
  // Mirror the sole discovery IPC with the owned fixture's real health DTO; reject all child senders.
  // 使用自有夹具真实健康 DTO 镜像唯一发现 IPC；拒绝全部子框架发送者。
  await writeFile(entry, `const {app,BrowserWindow,ipcMain}=require('electron');app.whenReady().then(()=>{const win=new BrowserWindow({show:false,frame:false,webPreferences:{preload:${JSON.stringify(preload)},contextIsolation:true,nodeIntegration:false,sandbox:true}});ipcMain.handle('vulcan:service:connection',async event=>{if(event.sender!==win.webContents||event.senderFrame!==win.webContents.mainFrame)throw new Error('untrusted fixture frame');return (await fetch(${JSON.stringify(new URL('/health', viewerUrl).href)})).json();});win.showInactive();win.loadURL('about:blank');});app.on('window-all-closed',()=>app.quit());`);
  const environment = { ...process.env };
  delete environment.ELECTRON_RUN_AS_NODE;
  desktop = await electron.launch({ executablePath: await realpath(createRequire(import.meta.url)('electron')), args: [entry], env: environment });
  phase = 'electron';
  const desktopPage = await desktop.firstWindow();
  await exerciseCard(desktopPage);
  assert.equal(await desktopPage.evaluate(() => typeof window.vulcanDesktop), 'object');
  process.stdout.write('TUI + WebUI + Electron: same Core identity, real interaction, isolated content passed\n');
} catch {
  process.stderr.write(`Synthetic TUI browser acceptance failed at ${phase} (capability details withheld)\n`);
  process.exitCode = 1;
} finally {
  if (desktop) await desktop.close();
  if (browser) await browser.close();
  if (temporary) { await unlink(path.join(temporary, 'main.cjs')); await rmdir(temporary); }
}
