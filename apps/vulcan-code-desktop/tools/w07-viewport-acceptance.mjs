// Verify Core viewport changes against real Chromium pages, streamed frames, and edge input.
// 用真实 Chromium 页面、投影画面和边缘输入验证 Core 视口变更。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { cp, link, mkdir, mkdtemp, readFile, realpath, rm, symlink, unlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// repository identifies the checked-out source containing the packaged runtime.
// repository 标识包含打包运行时的当前源码检出。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
// packagedRoot is a read-only source for the pinned Electron and Chromium binaries.
// packagedRoot 是固定版本 Electron 与 Chromium 程序的只读来源。
const packagedRoot = path.join(repository, 'output');
// root isolates the service database and browser profile from real user data.
// root 将服务数据库和浏览器画像与真实用户数据隔离。
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-w07-viewport-'));
// layoutRoot selects the just-compiled host without changing the shared output package.
// layoutRoot 选用刚编译的宿主，不改动共享的 output 包。
const layoutRoot = await mkdtemp(path.join(repository, 'target', 'w07-viewport-layout-'));
await mkdir(path.join(layoutRoot, 'bin'), { recursive: true });
await mkdir(path.join(layoutRoot, 'browser'), { recursive: true });
await mkdir(path.join(layoutRoot, 'desktop'), { recursive: true });
await cp(path.join(repository, 'apps/vulcan-code-desktop/dist'), path.join(layoutRoot, 'compiled'), { recursive: true });
await Promise.all([
  link(path.join(repository, 'target/debug/vulcan-code-service.exe'), path.join(layoutRoot, 'bin/vulcan-code-service.exe')),
  symlink(path.join(packagedRoot, 'desktop/runtime'), path.join(layoutRoot, 'desktop/runtime'), 'junction'),
  symlink(path.join(packagedRoot, 'browser/chromium'), path.join(layoutRoot, 'browser/chromium'), 'junction'),
  symlink(path.join(packagedRoot, 'webui'), path.join(layoutRoot, 'webui'), 'junction'),
  symlink(path.join(packagedRoot, 'dependencies'), path.join(layoutRoot, 'dependencies'), 'junction'),
  symlink(await realpath(path.join(repository, 'apps/vulcan-code-desktop/node_modules')),
    path.join(layoutRoot, 'compiled/node_modules'), 'junction'),
]);
// layout preserves the packaged runtime contract while replacing only the host entry with current dist.
// layout 保留打包运行时契约，只将宿主入口替换为当前 dist。
const layout = JSON.parse(await readFile(path.join(packagedRoot, 'runtime-layout.json'), 'utf8'));
layout.browserHostEntry = 'compiled/chromium-host/main.js';
await writeFile(path.join(layoutRoot, 'runtime-layout.json'), JSON.stringify(layout, null, 2));
// serviceOutput preserves diagnostics when a real-process assertion fails.
// serviceOutput 在真实进程断言失败时保留诊断。
const serviceOutput = [];
// edgeClicks records the viewport seen by a genuine page click at the lower-right corner.
// edgeClicks 记录右下角真实页面点击所看到的视口。
const edgeClicks = [];
// fixture serves two deterministic pages and records a real browser event.
// fixture 提供两个确定性页面并记录真实浏览器事件。
const fixture = createServer((request, response) => {
  if (request.url === '/edge-clicked' && request.method === 'POST') {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', chunk => { body += chunk; });
    request.on('end', () => {
      edgeClicks.push(JSON.parse(body));
      response.writeHead(204).end();
    });
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  if (request.url === '/popup') {
    response.end('<!doctype html><meta charset="utf-8"><button id="edge" style="position:fixed;right:0;bottom:0;width:24px;height:24px" onclick="fetch(\'/edge-clicked\',{method:\'POST\',body:JSON.stringify({width:innerWidth,height:innerHeight})})">E</button>');
    return;
  }
  response.end('<!doctype html><meta charset="utf-8"><button id="open" style="position:absolute;left:0;top:0;width:100px;height:100px" onclick="window.open(\'/popup\',\'_blank\')">Open popup</button>');
});
await new Promise(resolve => fixture.listen(0, '127.0.0.1', resolve));
// service runs the current test-support Core with the isolated data root.
// service 使用隔离数据根运行当前测试支持版 Core。
const service = spawn(path.join(repository, 'target/debug/vulcan-code-service.exe'), ['--headless', `--test-root=${root}`], {
  env: { ...process.env, VULCAN_RUNTIME_LAYOUT: path.join(layoutRoot, 'runtime-layout.json') },
  windowsHide: true,
  stdio: ['ignore', 'pipe', 'pipe'],
});
service.stdout.on('data', chunk => serviceOutput.push(chunk.toString()));
service.stderr.on('data', chunk => serviceOutput.push(chunk.toString()));

// until retries one read-only observation within a bounded time.
// until 在限定时间内重试一次只读观察。
// Parameters: observe returns the accepted value; timeout is the maximum wait in milliseconds.
// 参数：observe 返回已认可的值；timeout 是最大等待毫秒数。
// Returns: the accepted value or the last failure.
// 返回值：已认可的值，或最后一次失败。
async function until(observe, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    if (service.exitCode !== null) throw new Error(`Core exited ${service.exitCode}: ${serviceOutput.join('')}`);
    try { return await observe(); }
    catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }
}

try {
  // discovery identifies the exact Core instance started by this test.
  // discovery 标识本测试启动的精确 Core 实例。
  const discovery = await until(async () => {
    const document = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
    const health = await (await fetch(`${document.baseUrl}/health`)).json();
    assert.equal(health.status, 'ready');
    assert.equal(health.pid, service.pid);
    return document;
  }, 120_000);
  // ownerSessionId scopes every Core browser RPC to this synthetic conversation.
  // ownerSessionId 将每条 Core 浏览器 RPC 限定于此合成会话。
  const ownerSessionId = 'w07-viewport-acceptance';
  // rpc sends one service-token-authorized browser operation and validates the result.
  // rpc 发送一项经服务令牌授权的浏览器操作并校验结果。
  // Parameters: method names the browser RPC and params carries its typed fields.
  // 参数：method 指定浏览器 RPC，params 携带其类型化字段。
  // Returns: the Core result; refusal throws the complete response.
  // 返回值：Core 结果；拒绝时抛出完整响应。
  async function rpc(method, params = {}) {
    const response = await fetch(`${discovery.baseUrl}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
      body: JSON.stringify({ method, params: { ...params, ownerSessionId } }),
    });
    const body = await response.json();
    assert.equal(response.status, 200, JSON.stringify(body));
    assert.equal(body.ok, true, `${method}: ${JSON.stringify(body)}`);
    return body.result;
  }
  // capture proves that Core accepts the host's actual size for one page.
  // capture 证明 Core 接受宿主报告的某个页面实际尺寸。
  // Parameters: pageId is the stable Core page identity and expected is the desired viewport.
  // 参数：pageId 是稳定的 Core 页面身份；expected 是目标视口。
  // Returns: the Core capture projection, including current input generations.
  // 返回值：包含当前输入代次的 Core 截图投影。
  async function capture(pageId, expected) {
    const result = await rpc('browser.page.capture', { sessionId: session.sessionId, pageId });
    assert.deepEqual([result.width, result.height], expected);
    assert.equal(result.mimeType, 'image/jpeg');
    return result;
  }
  // sendClick sends a real pointer pair using the latest Core frame generations.
  // sendClick 使用最新 Core 画面代次发送一组真实指针按下/释放事件。
  // Parameters: pageId selects the page, frame supplies generations, and x/y are content pixels.
  // 参数：pageId 选择页面，frame 提供代次，x/y 是内容像素。
  // Returns: no value after both inputs have been confirmed as applied.
  // 返回值：两次输入均确认为已应用后无返回值。
  let inputSequence = 0;
  async function sendClick(pageId, frame, x, y) {
    for (const type of ['down', 'up']) {
      const result = await rpc('browser.input.dispatch', {
        sessionId: session.sessionId, pageId, leaseSecret: lease.leaseSecret,
        pageGeneration: frame.pageGeneration, viewportGeneration: frame.viewportGeneration,
        sequence: String(++inputSequence), kind: 'pointer', payload: { type, x, y, button: 'left' },
      });
      assert.equal(result.outcome, 'applied', JSON.stringify(result));
    }
  }
  // session and lease own the real Chromium process and its exclusive writes.
  // session 与 lease 拥有真实 Chromium 进程及其独占写入权。
  const session = await rpc('browser.session.create', { profileMode: 'ephemeral' });
  const lease = await rpc('browser.lease.acquire', { sessionId: session.sessionId, holder: 'w07-viewport-acceptance' });
  const initialPages = await rpc('browser.pages.list', { sessionId: session.sessionId });
  assert.ok(initialPages.length >= 1);
  const initialPage = initialPages[0];
  await capture(initialPage.pageId, [1280, 800]);
  // resized is acknowledged only after Chromium applies the content viewport.
  // resized 仅在 Chromium 应用内容视口后得到确认。
  const resized = await rpc('browser.viewport.set', {
    sessionId: session.sessionId, leaseSecret: lease.leaseSecret, width: 390, height: 844,
  });
  assert.deepEqual(resized.viewportSize, { width: 390, height: 844 });
  assert.equal(resized.viewportGeneration, '2');
  await capture(initialPage.pageId, [390, 844]);
  // created is a page opened after the resize; it must inherit the current session viewport.
  // created 是视口变更后才打开的页面；它必须继承当前会话视口。
  const createdPages = await rpc('browser.page.create', {
    sessionId: session.sessionId, leaseSecret: lease.leaseSecret,
    url: `http://127.0.0.1:${fixture.address().port}/new`,
  });
  const created = createdPages.find(page => page.pageId !== initialPage.pageId);
  assert.ok(created, 'Core did not expose the newly created page.');
  const createdFrame = await capture(created.pageId, [390, 844]);
  // frameTicket binds a binary observer to the newly created page.
  // frameTicket 将二进制观察端绑定至新页面。
  const frameTicket = await rpc('browser.stream.ticket', { sessionId: session.sessionId });
  const streamUrl = new URL(`${discovery.baseUrl}/v1/browser/stream`);
  streamUrl.protocol = 'ws:';
  streamUrl.searchParams.set('ticket', frameTicket.ticket);
  streamUrl.searchParams.set('pageId', created.pageId);
  streamUrl.searchParams.set('subscriptionId', `w07-${crypto.randomUUID()}`);
  streamUrl.searchParams.set('token', discovery.serviceToken);
  // streamFrame contains the exact dimensions and generation delivered to a WebUI observer.
  // streamFrame 包含交付给 WebUI 观察端的精确尺寸与代次。
  const socket = new WebSocket(streamUrl);
  const streamFrame = await Promise.race([
    new Promise((resolve, reject) => socket.addEventListener('message', async event => {
      if (typeof event.data === 'string') {
        const message = JSON.parse(event.data);
        if (message.kind === 'error') reject(new Error(JSON.stringify(message)));
        return;
      }
      const packet = Buffer.from(event.data instanceof ArrayBuffer ? event.data : await event.data.arrayBuffer());
      const length = packet.readUInt32BE(0);
      const header = JSON.parse(packet.subarray(4, 4 + length).toString('utf8'));
      const image = packet.subarray(4 + length);
      if (header.kind === 'frame' && header.pageId === created.pageId) {
        assert.deepEqual([header.width, header.height], [390, 844]);
        assert.equal(header.viewportGeneration, createdFrame.viewportGeneration);
        assert.ok(image.length > 100);
        assert.deepEqual([...image.subarray(0, 2)], [0xff, 0xd8]);
        resolve(header);
      }
    })),
    new Promise((_, reject) => {
      const timer = setTimeout(() => reject(new Error('Timed out waiting for viewport frame.')), 20_000);
      timer.unref();
    }),
  ]);
  socket.close();
  // A real click opens a popup after the resize, testing the asynchronous registration path.
  // 真实点击在视口变更后打开弹窗，验证异步登记路径。
  await sendClick(created.pageId, createdFrame, 20, 20);
  const popup = await until(async () => {
    const pages = await rpc('browser.pages.list', { sessionId: session.sessionId });
    const page = pages.find(candidate => candidate.url.endsWith('/popup'));
    assert.ok(page, `Popup was not registered: ${JSON.stringify(pages)}`);
    return page;
  });
  const popupFrame = await capture(popup.pageId, [390, 844]);
  // This coordinate lands inside the 390x844 popup but outside a stale 1280x800 viewport.
  // 此坐标位于 390x844 弹窗内部，却超出旧的 1280x800 视口高度。
  await sendClick(popup.pageId, popupFrame, 380, 834);
  await until(() => {
    assert.deepEqual(edgeClicks, [{ width: 390, height: 844 }]);
    return true;
  });
  await rpc('browser.session.close', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  console.log(JSON.stringify({ result: 'passed', sessionId: session.sessionId, viewport: resized.viewportSize,
    projection: [streamFrame.width, streamFrame.height], popup: popup.pageId, edgeClicks }));
} catch (error) {
  console.error(serviceOutput.join(''));
  throw error;
} finally {
  service.kill();
  fixture.close();
  // Both generated roots were created in exact test-owned parents; resolve them before removal.
  // 两个生成根目录均位于精确的测试自有父目录下；删除前校验解析后的实际路径。
  const targetParent = await realpath(path.join(repository, 'target'));
  const tempParent = await realpath(tmpdir());
  const realLayoutRoot = await realpath(layoutRoot);
  const realRoot = await realpath(root);
  assert.ok(realLayoutRoot.startsWith(targetParent + path.sep));
  assert.ok(realRoot.startsWith(tempParent + path.sep));
  await new Promise(resolve => {
    if (service.exitCode !== null) resolve();
    else { service.once('exit', resolve); setTimeout(resolve, 5_000); }
  });
  // Remove explicit directory links first so recursive deletion never visits shared runtime trees.
  // 先移除明确创建的目录链接，使递归删除绝不会遍历共享运行时目录树。
  for (const relative of [
    'desktop/runtime', 'browser/chromium', 'webui', 'dependencies',
    'compiled/node_modules',
  ]) {
    await unlink(path.join(layoutRoot, relative));
  }
  await rm(layoutRoot, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  // Chromium may release its profile lock shortly after the Core has exited.
  // Core 退出后，Chromium 可能需要片刻才释放配置目录锁。
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
}
