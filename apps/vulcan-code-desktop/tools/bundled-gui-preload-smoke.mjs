// Verify the shipped preload in a real sandboxed Electron renderer, including document reloads.
// 在真实沙箱 Electron 渲染进程中验证随包预加载，包含文档重载。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';

// distribution supplies the same preload path resolved by gui/main.js.
// distribution 提供与 gui/main.js 解析规则一致的预加载路径。
const distribution = path.resolve(process.argv[2]);
const layout = JSON.parse(await readFile(path.join(distribution, 'runtime-layout.json'), 'utf8'));
// The unbranded runtime accepts a fixture entry; a branded packaged app always boots its own entry.
// 未品牌化的运行时接受测试入口；品牌化发行应用始终启动自己的入口。
const require = createRequire(import.meta.url);
const electronExecutable = require('electron');
assert.equal(require('electron/package.json').version, layout.runtimeVersion);
const appRoot = path.resolve(distribution, path.dirname(layout.desktopEntry), '..');
// root owns only this hidden fixture's script, profile and report, never the user's GUI data.
// root 仅拥有本隐藏测试的脚本、画像与报告，绝不使用用户 GUI 数据。
const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-gui-preload-'));
const script = path.join(root, 'preload-smoke.cjs');
const report = path.join(root, 'result.json');
const environment = { ...process.env };
delete environment.ELECTRON_RUN_AS_NODE;
try {
  await writeFile(script, `
const assert = require('node:assert/strict');
const { writeFileSync } = require('node:fs');
const { once } = require('node:events');
const { app, BrowserWindow, ipcMain } = require('electron');
const { WINDOW_COMMAND_CHANNELS } = require(${JSON.stringify(path.join(appRoot, 'gui/ipc-channels.cjs'))});
// Isolate the test renderer and keep all windows hidden.
// 隔离测试渲染进程，并始终隐藏所有窗口。
app.setPath('userData', ${JSON.stringify(path.join(root, 'profile'))});
app.on('window-all-closed', () => {});
app.whenReady().then(async () => {
  const window = new BrowserWindow({ show: false, frame: false,
    webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false,
      preload: ${JSON.stringify(path.join(appRoot, 'preload.cjs'))} } });
  const preloadErrors = [];
  const actions = [];
  window.webContents.on('preload-error', (_event, _file, error) => preloadErrors.push(error.message));
  ipcMain.handle(WINDOW_COMMAND_CHANNELS.minimize, () => { actions.push('minimize'); window.minimize(); });
  ipcMain.handle(WINDOW_COMMAND_CHANNELS.toggleMaximize, () => {
    actions.push('maximize');
    if (window.isMaximized()) window.unmaximize(); else window.maximize();
  });
  ipcMain.handle(WINDOW_COMMAND_CHANNELS.isMaximized, () => window.isMaximized());
  // Reply before destruction so the renderer can finish the IPC call deterministically.
  // 销毁前先应答，使渲染进程能够确定性完成 IPC 调用。
  ipcMain.handle(WINDOW_COMMAND_CHANNELS.close, () => { actions.push('close'); setTimeout(() => window.close(), 50); });
  await window.loadURL('data:text/html,<html><body>GUI preload smoke</body></html>');
  for (let documentIndex = 0; documentIndex < 3; documentIndex++) {
    assert.deepEqual(preloadErrors, []);
    // Probe the exact bridge used by WebUI host detection after each fresh document.
    // 在每个新文档中探测 WebUI 宿主识别所使用的精确桥接对象。
    const bridge = await window.webContents.executeJavaScript(
      '({ methods: Object.keys(window.vulcanDesktop.desktop).filter(key => typeof window.vulcanDesktop.desktop[key] === "function"), notifications: typeof window.vulcanDesktop.desktop.nativeNotificationsAvailable, localBrowser: typeof window.vulcanDesktop.localBrowser.initialize })');
    for (const method of ['minimizeWindow', 'toggleWindowMaximize', 'isWindowMaximized', 'closeWindow',
      'subscribeWindowResize', 'isWindowMinimized', 'isWindowFocused', 'isWindowVisible',
      'subscribeWindowFocusChange', 'getNotificationPermission', 'requestNotificationPermission',
      'showNotification', 'subscribeNotificationActivation']) assert.ok(bridge.methods.includes(method), method);
    assert.equal(bridge.notifications, 'boolean');
    assert.equal(bridge.localBrowser, 'function');
    await window.webContents.executeJavaScript('window.vulcanDesktop.desktop.minimizeWindow()');
    await window.webContents.executeJavaScript('window.vulcanDesktop.desktop.toggleWindowMaximize()');
    assert.equal(typeof await window.webContents.executeJavaScript('window.vulcanDesktop.desktop.isWindowMaximized()'), 'boolean');
    if (documentIndex < 2) {
      const loaded = once(window.webContents, 'did-finish-load');
      window.webContents.reload();
      await loaded;
    }
  }
  const closed = once(window, 'closed');
  await window.webContents.executeJavaScript('window.vulcanDesktop.desktop.closeWindow()');
  await closed;
  assert.deepEqual(actions, ['minimize', 'maximize', 'minimize', 'maximize', 'minimize', 'maximize', 'close']);
  writeFileSync(${JSON.stringify(report)}, JSON.stringify({ result: 'passed', documents: 3, actions }));
  app.exit(0);
}).catch(error => { console.error(error); app.exit(1); });
`);
  // child is owned exclusively by this smoke test; the watchdog bounds startup and IPC waits.
  // child 仅由本冒烟测试拥有；看门狗限制启动及 IPC 等待时间。
  const child = spawn(electronExecutable, [script], {
    windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], env: environment,
  });
  let diagnostics = '';
  child.stdout.on('data', chunk => { diagnostics += chunk; });
  child.stderr.on('data', chunk => { diagnostics += chunk; });
  const timeout = setTimeout(() => child.kill(), 30_000);
  try {
    const [code] = await once(child, 'exit');
    assert.equal(code, 0, diagnostics);
    console.log(await readFile(report, 'utf8'));
  } finally { clearTimeout(timeout); }
} finally {
  await rm(root, { recursive: true, force: true });
}
