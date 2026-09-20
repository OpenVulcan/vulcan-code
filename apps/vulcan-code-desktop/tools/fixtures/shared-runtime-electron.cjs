// Hidden Electron fixture for the official Playwright Electron API, with a disposable profile.
// 官方 Playwright Electron API 的隐藏 Electron 用例，使用可丢弃画像。
const { app, BrowserWindow } = require('electron');
// The harness supplies this exact profile; never inherit a real user's Electron data directory.
// 工具提供此精确画像；绝不继承真实用户的 Electron 数据目录。
const profile = process.argv.find(argument => argument.startsWith('--profile=')).slice('--profile='.length);
app.setPath('userData', profile);
app.on('window-all-closed', () => {});
app.whenReady().then(async () => {
  // Keep the native surface hidden while retaining page execution, matching the Core host policy.
  // 保持原生表面隐藏并保留页面执行能力，与 Core 宿主策略一致。
  const window = new BrowserWindow({ show: false, width: 1280, height: 800,
    webPreferences: { sandbox: true, nodeIntegration: false, contextIsolation: true, backgroundThrottling: false } });
  await window.loadURL('about:blank');
});
