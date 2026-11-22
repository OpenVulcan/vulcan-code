import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtempDisposable } from 'node:fs/promises';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

// This file is both a Node test and the Electron application that exercises its real renderer.
// 本文件既是 Node 测试，也是用于检验真实渲染进程的 Electron 应用。
if (process.versions.electron) {
  const { app, BrowserWindow } = await import('electron');
  // Start a hidden sandboxed window exercising the same Node isolation as the desktop client.
  // 启动隐藏的沙箱窗口，检验与桌面客户端相同的 Node 隔离。
  // Electron emits ready only after its main module finishes loading.
  // Electron 在主模块加载完成后才发出 ready。
  void app.whenReady().then(async () => {
    const window = new BrowserWindow({
      show: false,
      webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false },
    });
    await window.loadURL('data:text/html,<title>runtime-fixture</title><main>loaded</main>');
    // Report only actual binary and renderer observations to the parent test.
    // 仅向父测试报告从实际二进制和渲染进程观察到的结果。
    const renderer = await window.webContents.executeJavaScript(
      "({ title: document.title, content: document.querySelector('main').textContent, node: typeof require })",
    );
    process.stdout.write(JSON.stringify({ electron: process.versions.electron, renderer }), () => app.exit(0));
  }).catch((error) => {
    process.stderr.write(String(error), () => app.exit(1));
  });
} else {
  const { test } = await import('node:test');
  test('runs the installed Electron binary and loads a local sandboxed document', async () => {
    // Isolate the smoke test from every installed user's profile and process.
    // 将冒烟测试与所有已安装用户的资料和进程隔离。
    const require = createRequire(import.meta.url);
    const profile = await mkdtempDisposable(path.join(os.tmpdir(), 'vulcan-electron-smoke-'));
    try {
      const { stdout } = await promisify(execFile)(
        require('electron'),
        [fileURLToPath(import.meta.url), `--user-data-dir=${profile.path}`, '--disable-gpu'],
        { windowsHide: true, timeout: 20_000 },
      );
      // The binary reports its own version, so a stale extracted runtime cannot pass.
      // 版本由二进制自身报告，防止解包后残留的旧运行时通过测试。
      assert.deepEqual(JSON.parse(stdout), {
        electron: require('electron/package.json').version,
        renderer: { title: 'runtime-fixture', content: 'loaded', node: 'undefined' },
      });
    } finally {
      await profile.remove();
    }
  });
}
