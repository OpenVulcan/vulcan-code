// Exercise local files through the production Electron manager, without the user's profile or Core.
// 通过生产 Electron 管理器验证本地文件，不使用用户配置或 Core。
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { app, BrowserWindow, shell } from 'electron';
import { LocalBrowserManager } from '../dist/gui/local-browser.js';

// until polls a native observation until truthy; observe is asynchronous and failures have a fixed deadline.
// until 轮询原生观察结果直到为真；observe 为异步函数，失败具有固定截止时间。
// Returns after success or throws when the native document never reaches the expected state.
// 成功后返回；原生文档未达到预期状态时抛错。
async function until(observe) {
  // Bound native load failures to keep this standalone probe finite.
  // 限制原生加载失败的等待时间，使独立探测有界。
  const deadline = Date.now() + 10000;
  while (!(await observe())) {
    if (Date.now() >= deadline) throw new Error('Timed out waiting for local file navigation.');
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

// runAcceptance creates isolated HTML assets and tests open, navigate, relative resources and history.
// runAcceptance 创建隔离 HTML 资源，验证打开、导航、相对资源与历史记录。
// Takes no parameters; exits Electron after reporting assertions and closing its owned window.
// 无参数；报告断言并关闭所属窗口后退出 Electron。
async function runAcceptance() {
  // The temporary directory contains only this probe's browser profile and fixture files.
  // 临时目录仅包含本次探测的浏览器配置和用例文件。
  const fixture = await mkdtemp(path.join(tmpdir(), 'vulcan-local-files-'));
  app.setPath('userData', path.join(fixture, 'profile'));
  app.on('window-all-closed', () => {});
  // Special characters verify literal filename encoding rather than accidental fragments or escapes.
  // 特殊字符验证字面文件名编码，避免误作片段或转义。
  const pagePath = path.join(fixture, '鹈鹕 #100%.html');
  const pageUrl = pathToFileURL(pagePath).href;
  const nextUrl = pathToFileURL(path.join(fixture, 'next.html')).href;
  await writeFile(pagePath, '<!doctype html><meta charset="utf-8"><title>Local HTML</title>'
    + '<link rel="stylesheet" href="style.css"><script src="script.js"></script>'
    + '<body><a href="next.html">Next</a><svg width="50" height="50"><circle cx="25" cy="25" r="20"/></svg></body>');
  await writeFile(path.join(fixture, 'style.css'), 'body { background-color: rgb(1, 2, 3); }');
  await writeFile(path.join(fixture, 'script.js'), 'window.localFixtureReady = true;');
  await writeFile(path.join(fixture, 'next.html'), '<!doctype html><title>Next local page</title>next');
  await app.whenReady();
  // Hidden native views still exercise Chromium navigation and the actual production security preferences.
  // 隐藏原生视图仍覆盖 Chromium 导航和真实生产安全配置。
  const window = new BrowserWindow({ show: false, webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false } });
  const manager = new LocalBrowserManager(window, new Set(['http://127.0.0.1:17921']), () => {});
  // Record OS dispatch without opening the tester's browser or launching any file association.
  // 记录操作系统派发，不打开测试者的浏览器或启动任何文件关联程序。
  const externalUrls = [];
  const originalOpenExternal = shell.openExternal;
  shell.openExternal = async (url) => { externalUrls.push(url); };
  try {
    await window.loadURL('about:blank');
    manager.setOwner('local-file-test');
    // Opening a raw platform path must create a file URL without an intermediate HTTPS navigation.
    // 打开原始平台路径必须生成文件 URL，不经过错误的 HTTPS 导航。
    const id = manager.open(pagePath);
    const contents = window.contentView.children.at(-1).webContents;
    await until(() => contents.getURL() === pageUrl && !contents.isLoading());
    assert.equal(manager.snapshot().tabs.find((tab) => tab.id === id).url, pageUrl);
    assert.equal(await contents.executeJavaScript('window.localFixtureReady'), true);
    assert.equal(await contents.executeJavaScript('getComputedStyle(document.body).backgroundColor'), 'rgb(1, 2, 3)');
    assert.deepEqual(await contents.executeJavaScript('[typeof require, typeof process, typeof window.vulcanDesktop]'),
      ['undefined', 'undefined', 'undefined']);
    // External routing preserves the same encoded URL and never creates another embedded page.
    // 外部路由保留同一个编码 URL，且不创建额外内置页面。
    await manager.openExternalUrl(pageUrl);
    await manager.openExternal(id);
    await manager.openExternalUrl('https://example.com/page');
    assert.deepEqual(externalUrls, [pageUrl, pageUrl, 'https://example.com/page']);
    assert.equal(manager.snapshot().tabs.length, 1);
    await assert.rejects(manager.openExternalUrl('javascript:alert(1)'), /Only website/);
    await assert.rejects(manager.openExternalUrl('http://127.0.0.1:17921/private'), /Only website/);
    assert.equal(externalUrls.length, 3);
    // A real click activates the document; Chromium otherwise skips unactivated script-driven history entries.
    // 真实点击会激活文档；否则 Chromium 会跳过未激活文档的脚本导航历史条目。
    await contents.executeJavaScript("document.querySelector('a').click()", true);
    await until(() => contents.getURL() === nextUrl && !contents.isLoading());
    assert.equal(contents.navigationHistory.canGoBack(), true);
    manager.history(id, 'back');
    await until(() => contents.getURL() === pageUrl && !contents.isLoading());
    manager.navigate(id, nextUrl);
    await until(() => contents.getURL() === nextUrl && !contents.isLoading());
    manager.navigate(id, pagePath.replaceAll('\\', '/'));
    await until(() => contents.getURL() === pageUrl && !contents.isLoading());
    assert.throws(() => manager.navigate(id, 'http://127.0.0.1:17921/'), /unavailable/);
    assert.throws(() => manager.navigate(id, 'javascript:alert(1)'), /unavailable/);
    console.log(JSON.stringify({ ok: true, fixture, checks: ['raw path open', 'encoded file URL',
      'relative script and CSS', 'relative local link', 'history', 'file URL navigation',
      'forward-slash path', 'sandbox isolation', 'protected origin and scheme rejection', 'external file/website dispatch without tabs'] }));
  } finally {
    shell.openExternal = originalOpenExternal;
    manager.dispose();
    window.destroy();
  }
}

void runAcceptance().then(() => app.exit(0)).catch((error) => { console.error(error); app.exit(1); });
