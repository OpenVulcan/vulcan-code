// Validate local browser behavior against real Electron pages using an isolated temporary profile.
// 使用隔离临时配置，在真实 Electron 页面上验证本地浏览器行为。
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { app, BrowserWindow } from 'electron';
import { LocalBrowserManager } from '../dist/gui/local-browser.js';

// runAcceptance starts asynchronously so Electron can finish loading the ESM entry before app readiness.
// runAcceptance 异步启动，使 Electron 在应用就绪前能够完成 ESM 入口加载。
// Returns after all native assertions and owned-resource cleanup have completed.
// 全部原生断言及所属资源清理完成后返回。
async function runAcceptance() {
  // profile is unique to this run and never reads or changes a real user's browser data.
  // profile 为本次运行独有，绝不读取或修改真实用户浏览器数据。
  const profile = await mkdtemp(path.join(tmpdir(), 'vulcan-local-browser-'));
  app.setPath('userData', profile);
  app.on('window-all-closed', () => {});

  // server provides deterministic redirects, documents, and an independently navigating child frame.
  // server 提供确定性重定向、文档和独立导航的子框架。
  const server = createServer((request, response) => {
    if (request.url === '/icon.svg') {
      response.setHeader('content-type', 'image/svg+xml');
      response.end('<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="blue"/></svg>');
      return;
    }
    if (request.url === '/redirect') {
      response.writeHead(302, { location: '/b' }).end();
      return;
    }
    response.setHeader('content-type', 'text/html; charset=utf-8');
    response.end(`<html><head><meta name="viewport" content="width=device-width, initial-scale=1"><link rel="icon" href="/icon.svg"><title>${request.url}</title></head>
      <body style="margin:0"><h1>${request.url}</h1><a href="/b">Next</a>
      ${request.url === '/a' ? '<iframe src="/frame"></iframe>' : ''}</body></html>`);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  // origin belongs only to the fixture server.
  // origin 仅属于用例服务器。
  const origin = `http://127.0.0.1:${server.address().port}`;

  // until waits for one native event-derived condition within a bounded deadline.
  // until 在有界时限内等待一个源自原生事件的条件。
  // observe returns truthy on success; returns after success or throws on timeout.
  // observe 成功时返回真值；成功后返回，超时则抛错。
  async function until(observe) {
    // deadline bounds failures, including native events that never arrive.
    // deadline 限制失败等待，包括永远未到达的原生事件。
    const deadline = Date.now() + 10000;
    while (!(await observe())) {
      if (Date.now() >= deadline) throw new Error('Timed out waiting for native browser state.');
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  }

  await app.whenReady();
  // window owns native pages without connecting to Core or opening a user's workbench.
  // window 持有原生页面，不连接 Core 或打开用户工作台。
  const window = new BrowserWindow({ show: false, width: 1000, height: 800,
    webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false } });
  // changes records snapshots emitted by the actual native manager.
  // changes 记录真实原生管理器发出的快照。
  const changes = [];
  // manager enforces the same ownership and blocked-origin policy as the desktop workbench.
  // manager 实施与桌面工作台相同的归属与来源限制。
  const manager = new LocalBrowserManager(window, new Set(['http://blocked.invalid']), (snapshot) => changes.push(snapshot));
  // passed records concrete native acceptance checks for the final report.
  // passed 记录最终报告中的具体原生验收项。
  const passed = [];
  try {
    await window.loadURL('about:blank');
    manager.setOwner('owner-a');
    // first is the tab whose history and viewport are exercised below.
    // first 是下面验证历史与视口的标签。
    const first = manager.open(`${origin}/a`);
    // firstView retains the exact object so reordering cannot accidentally recreate it.
    // firstView 保留精确对象，使排序无法意外重建页面而不被发现。
    const firstView = window.contentView.children.find((view) => 'webContents' in view && view.webContents !== window.webContents);
    await until(() => manager.snapshot().tabs[0].url === `${origin}/a` && !manager.snapshot().tabs[0].loading);
    await until(() => manager.snapshot().tabs[0].faviconUrl === `${origin}/icon.svg`);
    passed.push('native favicon events reach the renderer snapshot');
    await firstView.webContents.executeJavaScript("document.querySelector('iframe').contentWindow.location.hash = 'child-only'");
    await until(() => firstView.webContents.executeJavaScript("document.querySelector('iframe').contentWindow.location.hash === '#child-only'"));
    assert.equal(manager.snapshot().tabs[0].url, `${origin}/a`);
    passed.push('child-frame navigation never overwrites the address bar');

    manager.navigate(first, `${origin}/redirect`);
    await until(() => manager.snapshot().tabs[0].url === `${origin}/b` && !manager.snapshot().tabs[0].loading);
    assert.equal(manager.snapshot().tabs[0].canGoBack, true);
    manager.history(first, 'back');
    await until(() => manager.snapshot().tabs[0].url === `${origin}/a` && !manager.snapshot().tabs[0].loading);
    assert.equal(manager.snapshot().tabs[0].canGoForward, true);
    manager.history(first, 'forward');
    await until(() => manager.snapshot().tabs[0].url === `${origin}/b` && !manager.snapshot().tabs[0].loading);
    await firstView.webContents.executeJavaScript("history.pushState({}, '', '/b#state')");
    await until(() => manager.snapshot().tabs[0].url === `${origin}/b#state`);
    manager.history(first, 'reload');
    await until(() => !firstView.webContents.isLoading());
    assert.equal(firstView.webContents.getURL(), `${origin}/b#state`);
    passed.push('redirects, back, forward, reload, and same-document history');

    // second is a sibling that may move but must never replace the first page's native object.
    // second 是可移动的同级标签，但绝不能替换首个页面的原生对象。
    const second = manager.open('about:blank');
    manager.select(first);
    manager.move(first, second);
    assert.deepEqual(manager.snapshot().tabs.map((tab) => tab.id), [second, first]);
    assert.equal(manager.snapshot().activeId, first);
    assert.ok(window.contentView.children.includes(firstView));
    assert.equal(firstView.webContents.getURL(), `${origin}/b#state`);
    manager.move(first, second);
    assert.deepEqual(manager.snapshot().tabs.map((tab) => tab.id), [first, second]);
    passed.push('bidirectional tab reordering preserves selection and live page identity');

    manager.setOwner('owner-b');
    // foreign belongs to a different conversation and must reject every new page operation.
    // foreign 属于另一会话，必须拒绝每个新增页面操作。
    const foreign = manager.open('about:blank');
    assert.throws(() => manager.history(first, 'reload'), /another conversation/);
    assert.throws(() => manager.move(first, foreign), /another conversation/);
    assert.throws(() => manager.openDeveloperTools(first), /another conversation/);
    await assert.rejects(() => manager.openExternal(first), /another conversation/);
    manager.setOwner('owner-a');
    assert.deepEqual(manager.snapshot().tabs.map((tab) => tab.id), [first, second]);
    manager.select(first);
    passed.push('new actions enforce conversation ownership and preserve tab order on return');

    // bounds reflects actual content size so native scaling is exactly one.
    // bounds 反映真实内容尺寸，使原生缩放精确为一。
    const bounds = window.getContentBounds();
    // viewport offers less room than the requested device size to exercise fit scaling.
    // viewport 提供小于请求设备尺寸的空间，以验证适配缩放。
    const viewport = { x: 20, y: 70, width: 600, height: 500,
      viewportWidth: bounds.width, viewportHeight: bounds.height, visible: true };
    window.showInactive();
    manager.setViewport({ ...viewport, deviceSize: { width: 393, height: 852 } });
    await until(async () => await firstView.webContents.executeJavaScript('innerWidth') === 393);
    assert.equal(await firstView.webContents.executeJavaScript('innerHeight'), 852);
    assert.ok(firstView.getBounds().width < 393);
    assert.ok(firstView.getBounds().height <= 500);
    manager.navigate(first, `${origin}/a`);
    await until(() => manager.snapshot().tabs[0].url === `${origin}/a` && !manager.snapshot().tabs[0].loading);
    assert.equal(await firstView.webContents.executeJavaScript('innerWidth'), 393);
    manager.history(first, 'reload');
    await until(() => !firstView.webContents.isLoading());
    assert.equal(await firstView.webContents.executeJavaScript('innerWidth'), 393);
    manager.setViewport({ ...viewport, deviceSize: { width: 1280, height: 720 } });
    await until(async () => await firstView.webContents.executeJavaScript('innerWidth') === 1280);
    assert.throws(() => manager.setViewport({ ...viewport, deviceSize: { width: -1, height: 852 } }), /dimensions/);
    manager.setViewport({ ...viewport, deviceSize: null });
    await until(async () => await firstView.webContents.executeJavaScript('innerWidth') === 600);
    assert.equal(firstView.getBounds().height, 500);
    manager.setViewport({ ...viewport, width: 700 });
    await until(async () => await firstView.webContents.executeJavaScript('innerWidth') === 700);
    manager.setViewport({ ...viewport, visible: false });
    assert.equal(firstView.getVisible(), false);
    manager.setViewport(viewport);
    await until(async () => await firstView.webContents.executeJavaScript('innerWidth') === 600);
    passed.push('393x852 and 1280x720 are actual layout viewports; fit and exit restore native dimensions');

    // Opening a new tab while an existing viewport is visible must not touch an unready renderer.
    // 已有视口可见时打开新标签，不得访问尚未就绪的渲染器。
    console.log('Checking new tab creation with a visible viewport.');
    // added is created exactly as the browser toolbar plus button creates a blank page.
    // added 与浏览器工具栏加号一样创建空白页面。
    const added = manager.open('about:blank');
    await until(() => manager.snapshot().tabs.find((tab) => tab.id === added)?.loading === false);
    manager.close(added);
    manager.select(first);
    passed.push('new blank tab with an already visible browser viewport');

    // Rapid plus clicks exercise multiple new renderers before their dom-ready events arrive.
    // 快速连续点击加号，覆盖多个新渲染器的 dom-ready 尚未到达时的状态。
    const rapidTabs = Array.from({ length: 4 }, () => manager.open('about:blank'));
    await until(() => rapidTabs.every((id) => manager.snapshot().tabs.find((tab) => tab.id === id)?.loading === false));
    for (const id of rapidTabs) manager.close(id);
    manager.select(first);
    manager.setViewport({ ...viewport, deviceSize: { width: 393, height: 852 } });
    // responsiveTab must defer emulation while retaining the requested size for dom-ready.
    // responsiveTab 必须延迟模拟，并为 dom-ready 保留请求的尺寸。
    const responsiveTab = manager.open('about:blank');
    // responsiveView is the final child added by this exact open call.
    // responsiveView 是本次精确 open 调用新增的最后一个子视图。
    const responsiveView = window.contentView.children.at(-1);
    await until(() => manager.snapshot().tabs.find((tab) => tab.id === responsiveTab)?.loading === false);
    await until(async () => await responsiveView.webContents.executeJavaScript('innerWidth') === 393);
    assert.equal(await responsiveView.webContents.executeJavaScript('innerHeight'), 852);
    manager.close(responsiveTab);
    manager.select(first);
    manager.setViewport(viewport);
    passed.push('rapid plus clicks and new tabs in responsive mode wait for native document readiness');

    // beforeBounds proves DevTools does not dock into the page and shrink its native content area.
    // beforeBounds 证明开发者工具没有停靠到页面内并缩小其原生内容区域。
    const beforeBounds = firstView.getBounds();
    manager.openDeveloperTools(first);
    await until(() => firstView.webContents.isDevToolsOpened());
    assert.ok(firstView.webContents.devToolsWebContents);
    assert.notEqual(firstView.webContents.devToolsWebContents.id, window.webContents.id);
    assert.deepEqual(firstView.getBounds(), beforeBounds);
    await until(() => firstView.webContents.isDevToolsFocused());
    firstView.webContents.closeDevTools();
    passed.push('page developer tools open independently and leave page bounds unchanged');
    // closingContents retains the handle after Electron removes it from the destroyed view.
    // closingContents 在 Electron 从已销毁视图移除句柄后仍保留该句柄。
    const closingContents = firstView.webContents;
    manager.close(first);
    await until(() => closingContents.isDestroyed());
    assert.equal(manager.snapshot().activeId, second);
    // beforeCancel proves that declining the native prompt keeps both tab state and live resources.
    // beforeCancel 证明拒绝原生提示后，标签状态和存活资源均保持不变。
    const beforeCancel = manager.snapshot();
    assert.equal(await manager.closeOwner('owner-a', async (count) => { assert.equal(count, 1); return false; }), false);
    assert.deepEqual(manager.snapshot(), beforeCancel);
    // retiredContents includes a tab opened while the confirmation is pending.
    // retiredContents 包含确认等待期间新打开的标签。
    const retiredContents = [];
    assert.equal(await manager.closeOwner('owner-a', async () => {
      manager.open('about:blank');
      retiredContents.push(...window.contentView.children
        .map((view) => view.webContents));
      return true;
    }), true);
    assert.deepEqual(manager.snapshot().tabs, []);
    assert.equal(manager.snapshot().activeId, null);
    assert.equal(window.contentView.children.length, 1);
    assert.equal(retiredContents.filter((contents) => contents.isDestroyed()).length, 2);
    manager.setOwner('owner-b');
    assert.equal(manager.snapshot().activeId, foreign);
    assert.equal(manager.snapshot().tabs.length, 1);
    manager.setOwner('owner-a');
    assert.deepEqual(manager.snapshot().tabs, []);
    assert.equal(await manager.closeOwner('owner-a', async () => { throw new Error('Empty browser must not prompt'); }), true);
    passed.push('close all respects cancellation, destroys all confirmed-owner pages, preserves other owners and reopens empty');
    manager.initialize();
    assert.deepEqual(manager.snapshot(), { ownerSessionId: null, activeId: null, tabs: [] });
    assert.equal(window.contentView.children.length, 0);
    manager.setOwner('after-initialize');
    // reloadedId verifies that resetting the renderer does not permanently dispose the manager.
    // reloadedId 验证重置渲染界面不会永久销毁管理器。
    const reloadedId = manager.open(`${origin}/a`);
    await until(() => manager.snapshot().tabs[0]?.loading === false);
    manager.setViewport(viewport);
    await window.webContents.executeJavaScript("location.hash = 'workbench-route'");
    assert.equal(manager.snapshot().activeId, reloadedId);
    // reloadContents must be destroyed when the workbench document reloads, without React cleanup.
    // reloadContents 必须在工作台文档重载时销毁，不依赖 React 清理。
    const reloadContents = window.contentView.children.at(-1).webContents;
    window.webContents.reload();
    await until(() => reloadContents.isDestroyed());
    assert.equal(window.contentView.children.length, 0);
    assert.deepEqual(manager.snapshot(), { ownerSessionId: null, activeId: null, tabs: [] });
    passed.push('renderer initialization and full reload destroy old views; same-document routes preserve them');
    assert.ok(changes.length > 0);
    console.log(JSON.stringify({ ok: true, passed, profile }, null, 2));
  } catch (error) {
    process.exitCode = 1;
    console.error(error);
  } finally {
    manager.dispose();
    window.destroy();
    await new Promise((resolve) => server.close(resolve));
    app.exit(process.exitCode ?? 0);
  }
}

void runAcceptance().catch((error) => { console.error(error); app.exit(1); });
