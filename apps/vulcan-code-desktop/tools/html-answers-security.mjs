// Real-browser regression for the production HTML answer HTTP isolation policy.
// 正式 HTML 回答 HTTP 隔离策略的真实浏览器回归。

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { chromium, firefox, _electron as electron } from 'playwright-core';

// Arguments name explicit local executables and an optional evidence file; no user service is touched.
// 参数指定明确的本地可执行文件和可选证据文件；不接触用户服务。
const { values } = parseArgs({ options: {
  fixture: { type: 'string' }, chromium: { type: 'string' }, report: { type: 'string' },
  browser: { type: 'string', multiple: true },
} });
assert.ok(values.fixture, '--fixture must name the built Rust security fixture');
// Explicit filtering supports focused reproduction; reports retain the requested browser matrix.
// 显式过滤支持定向复现；报告保留请求的浏览器矩阵。
const requestedBrowsers = values.browser ?? ['chromium', 'firefox', 'electron'];
assert.ok(requestedBrowsers.every((name) => ['chromium', 'firefox', 'electron'].includes(name)));

// Result records contain synthetic observations only, never tokens or user content.
// 结果记录仅含合成观测，绝不包含令牌或用户内容。
const results = [];
// Frozen lifecycle acceptance count, shared by execution and evidence.
// 冻结的生命周期验收次数，由执行和证据共用。
const cycleCount = 20;
// The fixture owns only three ephemeral loopback listeners and is always terminated below.
// 夹具只拥有三个临时回环监听器，并且必定在下方结束。
const child = spawn(values.fixture, [], { windowsHide: true, stdio: ['ignore', 'pipe', 'inherit'] });
// Resolve setup by its single JSON readiness line, or fail explicitly when startup exits first.
// 通过唯一 JSON 就绪行确定初始化成功；初始化先退出时明确失败。
const ready = new Promise((resolve, reject) => {
  const lines = createInterface({ input: child.stdout });
  child.once('error', reject);
  child.once('exit', (code) => reject(new Error(`Security fixture exited before readiness: ${code}`)));
  lines.once('line', (line) => { lines.close(); resolve(JSON.parse(line)); });
});

/** Exercise one actual browser page against production headers; return observable evidence.
 * 使用实际浏览器页面验证正式响应头；返回可观测证据。
 * @param {import('playwright-core').Page} page - Fresh browser or Electron workbench-like page.
 * @param {{viewer: string, renderer: string}} fixture - Synthetic loopback addresses.
 * @returns {Promise<object>} Interaction, privilege, navigation and cleanup observations.
 * @param page 新建的浏览器页或模拟工作台的 Electron 页面。
 * @param fixture 合成回环地址。
 * @returns 交互、权限、导航和清理观测。
 */
async function exercise(page, fixture) {
  await page.goto(fixture.viewer);
  assert.equal(await page.locator('iframe').count(), 0, 'unopened answers must be inert');
  // Native preload is expected only in Electron's main frame; it must never reach generated content.
  // 原生预加载只应出现在 Electron 主框架；绝不能到达生成内容。
  const mainDesktop = await page.evaluate(() => typeof window.vulcanDesktop);
  await page.locator('#open').click();
  const answer = page.frameLocator('#answer');
  await answer.locator('body[data-result]').waitFor();
  await answer.locator('#run').click();
  try { await answer.locator('#value').filter({ hasText: /^1$/ }).waitFor({ timeout: 3000 }); }
  catch (error) {
    const diagnostics = await answer.locator('#run').evaluate((button) => ({ handler: typeof button.onclick, rect: button.getBoundingClientRect().toJSON(), state: document.body.dataset.result, value: document.getElementById('value').textContent }));
    throw new Error(`${error}\nInteraction diagnostics: ${JSON.stringify(diagnostics)}`);
  }
  assert.equal(await answer.locator('#value').textContent(), '1');
  await answer.locator('body[data-result][data-fetch-blocked="true"]').waitFor();
  const privileges = JSON.parse(await answer.locator('body').getAttribute('data-result'));
  assert.deepEqual(privileges, {
    scripts: true, desktop: 'undefined', node: 'undefined', process: 'undefined',
    parentDom: 'blocked', cookie: 'blocked', storage: 'blocked', eval: 'blocked', popup: 'blocked',
  });
  await page.locator('#close').click();
  assert.equal(page.frames().length, 1, 'closing must destroy all generated descendant documents');
  // Reopening always starts a fresh page and removes every document on close.
  // 重新打开总是从新页面开始，关闭时删除全部文档。
  for (let cycle = 0; cycle < cycleCount; cycle += 1) {
    await page.locator('#open').click();
    assert.equal(await page.frameLocator('#answer').locator('#value').textContent(), '0');
    await page.locator('#close').click();
    assert.equal(page.frames().length, 1);
  }
  // Each attempted escape uses a fresh document; a bounded settle window catches delayed navigation.
  // 每次逃逸尝试使用新文档；有界稳定窗口捕捉延迟导航。
  for (const scenario of ['navigate', 'data', 'top', 'meta', 'redirect']) {
    await page.goto(`${fixture.viewer}/?case=${scenario}`);
    await page.locator('#open').click();
    if (['navigate', 'data', 'top'].includes(scenario)) {
      await page.frameLocator('#answer').locator('#run').click();
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
    assert.equal(new URL(page.url()).origin, fixture.viewer, `top-level escape: ${scenario}`);
    assert.ok(page.frames().every((frame) => !frame.url().startsWith('data:')), `data navigation escaped: ${scenario}`);
    const response = await fetch(`${fixture.viewer}/observations`);
    const observations = await response.json();
    assert.equal(observations.forbidden_requests, 0, `network escape: ${scenario}`);
    await page.locator('#close').click();
    assert.equal(page.frames().length, 1);
  }
  return { mainDesktop, privileges, closedFrames: page.frames().length, cycles: cycleCount, forbiddenRequests: 0 };
}

/** Launch a real Electron window with the application's built preload and exact trust options.
 * 启动真实 Electron 窗口，使用应用已构建的预加载及相同信任选项。
 * @param {string} directory - Owned temporary directory for the test-only Electron entrypoint.
 * @returns {Promise<import('playwright-core').ElectronApplication>} Owned Electron process.
 * @param directory 测试专用 Electron 入口的自有临时目录。
 * @returns 自有 Electron 进程。
 */
async function launchElectron(directory) {
  const preload = path.resolve(import.meta.dirname, '../dist/preload.cjs');
  await readFile(preload);
  const entry = path.join(directory, 'electron-main.cjs');
  // The fixture injects no bridge of its own; this is the real shipped preload binary.
  // 夹具不注入自身桥接；这里使用实际分发的预加载产物。
  await writeFile(entry, `const {app, BrowserWindow}=require('electron');
app.whenReady().then(()=>{const window=new BrowserWindow({show:false,frame:false,webPreferences:{preload:${JSON.stringify(preload)},contextIsolation:true,nodeIntegration:false,sandbox:true}});window.showInactive();window.loadURL('about:blank');});
app.on('window-all-closed',()=>app.quit());\n`);
  const require = createRequire(import.meta.url);
  const environment = { ...process.env };
  delete environment.ELECTRON_RUN_AS_NODE;
  return electron.launch({ executablePath: await realpath(require('electron')), args: [entry], env: environment });
}

// The task-owned temporary folder contains no credentials and is removed after all browsers exit.
// 任务自有临时目录不含凭据，在全部浏览器退出后删除。
const temporary = await mkdtemp(path.join(os.tmpdir(), 'vulcan-html-security-'));
try {
  const fixture = await ready;
  for (const [name, browserType] of [['chromium', chromium], ['firefox', firefox]]) {
    if (!requestedBrowsers.includes(name)) continue;
    try {
      // Windows packaged hosts virtualize LocalAppData; the native SxS loader needs the actual path.
      // Windows 打包宿主会虚拟化 LocalAppData；原生 SxS 加载器需要实际路径。
      const executablePath = await realpath(name === 'chromium' && values.chromium ? values.chromium : browserType.executablePath());
      const browser = await browserType.launch({ headless: true, executablePath });
      try {
        const page = await browser.newPage();
        const evidence = await exercise(page, fixture);
        assert.equal(evidence.mainDesktop, 'undefined');
        results.push({ browser: name, passed: true, version: browser.version(), ...evidence });
      } finally { await browser.close(); }
    } catch (error) {
      results.push({ browser: name, passed: false, error: String(error) });
    }
  }
  if (requestedBrowsers.includes('electron')) try {
    const app = await launchElectron(temporary);
    try {
      const page = await app.firstWindow();
      const evidence = await exercise(page, fixture);
      assert.equal(evidence.mainDesktop, 'object', 'the main frame must load the real application preload');
      results.push({ browser: 'electron', passed: true, version: await app.evaluate(() => process.versions.electron), ...evidence });
    } finally { await app.close(); }
  } catch (error) {
    results.push({ browser: 'electron', passed: false, error: String(error) });
  }
  const passed = results.every((result) => result.passed);
  const report = JSON.stringify({ passed, requestedBrowsers, results }, null, 2);
  if (values.report) await writeFile(values.report, `${report}\n`);
  process.stdout.write(`${report}\n`);
  if (!passed) process.exitCode = 1;
} finally {
  if (child.exitCode === null) { child.kill(); await once(child, 'exit'); }
  assert.equal(path.dirname(path.resolve(temporary)), path.resolve(os.tmpdir()), 'cleanup must remain inside the task temporary root');
  assert.ok(path.basename(temporary).startsWith('vulcan-html-security-'), 'cleanup requires the owned fixture directory');
  await rm(temporary, { recursive: true, force: true });
}
