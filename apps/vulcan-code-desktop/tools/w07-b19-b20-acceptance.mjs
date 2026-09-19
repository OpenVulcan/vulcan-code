// Verify the W07 private-file and third-party-page boundaries against a real Core process.
// 使用真实 Core 进程验收 W07 私有文件与第三方页面权限边界。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { chromium } from 'playwright-core';

// Resolve the repository from this acceptance script instead of from a package output path.
// 从本验收脚本位置解析仓库，避免依赖打包产物中的脚本路径。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const serviceExecutable = path.join(repository, 'target/debug/vulcan-code-service.exe');
const runtimeLayout = path.join(repository, 'output/runtime-layout.json');
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-w07-boundary-'));
const ownerSessionId = 'w07-boundary-acceptance';
const diagnostics = [];

// Serve a page which attempts both a local file navigation and a workbench navigation.
// 提供一个同时尝试本地文件导航和工作台导航的页面。
const fixture = createServer((request, response) => {
  const target = new URL(request.url ?? '/', 'http://127.0.0.1');
  if (target.pathname === '/page') {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end('<!doctype html><meta charset="utf-8"><title>W07 boundary fixture</title><a id="workbench" target="_blank">Open workbench</a>');
    return;
  }
  response.writeHead(404).end();
});
await new Promise(resolve => fixture.listen(0, '127.0.0.1', resolve));
const pageUrl = `http://127.0.0.1:${fixture.address().port}/page`;

// Retry only read-only readiness checks until the service publishes its identity.
// 仅对只读就绪检查进行重试，直到服务发布自己的身份。
async function until(observe, timeout = 120_000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    if (service.exitCode !== null) throw new Error(`Service exited: ${service.exitCode}\n${diagnostics.join('')}`);
    try { return await observe(); } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }
}

// Start an isolated service and never touch an existing shared service instance.
// 启动隔离服务，绝不触碰已经存在的共享服务实例。
const service = spawn(serviceExecutable, ['--headless', `--test-root=${root}`], {
  env: { ...process.env, VULCAN_RUNTIME_LAYOUT: runtimeLayout },
  windowsHide: true,
  stdio: ['ignore', 'pipe', 'pipe'],
});
service.stdout.on('data', chunk => diagnostics.push(chunk.toString()));
service.stderr.on('data', chunk => diagnostics.push(chunk.toString()));

try {
  // Read the service token from the isolated discovery record and verify its PID.
  // 从隔离发现记录读取服务令牌并验证服务 PID。
  const discovery = await until(async () => {
    const document = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
    const health = await (await fetch(`${document.baseUrl}/health`)).json();
    assert.equal(health.status, 'ready');
    assert.equal(health.pid, service.pid);
    return document;
  });

  // Send one authenticated RPC while keeping the conversation identity explicit.
  // 发送一条带认证的 RPC，并始终显式携带会话身份。
  async function rpc(method, params = {}) {
    const response = await fetch(`${discovery.baseUrl}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
      body: JSON.stringify({ method, params: { ownerSessionId, ...params } }),
    });
    const body = await response.json();
    assert.equal(response.status, 200, JSON.stringify(body));
    assert.equal(body.ok, true, `${method}: ${JSON.stringify(body)}`);
    return body.result;
  }

  const session = await rpc('browser.session.create', { profileMode: 'ephemeral' });
  const lease = await rpc('browser.lease.acquire', { sessionId: session.sessionId, holder: 'w07-boundary' });
  const pages = await rpc('browser.pages.list', { sessionId: session.sessionId });
  assert.ok(pages.length > 0);
  const page = pages[0];
  await rpc('browser.session.navigate', {
    sessionId: session.sessionId,
    pageId: page.pageId,
    leaseSecret: lease.leaseSecret,
    url: pageUrl,
  });

  // The Core endpoint is only used to create an automation probe in this test.
  // 本测试仅使用 Core 暴露的端点创建自动化探针。
  const endpointFile = path.join(root, 'user-vulcan', 'code', 'browser-profiles', 'ephemeral', session.sessionId, 'DevToolsActivePort');
  const cdpPort = await until(async () => Number((await readFile(endpointFile, 'utf8')).split(/\r?\n/)[0]));
  assert.ok(Number.isInteger(cdpPort) && cdpPort > 0);
  const automationBrowser = await chromium.connectOverCDP(`http://127.0.0.1:${cdpPort}`);
  try {
    const context = automationBrowser.contexts()[0];
    const automationPage = await context.newPage();
    await automationPage.goto(pageUrl);

    // Record the actual raw-CDP file result; a successful navigation is a W07 failure.
    // 记录原始 CDP 文件导航的真实结果；导航成功即表示 W07 失败。
    const privateFile = path.join(root, 'w07-private.html');
    const privateUrl = pathToFileURL(privateFile).href;
    let privateFileBlocked = false;
    try {
      await (await import('node:fs/promises')).writeFile(privateFile, '<title>private</title>');
      await automationPage.goto(privateUrl, { timeout: 5_000 });
    } catch {
      privateFileBlocked = true;
    }

    // A third-party page must not receive the desktop preload or Node globals.
    // 第三方页面不应获得桌面预加载对象或 Node 全局对象。
    const capabilities = await automationPage.evaluate(() => ({
      desktop: 'vulcanDesktop' in window,
      node: typeof window.require !== 'undefined' || typeof window.process !== 'undefined',
    }));
    assert.deepEqual(capabilities, { desktop: false, node: false });
    await automationPage.goto(pageUrl);

    // Opening the service URL in a third-party popup must remain unprivileged.
    // 第三方页面打开服务地址的弹窗仍必须保持无特权。
    const popupPromise = context.waitForEvent('page');
    await (async () => {
      await automationPage.evaluate(url => {
        const link = document.getElementById('workbench');
        link.href = url;
        link.click();
      }, discovery.baseUrl);
    })();
    const popup = await popupPromise;
    await popup.waitForLoadState('domcontentloaded');
    assert.equal(await popup.evaluate(() => 'vulcanDesktop' in window), false);
    const unauthorizedStatus = await popup.evaluate(async () => {
      const response = await fetch('/rpc', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ method: 'browser.sessions.list', params: {} }),
      });
      return response.status;
    });
    assert.equal(unauthorizedStatus, 401);
    await popup.close();
    await automationPage.close();

    const report = {
      result: privateFileBlocked ? 'passed' : 'failed',
      B19: {
        status: privateFileBlocked ? 'unverified' : 'failed',
        detail: privateFileBlocked ? 'raw CDP file navigation was blocked' : 'raw CDP file navigation succeeded',
      },
      B20: { status: 'passed', detail: 'third-party page had no preload/Node and popup RPC returned 401' },
      sessionId: session.sessionId,
      cdpPort,
    };
    console.log(JSON.stringify(report));
    await rpc('browser.session.close', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
    if (!privateFileBlocked) process.exitCode = 1;
  } finally {
    await automationBrowser.close();
  }
} finally {
  service.kill();
  fixture.close();
}
