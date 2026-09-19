// Verify the actual Vite workbench, its window controls, and Core independence in an isolated root.
// 在隔离根中验证真实 Vite 工作台、窗口按钮和 Core 的独立生命周期。
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { _electron as electron } from 'playwright-core';

// Derive executable locations from this harness, never from user data or a guessed process ID.
// 从本工具确定可执行文件位置，不使用用户数据或猜测的进程编号。
const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repository = path.resolve(desktop, '../..');
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-takeover-gui-'));
const executable = path.join(repository, 'target/debug/vulcan-code-service.exe');
const diagnostics = [];
const service = spawn(executable, ['--headless', `--test-root=${root}`], { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
service.stdout.on('data', chunk => diagnostics.push(chunk.toString()));
service.stderr.on('data', chunk => diagnostics.push(chunk.toString()));
let application;
// Retain the process handle because Playwright discards its dispatcher after the GUI exits.
// 保留进程句柄，因为 GUI 退出后 Playwright 会销毁其分发器。
let guiProcess;

// Retry a read-only observation until it succeeds; preserve its final error on timeout.
// 重试只读观察直至成功，超时保留最终错误。
// observe returns the accepted value or throws; timeout bounds the total observation duration.
// observe 返回已认可的值或抛错；timeout 限制观察总时长。
async function until(observe, timeout = 60_000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    try { return await observe(); } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }
}

try {
  const instance = await until(async () => {
    const discovery = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
    const health = await (await fetch(`${discovery.baseUrl}/health`)).json();
    assert.equal(health.pid, service.pid);
    assert.equal(health.status, 'ready');
    return discovery;
  }, 120_000);
  application = await electron.launch({
    executablePath: path.join(desktop, 'node_modules/electron/dist/electron.exe'),
    args: [path.join(desktop, 'dist/gui/main.js'), `--service-executable=${executable}`, `--test-root=${root}`, `--user-data-dir=${path.join(root, 'gui')}`],
    timeout: 60_000,
  });
  guiProcess = application.process();
  const page = await until(async () => {
    const workbench = application.windows().find(window => window.url().startsWith('http://127.0.0.1:17601'));
    assert.ok(workbench, 'The default debug route must load Vite without a URL override.');
    await workbench.locator('.window-controls .window-control-button').first().waitFor({ timeout: 1_000 });
    return workbench;
  });
  const connection = await page.evaluate(() => window.vulcanDesktop.service.getConnection());
  page.on('console', message => console.log('Renderer:', message.type(), message.text()));
  page.on('pageerror', error => console.error('Renderer error:', error));
  assert.equal(connection.pid, instance.pid);
  assert.equal(connection.baseUrl, instance.baseUrl);
  const native = await application.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().map(window => ({ menu: window.isMenuBarVisible(), url: window.webContents.getURL() })));
  assert.equal(native.length, 1);
  assert.equal(native[0].menu, false);
  console.log('Actual control labels:', await page.locator('.window-controls button').evaluateAll(buttons => buttons.map(button => button.getAttribute('aria-label'))));
  await page.screenshot({ path: path.join(root, 'before-controls.png') });
  await page.getByRole('button', { name: /^(最大化窗口|Maximize window)$/ }).click();
  console.log('After maximize click:', await page.evaluate(() => window.vulcanDesktop.desktop.isWindowMaximized()));
  await until(async () => assert.equal(await application.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].isMaximized()), true));
  await page.getByRole('button', { name: /^(还原窗口|Restore window)$/ }).click();
  console.log('Restore clicked');
  await until(async () => assert.equal(await application.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].isMaximized()), false));
  await page.getByRole('button', { name: /^(最小化窗口|Minimize window)$/ }).click();
  console.log('Minimize clicked');
  await until(async () => assert.equal(await application.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].isMinimized()), true));
  // Restore only to continue the test; minimize and maximize were driven by real DOM buttons.
  // 仅为继续测试执行还原；最小化和最大化均由真实网页按钮触发。
  await application.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].restore());
  await page.screenshot({ path: path.join(root, 'workbench.png') });
  await page.getByRole('button', { name: /^(关闭窗口|Close window)$/ }).click();
  await until(async () => assert.notEqual(guiProcess.exitCode, null));
  const after = await (await fetch(`${instance.baseUrl}/health`)).json();
  assert.equal(after.pid, instance.pid);
  assert.equal(after.status, 'ready');
  console.log(JSON.stringify({ root, vite: page.url(), core: instance.baseUrl, controls: 'minimize/maximize/restore/close passed', coreSurvivesClose: true }));
} catch (error) {
  if (application && guiProcess.exitCode === null) {
    for (const page of application.windows()) console.error('Window failure:', page.url(), await page.locator('body').innerText());
  }
  throw error;
} finally {
  try {
    if (application && guiProcess.exitCode === null) await application.close();
  } finally {
    service.kill();
    await writeFile(path.join(root, 'service.log'), diagnostics.join(''));
    console.log(`Evidence: ${root}`);
  }
}
