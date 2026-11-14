// Exercise the packaged Core, standalone Chromium host, stream, and input against one local page.
// 使用一个本地页面验证打包 Core、独立 Chromium 宿主、投影流和输入。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { createServer } from 'node:http';
import { cp, link, mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, symlink, unlink, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { chromium } from 'playwright-core';
import { DomProjection } from '../dist/chromium-host/dom-projection.js';

// repository identifies the authoritative package root relative to this fixture.
// repository 根据本夹具位置标识权威发行根。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
// assetRoot selects the current isolated distribution without changing shared output files.
// assetRoot 选择当前隔离发行资源包，不修改共享 output 文件。
const assetRoot = process.env.VULCAN_W09_ASSET_ROOT
  ? path.resolve(process.env.VULCAN_W09_ASSET_ROOT) : path.join(repository, 'output');
if (process.env.VULCAN_W09_ASSET_ROOT) {
  const relative = path.relative(path.join(repository, 'target'), assetRoot);
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative),
    'VULCAN_W09_ASSET_ROOT must be inside the repository target directory.');
}
// serviceExecutable is the isolated-test-support build of the same service source.
// serviceExecutable 是同一服务源码编译出的隔离测试支持版本。
const serviceExecutable = path.join(repository, 'target/debug/vulcan-code-service.exe');
// root isolates the service and browser profile from actual user data.
// root 将服务和浏览器画像与真实用户数据隔离。
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-w09-browser-'));
// layoutRoot binds the debug service to the fresh host build and selected Chromium package.
// layoutRoot 将 debug 服务绑定到新宿主构建和选定的 Chromium 资源包。
const layoutRoot = await mkdtemp(path.join(repository, 'target', 'w09-core-browser-layout-'));
await Promise.all([
  mkdir(path.join(layoutRoot, 'bin'), { recursive: true }),
  mkdir(path.join(layoutRoot, 'browser'), { recursive: true }),
  mkdir(path.join(layoutRoot, 'desktop'), { recursive: true }),
]);
await cp(path.join(repository, 'apps/vulcan-code-desktop/dist'), path.join(layoutRoot, 'compiled'), { recursive: true });
await Promise.all([
  link(path.join(repository, 'target/debug/vulcan-code-service.exe'), path.join(layoutRoot, 'bin/vulcan-code-service.exe')),
  symlink(path.join(assetRoot, 'desktop/runtime'), path.join(layoutRoot, 'desktop/runtime'), 'junction'),
  symlink(path.join(assetRoot, 'browser/chromium'), path.join(layoutRoot, 'browser/chromium'), 'junction'),
  symlink(path.join(assetRoot, 'webui'), path.join(layoutRoot, 'webui'), 'junction'),
  symlink(path.join(assetRoot, 'dependencies'), path.join(layoutRoot, 'dependencies'), 'junction'),
  symlink(await realpath(path.join(repository, 'apps/vulcan-code-desktop/node_modules')),
    path.join(layoutRoot, 'compiled/node_modules'), 'junction'),
]);
// runtimeLayout records a debug host entry rather than reusing a stale release host path.
// runtimeLayout 记录 debug 宿主入口，不复用陈旧的 release 宿主路径。
const runtimeLayout = JSON.parse(await readFile(path.join(assetRoot, 'runtime-layout.json'), 'utf8'));
runtimeLayout.profile = 'debug';
runtimeLayout.browserHostEntry = 'compiled/chromium-host/main.js';
await writeFile(path.join(layoutRoot, 'runtime-layout.json'), JSON.stringify(runtimeLayout, null, 2));
// diagnostics retains service output if an assertion fails.
// diagnostics 在断言失败时保留服务输出。
const diagnostics = [];
// Every browser RPC is scoped to this exact VulcanCode conversation owner.
// 每个浏览器 RPC 都限定在这个精确的 VulcanCode 会话所有者内。
const ownerSessionId = 'conversation-w09';
// A real bundled font verifies that Chromium and the WebUI use the same bytes.
// 使用真实随包字体验证 Chromium 与 WebUI 使用相同字节。
const fontAssets = path.join(assetRoot, 'webui/assets');
const fixtureFontName = (await readdir(fontAssets)).find(name => /^KaTeX_Main-Regular-.*\.woff2$/.test(name));
assert.ok(fixtureFontName);
const fixtureFont = await readFile(path.join(fontAssets, fixtureFontName));
// clicks counts DOM-dispatched clicks received by the fixture server.
// clicks 统计夹具服务器收到的页面 DOM 点击。
let clicks = 0;
// latencyClickTimes records when the fixture server observed each numbered page click.
// latencyClickTimes 记录夹具服务器观察到每次编号页面点击的时间。
const latencyClickTimes = [];
// uploaded records the filename and content observed by the real page after Core attachment.
// uploaded 记录 Core 附加文件后真实页面观察到的文件名与内容。
let uploaded = null;
// dialogAnswer records the real page's result after the controller answers its prompt.
// dialogAnswer 记录控制者答复提示框后真实页面获得的结果。
let dialogAnswer = null;
// dialogResultCount reveals whether Chromium silently dismissed the prompt.
// dialogResultCount 用于判断 Chromium 是否静默拒绝了提示框。
let dialogResultCount = 0;
// dialogClicks distinguishes a missed pointer from a missed CDP dialog event.
// dialogClicks 区分未命中指针与未捕获 CDP 对话框事件。
let dialogClicks = 0;
// fixture serves a deterministic local page and records actual page click callbacks.
// fixture 提供确定性的本地页面并记录真实页面点击回调。
const fixture = createServer((request, response) => {
  if (request.url === '/projection.svg') {
    response.writeHead(200, { 'content-type': 'image/svg+xml', 'cache-control': 'no-store' });
    response.end('<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect width="40" height="40" fill="blue"/></svg>');
    return;
  }
  if (request.url === '/projection.css') {
    response.writeHead(200, { 'content-type': 'text/css' });
    response.end('@import "/projection-nested.css"; #projected { width:40px; height:40px; }');
    return;
  }
  if (request.url === '/projection.woff2') {
    response.writeHead(200, { 'content-type': 'font/woff2' }).end(fixtureFont);
    return;
  }
  if (request.url === '/projection-nested.css') {
    response.writeHead(200, { 'content-type': 'text/css' });
    response.end('@font-face { font-family: ProjectionFont; src:url("/projection.woff2"); } #projected { background-image:url("/projection.svg"); font-family:ProjectionFont; color:rgb(13,42,96); }');
    return;
  }
  if (request.url === '/projection') {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end(`<!doctype html><link rel="stylesheet" href="/projection.css">
      <input id="edit" style="position:fixed;left:0;top:0;width:200px;height:60px">
      <div id="projected" style="position:fixed;left:100px;top:130px;touch-action:none">Drag</div><img id="projected-image" src="/projection.svg">
      <a id="projected-download" href="/projection-download" style="position:fixed;left:500px;top:100px">Download projection fixture</a>
      <img id="projected-blob">
      <a id="popup" style="position:fixed;left:250px;top:0;width:180px;height:60px" target="_blank" rel="noopener" href="/popup">Open popup</a>
      <div style="position:fixed;left:90px;top:215px">
        <projection-widget>Slotted label</projection-widget>
        <div id="hover-target" style="height:55px">Hover target <span id="hover-menu">Hover menu</span></div>
        <input type="email" id="projection-email" autocomplete="off">
        <input type="password" id="projection-password" autocomplete="off">
        <output id="password-length">0</output>
        <span id="cascade-order">Cascade order</span>
      </div>
      <style>#cascade-order { color:rgb(91,23,54) }</style>
      <script>
        document.querySelector('#edit').addEventListener('input', e => document.body.dataset.edited = e.target.value);
        document.querySelector('#projected-blob').src = URL.createObjectURL(new Blob(
          ['<svg xmlns="http://www.w3.org/2000/svg" width="31" height="31"><circle cx="15" cy="15" r="14"/></svg>'],
          { type: 'image/svg+xml' }
        ));
        const layout = new CSSStyleSheet();
        layout.replaceSync('#hover-menu { visibility:hidden } #hover-target:hover #hover-menu { visibility:visible; color:rgb(23,54,91) }'
          + '#cascade-order { color:rgb(23,54,91) }');
        document.adoptedStyleSheets = [layout];
        const host = document.querySelector('projection-widget');
        const root = host.attachShadow({ mode:'open' });
        root.innerHTML = '<slot></slot><projection-inner></projection-inner>';
        const inner = root.querySelector('projection-inner').attachShadow({ mode:'open' });
        inner.innerHTML = '<button id="shadow-button">Shadow action</button><span id="shadow-state">idle</span>'
          + '<div id="projection-rich" contenteditable="true" style="width:220px;height:32px">rich start</div>';
        const sheet = new CSSStyleSheet();
        sheet.replaceSync('#shadow-button { color:rgb(16,85,142); background-image:url("/projection.svg") }'
          + '#shadow-button:hover { color:rgb(35,110,81) }');
        inner.adoptedStyleSheets = [sheet];
        inner.querySelector('button').onclick = () => inner.querySelector('#shadow-state').textContent = 'clicked';
        document.querySelector('#projection-password').addEventListener('input', event => {
          document.querySelector('#password-length').textContent = event.target.value.length;
        });
        let dragging = false;
        const item = document.querySelector('#projected');
        item.addEventListener('pointerdown', () => dragging = true);
        document.addEventListener('pointermove', event => {
          if (dragging && event.buttons === 1) item.style.transform = 'rotate(15deg)';
        });
        document.addEventListener('pointerup', () => dragging = false);
      </script>`);
    return;
  }
  if (request.url === '/projection-download') {
    response.writeHead(200, { 'content-type': 'application/octet-stream',
      'content-disposition': 'attachment; filename="projection-fixture.txt"' }).end('projection fixture');
    return;
  }
  if (request.url === '/popup') {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end('<title>Projection popup</title><link rel="stylesheet" href="/popup-styles"><main>Popup reached</main>');
    return;
  }
  if (request.url === '/popup-styles') {
    response.writeHead(302, { location: '/popup-assets/popup.css' }).end();
    return;
  }
  if (request.url === '/popup-assets/popup.css') {
    response.writeHead(200, { 'content-type': 'text/css' })
      .end('main { color:rgb(23,54,91); background-image:url("popup.svg") }');
    return;
  }
  if (request.url === '/popup-assets/popup.svg') {
    response.writeHead(200, { 'content-type': 'image/svg+xml' })
      .end('<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"><rect width="18" height="18" fill="green"/></svg>');
    return;
  }
  if (request.url === '/clicked' && request.method === 'POST') {
    clicks += 1;
    response.writeHead(204).end();
    return;
  }
  // A separate callback distinguishes delayed image delivery from a pointer that never reached DOM.
  // 单独的回调区分画面延迟与从未到达 DOM 的指针输入。
  const latencyClick = request.url?.match(/^\/latency-clicked\?count=([0-9]+)$/);
  if (latencyClick && request.method === 'POST') {
    latencyClickTimes[Number(latencyClick[1]) - 1] = Date.now();
    response.writeHead(204).end();
    return;
  }
  if (request.url === '/uploaded' && request.method === 'POST') {
    // body accumulates only the small deterministic fixture payload.
    // body 仅累积很小且确定性的夹具载荷。
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk) => { body += chunk; });
    request.on('end', () => { uploaded = JSON.parse(body); response.writeHead(204).end(); });
    return;
  }
  if (request.url === '/download') {
    response.writeHead(200, {
      'content-type': 'application/octet-stream',
      'content-disposition': 'attachment; filename="fixture-download.txt"',
    });
    response.end('core-download-content');
    return;
  }
  if (request.url === '/dialog-result' && request.method === 'POST') {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk) => { body += chunk; });
    request.on('end', () => { dialogAnswer = JSON.parse(body); dialogResultCount += 1; response.writeHead(204).end(); });
    return;
  }
  if (request.url === '/dialog-clicked' && request.method === 'POST') {
    dialogClicks += 1;
    response.writeHead(204).end();
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  response.end('<!doctype html><meta charset="utf-8"><button style="position:absolute;left:0;top:0;width:220px;height:150px" onclick="fetch(\'/clicked\',{method:\'POST\'})">Core browser test</button><button id="latency" style="position:absolute;left:500px;top:0;width:200px;height:120px" onclick="this.textContent=String(Number(this.textContent)+1);fetch(\'/latency-clicked?count=\'+this.textContent,{method:\'POST\'})">0</button><input id="upload" type="file" style="position:absolute;top:180px"><a href="/download" style="position:absolute;left:0;top:300px;width:220px;height:60px">Download fixture</a><button style="position:absolute;left:0;top:380px;width:220px;height:60px" onclick="fetch(\'/dialog-clicked\',{method:\'POST\'});alert(\'Core dialog\');fetch(\'/dialog-result\',{method:\'POST\',headers:{\'content-type\':\'application/json\'},body:JSON.stringify({answer:\'accepted\'})})">Alert fixture</button><button style="position:absolute;left:0;top:460px;width:220px;height:60px" onclick="const answer=confirm(\'Core confirm\');fetch(\'/dialog-result\',{method:\'POST\',headers:{\'content-type\':\'application/json\'},body:JSON.stringify({answer})})">Confirm fixture</button><button style="position:absolute;left:0;top:540px;width:220px;height:60px" onclick="const answer=prompt(\'Core prompt\',\'initial\');fetch(\'/dialog-result\',{method:\'POST\',headers:{\'content-type\':\'application/json\'},body:JSON.stringify({answer})})">Prompt fixture</button><script>document.querySelector("#upload").addEventListener("change", async event => { const file = event.target.files[0]; await fetch("/uploaded", {method:"POST",body:JSON.stringify({name:file.name,text:await file.text()})}); });</script>');
});
await new Promise((resolve) => fixture.listen(0, '127.0.0.1', resolve));
// pageUrl is the exact fixture origin loaded by the Core-owned host.
// pageUrl 是 Core 所属宿主加载的精确夹具来源。
const pageUrl = `http://127.0.0.1:${fixture.address().port}/`;
// service is the service process launched with an isolated data root and packaged runtime layout.
// service 是使用隔离数据根和打包运行时布局启动的服务进程。
const service = spawn(serviceExecutable, ['--headless', `--test-root=${root}`], {
  env: { ...process.env, VULCAN_RUNTIME_LAYOUT: path.join(layoutRoot, 'runtime-layout.json'),
    VULCAN_CODE_OUTPUT_ROOT: assetRoot },
  windowsHide: true,
  stdio: ['ignore', 'pipe', 'pipe'],
});
service.stdout.on('data', (chunk) => diagnostics.push(chunk.toString()));
service.stderr.on('data', (chunk) => diagnostics.push(chunk.toString()));

// until retries a read-only condition until it returns an accepted value.
// until 重试只读条件，直到它返回已认可值。
// observe produces the accepted value; timeout limits the total wait.
// observe 产生已认可值；timeout 限制总等待时间。
async function until(observe, timeout = 60_000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    if (service.exitCode !== null) {
      throw new Error(`The service exited before readiness: ${service.exitCode}\n${diagnostics.join('')}`);
    }
    try { return await observe(); } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
}

// Mount the real BrowserArea in two regular Chromium clients over the authenticated Core transport.
// 通过已鉴权的 Core 传输，在两个普通 Chromium 客户端中挂载实际 BrowserArea。
async function verifyDomViewers(discovery, sessionId) {
  const webRoot = path.join(repository, 'apps/vulcan-code-web');
  const webRequire = createRequire(path.join(webRoot, 'package.json'));
  const { createServer: createVite } = await import(pathToFileURL(webRequire.resolve('vite')).href);
  const viewerPath = '/projection-acceptance.html';
  const moduleId = 'virtual:projection-acceptance';
  const vite = await createVite({
    root: webRoot,
    server: { host: '127.0.0.1', strictPort: false, hmr: false },
    cacheDir: path.join(layoutRoot, 'vite-cache'),
    plugins: [{
      name: 'projection-acceptance',
      resolveId: id => id === moduleId ? '\0' + moduleId : null,
      load: id => id === '\0' + moduleId ? `
        import React from 'react';
        import { createRoot } from 'react-dom/client';
        import { I18nProvider } from '/src/i18n.tsx';
        import { BrowserArea } from '/src/components/features/browser/browser-area.tsx';
        createRoot(document.getElementById('mount')).render(React.createElement(
          I18nProvider, { initialLocale: 'en-US' }, React.createElement(
            BrowserArea, { ownerSessionId: ${JSON.stringify(ownerSessionId)} }
          )));
      ` : null,
      configureServer(server) {
        server.middlewares.use(async (request, response, next) => {
          // A loopback forwarding endpoint presents a same-origin Core connection to the test clients.
          // 本地回环转发端点为测试客户端提供同源的 Core 连接。
          const route = request.url?.split('?')[0];
          if (route === '/health' || route === '/rpc') {
            try {
              const chunks = [];
              if (route === '/rpc') for await (const chunk of request) chunks.push(chunk);
              const core = await fetch(`${discovery.baseUrl}${route}`, {
                method: request.method,
                headers: { 'content-type': 'application/json',
                  'x-vulcan-service-token': request.headers['x-vulcan-service-token'] ?? '' },
                body: route === '/rpc' ? Buffer.concat(chunks) : undefined,
                signal: AbortSignal.timeout(5_000),
              });
              const body = await core.json();
              if (route === '/health') body.baseUrl = `http://${request.headers.host}`;
              response.writeHead(core.status, { 'content-type': 'application/json', 'cache-control': 'no-store' });
              response.end(JSON.stringify(body));
            } catch (error) { next(error); }
            return;
          }
          if (request.url?.split('?')[0] !== viewerPath) return next();
          const html = await server.transformIndexHtml(viewerPath, `<!doctype html>
            <html><head>
            <style>body { margin: 0 } #mount { display:flex; height: 100vh; }</style>
            </head><body><div id="mount"></div><script type="module" src="/@id/${moduleId}"></script></body></html>`);
          response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }).end(html);
        });
      },
    }],
  });
  const browser = await chromium.launch({ executablePath: path.join(assetRoot, runtimeLayout.chromiumExecutable),
    channel: 'chromium', headless: true });
  try {
    // Attach after load to force the popup resource-tree path regardless of machine timing.
    // 在加载完成后附加，无论机器时序如何都覆盖弹窗资源树补采路径。
    const bootstrapContext = await browser.newContext();
    try {
      const projection = new DomProjection();
      projection.observeContext(bootstrapContext);
      const popup = await bootstrapContext.newPage();
      await popup.goto(new URL('/popup', pageUrl).href, { waitUntil: 'load' });
      await projection.trackPage(popup, await bootstrapContext.newCDPSession(popup));
      await until(async () => {
        const snapshot = await projection.snapshot(popup, null);
        assert.ok(snapshot?.assets);
        const assets = snapshot.assets.map(id => ({ id, chunk: projection.asset(popup, id, 0) }));
        const image = assets.find(asset => asset.chunk?.mimeType === 'image/svg+xml');
        const style = assets.find(asset => asset.chunk?.mimeType === 'text/css');
        assert.ok(image && style);
        assert.deepEqual(style.chunk.dependencies, [image.id]);
        return true;
      }, 5_000);
    } finally {
      await bootstrapContext.close();
    }
    await vite.listen(18_723);
    const origin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    const url = new URL(viewerPath, origin);
    url.searchParams.set('service', origin);
    // Count viewport writes from both clients to detect duplicate or automatic fits.
    // 统计双客户端的视口写入，检测重复适配或自动适配。
    let viewportWrites = 0;
    let originRequests = 0;
    const clients = [];
    for (let index = 0; index < 2; index++) {
      // Separate contexts isolate both clients' caches, storage and browser input.
      // 使用独立上下文隔离两个客户端的缓存、存储和浏览器输入。
      const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
      context.setDefaultTimeout(5_000);
      context.on('request', request => {
        if (request.method() === 'POST' && new URL(request.url()).pathname === '/rpc'
          && request.postDataJSON().method === 'browser.viewport.set') viewportWrites++;
      });
      await context.route(`${pageUrl}**`, route => { originRequests++; return route.abort(); });
      const client = await context.newPage();
      const errors = [];
      client.on('pageerror', error => errors.push(error.message));
      client.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      await client.goto(url.href, { waitUntil: 'domcontentloaded', timeout: 25_000 });
      const browserTab = client.locator(`[data-browser-session-id="${sessionId}"]`);
      await browserTab.waitFor({ state: 'attached', timeout: 25_000 }).catch(async error => {
        throw new Error(`Browser DOM acceptance viewer did not connect: ${JSON.stringify({
          errors, text: await client.locator('body').innerText(),
        })}`, { cause: error });
      });
      if (await browserTab.getAttribute('aria-selected') !== 'true' && await browserTab.isVisible()) {
        await browserTab.click({ timeout: 5_000 });
      }
      clients.push(client);
    }
    const [controller, observer] = clients;
    await controller.bringToFront();
    const frame = client => client.frameLocator('.vk-browser-dom-surface iframe');
    for (const client of clients) {
      const image = frame(client).locator('#projected-image');
      await image.waitFor({ state: 'attached', timeout: 25_000 });
      await until(async () => {
        assert.equal(await image.evaluate(element => element.naturalWidth), 40);
        assert.match(await image.getAttribute('src'), /^blob:/);
        assert.equal(await frame(client).locator('#projected-blob').evaluate(element => element.naturalWidth), 31);
        const appearance = await frame(client).locator('#projected').evaluate(async element => ({
          color: element.ownerDocument.defaultView.getComputedStyle(element).color,
          fonts: (await element.ownerDocument.fonts.load('16px ProjectionFont')).length,
        }));
        assert.deepEqual(appearance, { color: 'rgb(13, 42, 96)', fonts: 1 });
        return true;
      }, 15_000);
    }
    assert.equal(originRequests, 0, 'The WebUI must consume assets without requesting the source origin.');
    console.log('DOM viewers loaded the same image, imported style, and font without source requests.');
    await controller.getByRole('button', { name: 'Take control' }).click({ timeout: 5_000 });
    await controller.getByRole('button', { name: 'Release control' }).waitFor({ timeout: 5_000 });
    await frame(controller).locator('#projected-download').click();
    await controller.getByRole('button', { name: 'Download list' }).click();
    await controller.getByRole('button', { name: /projection-fixture\.txt/ }).waitFor();
    await controller.getByRole('button', { name: 'Download list' }).click();
    // Reload the passive client to request a fresh snapshot after the attachment.
    // 附件处理后重新加载观察端，请求新快照。
    await observer.reload({ waitUntil: 'domcontentloaded' });
    for (const client of clients) {
      await until(async () => {
        assert.equal(await frame(client).locator('#projected-image').evaluate(element => element.naturalWidth), 40);
        assert.equal(await frame(client).locator('#projected').evaluate(element => getComputedStyle(element).color), 'rgb(13, 42, 96)');
        return true;
      }, 15_000);
    }
    for (const client of clients) {
      const button = frame(client).getByRole('button', { name: 'Shadow action' });
      assert.equal(await button.evaluate(element => getComputedStyle(element).color), 'rgb(16, 85, 142)');
      assert.match(await button.evaluate(element => getComputedStyle(element).backgroundImage), /^url\("blob:/);
      assert.equal(await frame(client).locator('slot').evaluate(slot => slot.assignedNodes()[0].textContent), 'Slotted label');
      assert.equal(await frame(client).locator('#cascade-order').evaluate(element => getComputedStyle(element).color), 'rgb(23, 54, 91)');
    }
    await frame(controller).locator('#hover-target').hover();
    for (const client of clients) await frame(client).locator('#hover-menu').waitFor({ state: 'visible' });
    await controller.getByRole('button', { name: 'Resize viewport' }).hover();
    for (const client of clients) await frame(client).locator('#hover-menu').waitFor({ state: 'hidden' });
    await frame(controller).getByRole('button', { name: 'Shadow action' }).hover();
    for (const client of clients) {
      await frame(client).locator('#hover-menu').waitFor({ state: 'hidden' });
      await until(async () => {
        assert.equal(await frame(client).locator('#shadow-button').evaluate(element => getComputedStyle(element).color), 'rgb(35, 110, 81)');
        return true;
      }, 10_000);
    }
    await frame(controller).getByRole('button', { name: 'Shadow action' }).click();
    for (const client of clients) await frame(client).getByText('clicked', { exact: true }).waitFor();
    console.log('Both DOM viewers rendered nested shadow, scoped assets, slots, and authoritative hover.');
    // Drive Chromium's composition protocol while observing the other authenticated WebUI client.
    // 通过 Chromium 合成输入协议录入文字，同时观察另一个已鉴权的 WebUI 客户端。
    const cdp = await controller.context().newCDPSession(controller);
    // Observe routed edits so an unchanged composition cannot pass by checking before a delayed replay.
    // 观察已路由的编辑，避免未变化的输入在延迟回放前检查而误报通过。
    let projectedEdits = 0;
    controller.on('request', request => {
      if (request.method() !== 'POST' || new URL(request.url()).pathname !== '/rpc') return;
      const { method, params } = request.postDataJSON();
      if (method === 'browser.input.dispatch' && ['textEdit', 'textCommit'].includes(params.kind)) projectedEdits++;
    });
    for (const [selector, text, replace] of [
      ['#edit', '输入法 vulcancode', false],
      ['#edit', '替换文本', true],
      ['#projection-email', 'ime-test@example.test', false],
      ['#projection-rich', '富文本', true],
      ['#projection-password', '测试密码', false],
      ['#projection-password', '替代', true],
    ]) {
      await frame(controller).locator(selector).click();
      if (replace) await controller.keyboard.press('Control+a');
      await cdp.send('Input.imeSetComposition', { text: '候选', selectionStart: 2, selectionEnd: 2 });
      await cdp.send('Input.insertText', { text });
      await until(async () => {
        const observed = frame(observer).locator(selector);
        if (selector === '#projection-rich') assert.equal(await observed.textContent(), text);
        else if (selector === '#projection-password') {
          assert.equal(await observed.getAttribute('value'), null);
          assert.equal(await observed.inputValue(), '');
          assert.equal(await frame(observer).locator('#password-length').textContent(), String(text.length));
        } else assert.equal(await observed.getAttribute('value'), text);
        return true;
      }, 10_000);
    }
    // Repeat the same text over a local selection with both insertion and composition.
    // 分别通过普通插入和输入法组合，以相同文本替换本地选区。
    const edit = frame(controller).locator('#edit');
    await edit.click();
    const priorEdits = projectedEdits;
    await controller.keyboard.press('Control+a');
    await cdp.send('Input.insertText', { text: '替换文本' });
    await controller.keyboard.press('Control+a');
    await cdp.send('Input.imeSetComposition', { text: '候选', selectionStart: 2, selectionEnd: 2 });
    await cdp.send('Input.insertText', { text: '替换文本' });
    // Wait for a later real edit to reach the observer before comparing queue traffic.
    // 等待后续真实编辑到达观察端，再比较队列中的编辑请求。
    await controller.keyboard.press('End');
    await cdp.send('Input.insertText', { text: '✓' });
    await until(async () => {
      assert.equal(await frame(observer).locator('#edit').getAttribute('value'), '替换文本✓');
      return true;
    }, 10_000);
    assert.equal(projectedEdits, priorEdits + 1);
    // Check both native deletion directions using lengths only across the password boundary.
    // 仅比较长度，验证密码框两种原生删除方向在两端一致。
    const password = frame(controller).locator('#projection-password');
    await password.click();
    await password.press('End');
    await password.press('Backspace');
    await until(async () => {
      assert.equal(await frame(observer).locator('#password-length').textContent(), '1');
      assert.equal(await password.evaluate(element => element.value.length), 1);
      return true;
    }, 10_000);
    await password.press('Home');
    await password.press('Delete');
    await until(async () => {
      assert.equal(await frame(observer).locator('#password-length').textContent(), '0');
      assert.equal(await password.evaluate(element => element.value.length), 0);
      assert.equal(await frame(observer).locator('#projection-password').inputValue(), '');
      return true;
    }, 10_000);
    await cdp.detach();
    assert.equal(originRequests, 0, 'Neither client may fetch source resources, including inside Shadow DOM.');
    console.log('Both viewers verified committed IME insertion and replacement, including password redaction.');
    const box = await frame(controller).locator('#projected').boundingBox({ timeout: 5_000 });
    assert.ok(box);
    await controller.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await controller.mouse.down();
    await controller.mouse.move(box.x + box.width + 20, box.y + box.height / 2);
    try {
      for (const [index, client] of clients.entries()) await until(async () => {
        const style = await frame(client).locator('#projected').getAttribute('style', { timeout: 1_000 });
        assert.match(style, /rotate\(15deg\)/, `DOM viewer ${index} did not render the source drag while held: ${style}`);
        return true;
      }, 10_000);
      console.log('The controller and observer both rendered source rotation before pointer release.');
    } finally {
      await controller.mouse.up();
    }
    // Explicitly refocus the controller before activating the link in another browser tab.
    // 在另一个浏览器标签页激活链接前，先显式聚焦控制端。
    await controller.bringToFront();
    await frame(controller).locator('#popup').click({ timeout: 5_000 });
    for (const client of clients) {
      await until(async () => {
        const popup = frame(client).getByText('Popup reached');
        const view = await popup.evaluate(element => {
          const style = getComputedStyle(element);
          return { color: style.color, image: style.backgroundImage };
        });
        assert.equal(view.color, 'rgb(23, 54, 91)');
        assert.match(view.image, /^url\("blob:/);
        return true;
      }, 15_000);
    }
    assert.equal(originRequests, 0, 'Neither popup client may fetch the stylesheet or its relative assets.');
    // The active viewport is a shared Core value confirmed by both rendered DOM surfaces.
    // 当前视口是共享的 Core 值，由两个 DOM 投影界面共同确认。
    async function expectViewport(expected) {
      await until(async () => {
        for (const client of clients) {
          const viewport = await frame(client).locator('body').evaluate(body => ({
            width: body.ownerDocument.defaultView.innerWidth,
            height: body.ownerDocument.defaultView.innerHeight,
          }));
          assert.deepEqual(viewport, expected);
        }
        return true;
      }, 10_000);
    }
    // Read the same drawable CSS-pixel area used by the toolbar command.
    // 读取工具栏命令使用的同一块可绘制 CSS 像素区域。
    const panelSize = client => client.locator('[data-browser-viewport]').evaluate(panel => ({
      width: panel.clientWidth, height: panel.clientHeight,
    }));
    const viewportMenu = controller.getByRole('button', { name: 'Resize viewport' });
    // Hold one snapshot response after Core confirms the resize to exercise the original race.
    // Core 确认调整后阻塞一个快照应答，覆盖原先的竞态。
    let resumeSnapshots;
    const snapshotsPaused = new Promise(resolve => { resumeSnapshots = resolve; });
    await controller.route('**/rpc', async route => {
      if (viewportWrites === 1 && route.request().postDataJSON().method === 'browser.dom.snapshot') await snapshotsPaused;
      await route.continue();
    });
    await viewportMenu.click();
    await controller.getByRole('menuitemradio', { name: /Google Pixel 7.*915×412/ }).click();
    try {
      await until(async () => {
        assert.equal(await frame(observer).locator('body').evaluate(body => body.ownerDocument.defaultView.innerWidth), 915);
        return true;
      }, 10_000);
      await viewportMenu.click();
      assert.equal(await controller.getByRole('menuitemradio', { name: /Google Pixel 7.*915×412/ }).getAttribute('aria-disabled'), 'true');
      await controller.keyboard.press('Escape');
    } finally {
      resumeSnapshots();
    }
    await expectViewport({ width: 915, height: 412 });
    await viewportMenu.click();
    await controller.getByRole('menuitem', { name: 'Custom size…' }).click();
    const custom = controller.getByRole('dialog');
    await custom.getByRole('spinbutton', { name: 'Viewport width' }).fill('710');
    await custom.getByRole('spinbutton', { name: 'Viewport height' }).fill('510');
    await custom.getByRole('button', { name: 'Apply size' }).click();
    await expectViewport({ width: 710, height: 510 });
    const firstFit = await panelSize(controller);
    await viewportMenu.click();
    await controller.getByRole('menuitem', { name: /Fit to current window/ }).click();
    await expectViewport(firstFit);
    assert.equal(viewportWrites, 3);
    await controller.setViewportSize({ width: 1040, height: 710 });
    await observer.setViewportSize({ width: 870, height: 650 });
    assert.notDeepEqual(await panelSize(controller), firstFit);
    assert.notDeepEqual(await panelSize(observer), firstFit);
    await expectViewport(firstFit);
    assert.equal(viewportWrites, 3);
    await controller.getByRole('button', { name: 'Release control' }).click({ timeout: 5_000 });
    await controller.getByRole('button', { name: 'Take control' }).waitFor({ timeout: 5_000 });
    await observer.bringToFront();
    await observer.getByRole('button', { name: 'Take control' }).click();
    const secondFit = await panelSize(observer);
    await observer.getByRole('button', { name: 'Resize viewport' }).click();
    await observer.getByRole('menuitem', { name: /Fit to current window/ }).click();
    await expectViewport(secondFit);
    assert.equal(viewportWrites, 4);
    await observer.getByRole('button', { name: 'Release control' }).click();
    await observer.getByRole('button', { name: 'Take control' }).waitFor();
    console.log('Viewport presets, custom dimensions, and one-shot fitting passed in two clients.');
    console.log('Both DOM viewers followed the new Playwright tab.');
  } finally {
    await browser.close().catch(() => {});
    vite.httpServer?.closeAllConnections();
    await vite.close();
  }
}

try {
  // discovery binds every request to the isolated service's published identity.
  // discovery 将所有请求绑定到隔离服务公布的身份。
  const discovery = await until(async () => {
    const value = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
    const health = await (await fetch(`${value.baseUrl}/health`)).json();
    assert.equal(health.status, 'ready');
    assert.equal(health.pid, service.pid);
    return value;
  }, 120_000);
  // rpc sends one authorized request and returns only the confirmed Core result.
  // rpc 发送一条已授权请求，并仅返回 Core 确认的结果。
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
  // session owns the real background browser, independent of a GUI window.
  // session 拥有真实后台浏览器，独立于任何 GUI 窗口。
  const session = await rpc('browser.session.create', { profileMode: 'ephemeral' });
  // lease authorizes this fixture's host mutations and input.
  // lease 授权本夹具的宿主修改与输入。
  const lease = await rpc('browser.lease.acquire', { sessionId: session.sessionId, holder: 'w09-acceptance' });
  // pages exposes stable Core identities rather than client-controlled target IDs.
  // pages 暴露稳定的 Core 身份，而不是客户端控制的 target ID。
  const pages = await rpc('browser.pages.list', { sessionId: session.sessionId });
  assert.ok(pages.length >= 1, 'The host must register its first real page.');
  // page identifies the exact page observed and driven below.
  // page 标识下方观察和驱动的精确页面。
  const page = pages[0];
  // Verify the production DOM RPC and capture resources before running the legacy stream checks.
  // 先验证正式 DOM RPC 与资源捕获，再运行旧画面流检查。
  await rpc('browser.session.navigate', {
    sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
    url: new URL('/projection', pageUrl).href,
  });
  const projectionParams = { sessionId: session.sessionId, pageId: page.pageId };
  let projected;
  for (let attempt = 0; attempt < 30; attempt++) {
    projected = await rpc('browser.dom.snapshot', projectionParams);
    if (projected.result?.assets?.length === 5) break;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  assert.equal(projected.result?.assets?.length, 5);
  assert.doesNotMatch(projected.result.html, /<script|href="https?:|src="https?:/);
  const unchanged = await rpc('browser.dom.snapshot', { ...projectionParams, revision: projected.result.revision });
  assert.deepEqual(unchanged.result, { revision: projected.result.revision });
  const assets = [];
  for (const assetId of projected.result.assets) {
    const { result } = await rpc('browser.dom.asset', { ...projectionParams, assetId, offset: 0 });
    assets.push({ ...result, text: Buffer.from(result.dataBase64, 'base64').toString('utf8') });
  }
  assert.equal(assets.filter(asset => asset.mimeType === 'text/css').length, 2);
  assert.ok(assets.some(asset => asset.mimeType === 'image/svg+xml' && asset.text.includes('fill="blue"')));
  assert.ok(assets.some(asset => asset.mimeType === 'font/woff2'
    && Buffer.from(asset.dataBase64, 'base64').equals(fixtureFont)));
  assert.deepEqual(assets.filter(asset => asset.mimeType === 'text/css').map(asset => asset.dependencies.length).sort(), [1, 2]);
  let projectionSequence = 0;
  // The DOM controller sends exact generations and replays text against the focused projected node.
  // DOM 控制器发送确切代次，并针对已聚焦的投影节点回放文字。
  async function projectionInput(kind, payload) {
    const reply = await rpc('browser.input.dispatch', {
      ...projectionParams, leaseSecret: lease.leaseSecret,
      pageGeneration: projected.pageGeneration, viewportGeneration: projected.viewportGeneration,
      sequence: String(++projectionSequence), kind, payload,
    });
    assert.equal(reply.outcome, 'applied', JSON.stringify(reply));
  }
  await projectionInput('pointer', { type: 'down', x: 30, y: 20, button: 'left' });
  await projectionInput('pointer', { type: 'up', x: 30, y: 20, button: 'left' });
  projected = await rpc('browser.dom.snapshot', projectionParams);
  assert.ok(projected.result.focused);
  await projectionInput('textEdit', {
    id: projected.result.focused, documentId: projected.result.documentId,
    before: '', start: 0, end: 0, text: '输入法 vulcancode',
  });
  projected = await rpc('browser.dom.snapshot', projectionParams);
  assert.match(projected.result.html, /data-edited="输入法 vulcancode"/);
  await projectionInput('pointer', { type: 'down', x: 280, y: 20, button: 'left' });
  await projectionInput('pointer', { type: 'up', x: 280, y: 20, button: 'left' });
  let popup;
  for (let attempt = 0; attempt < 30; attempt++) {
    const currentPages = await rpc('browser.pages.list', { sessionId: session.sessionId });
    popup = currentPages.find(candidate => candidate.url === new URL('/popup', pageUrl).href);
    if (popup) break;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  assert.ok(popup, 'Playwright should register a popup even when it has no opener.');
  const popupProjection = await rpc('browser.dom.snapshot', { ...projectionParams, pageId: popup.pageId });
  assert.match(popupProjection.result.html, /Popup reached/);
  await rpc('browser.page.close', { ...projectionParams, pageId: popup.pageId, leaseSecret: lease.leaseSecret });
  await rpc('browser.session.navigate', {
    ...projectionParams, leaseSecret: lease.leaseSecret, url: new URL('/projection', pageUrl).href,
  });
  // Acquire a fresh sender sequence before exercising the independent legacy stream.
  // 在独立验证旧画面流之前获取新的发送序列。
  await rpc('browser.lease.release', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  await verifyDomViewers(discovery, session.sessionId);
  // Retire the popup created by the WebUI before running the remaining independent cases.
  // 运行其余独立用例之前关闭 WebUI 创建的弹窗。
  Object.assign(lease, await rpc('browser.lease.acquire', { sessionId: session.sessionId, holder: 'w09-acceptance' }));
  const viewerPopup = (await rpc('browser.pages.list', { sessionId: session.sessionId }))
    .find(candidate => candidate.url === new URL('/popup', pageUrl).href);
  assert.ok(viewerPopup);
  await rpc('browser.page.close', { ...projectionParams, pageId: viewerPopup.pageId, leaseSecret: lease.leaseSecret });
  // Restore the original size for the legacy fixtures that use fixed click coordinates.
  // 恢复原始尺寸，供使用固定点击坐标的旧夹具使用。
  await rpc('browser.viewport.set', {
    sessionId: session.sessionId, leaseSecret: lease.leaseSecret,
    width: projected.result.width, height: projected.result.height,
  });
  await rpc('browser.session.navigate', {
    sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret, url: pageUrl,
  });
  // rpcSubscriptionId is reused deliberately: one-shot RPC snapshots must release their observer.
  // rpcSubscriptionId 被刻意复用：一次性 RPC 快照必须释放自己的观察订阅。
  const rpcSubscriptionId = `w09-rpc-${crypto.randomUUID()}`;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    // rpcTicket is a fresh single-use grant for this independent snapshot attempt.
    // rpcTicket 是本次独立快照尝试使用的新一次性授权。
    const rpcTicket = await rpc('browser.stream.ticket', { sessionId: session.sessionId });
    // rpcSnapshot proves the registered page can be observed without retaining a WebSocket.
    // rpcSnapshot 证明无需持续 WebSocket 也能观察已登记页面。
    const rpcSnapshot = await rpc('browser.stream.resume', {
      sessionId: session.sessionId, pageId: page.pageId,
      subscriptionId: rpcSubscriptionId, ticket: rpcTicket.ticket,
    });
    assert.equal(rpcSnapshot.mode, 'snapshot');
  }
  // ticket authorizes one WebSocket observation of the same Core session.
  // ticket 授权对此 Core 会话的一次 WebSocket 观察。
  const ticket = await rpc('browser.stream.ticket', { sessionId: session.sessionId });
  // streamUrl carries the one-use ticket, page identity, and service instance token.
  // streamUrl 携带一次性凭据、页面身份和服务实例令牌。
  const streamUrl = new URL(`${discovery.baseUrl}/v1/browser/stream`);
  streamUrl.protocol = 'ws:';
  streamUrl.searchParams.set('ticket', ticket.ticket);
  streamUrl.searchParams.set('pageId', page.pageId);
  // streamSubscriptionId names the exact observer that the cancellation probe must retire.
  // streamSubscriptionId 标识取消探针必须终止的精确观察订阅。
  const streamSubscriptionId = `w09-${crypto.randomUUID()}`;
  streamUrl.searchParams.set('subscriptionId', streamSubscriptionId);
  streamUrl.searchParams.set('token', discovery.serviceToken);
  // streamStartedAt measures the user-visible time from connecting to the first real image.
  // streamStartedAt 测量从连接到首张真实图像的用户可见时间。
  const streamStartedAt = Date.now();
  // frameTimes records received complete frames for a short throughput measurement.
  // frameTimes 记录收到的完整帧时间，供短时吞吐量测量。
  const frameTimes = [];
  // latestFrame records the live generation after popup creation or closure.
  // latestFrame 记录弹窗创建或关闭后的实时代次。
  let latestFrame = null;
  // frameHashes distinguishes a real post-input image from an unrelated delivery tick.
  // frameHashes 区分输入后的真实新画面与无关交付时钟。
  const frameHashes = [];
  // socket receives metadata and actual binary JPEG frames from the service.
  // socket 从服务接收元数据和真实二进制 JPEG 画面。
  const socket = new WebSocket(streamUrl);
  // frame is the first real image produced for this page.
  // frame 是为此页面生成的第一张真实图像。
  const frame = await Promise.race([
    new Promise((resolve, reject) => socket.addEventListener('message', async (event) => {
      if (typeof event.data === 'string') {
        const metadata = JSON.parse(event.data);
        if (metadata.kind === 'error') reject(new Error(JSON.stringify(metadata)));
        return;
      }
      const packet = Buffer.from(event.data instanceof ArrayBuffer ? event.data : await event.data.arrayBuffer());
      const length = packet.readUInt32BE(0);
      const header = JSON.parse(packet.subarray(4, 4 + length).toString('utf8'));
      const image = packet.subarray(4 + length);
      if (header.kind === 'frame' && header.pageId === page.pageId && image.length > 100) {
        latestFrame = header;
        frameTimes.push(Date.now());
        frameHashes.push({ time: Date.now(), hash: createHash('sha256').update(image).digest('hex') });
        assert.equal(image[0], 0xff);
        assert.equal(image[1], 0xd8);
        resolve(header);
      }
    })),
    new Promise((_, reject) => {
      // timer bounds first-frame latency without retaining an otherwise completed probe.
      // timer 限制首帧等待时间，且不会拖住已完成的探针。
      const timer = setTimeout(() => reject(new Error('Timed out waiting for first browser frame.')), 20_000);
      timer.unref();
    }),
  ]);
  // firstFrameMs and observedFps are measurements, not assertions about every machine.
  // firstFrameMs 与 observedFps 是测量值，不对所有机器作统一断言。
  const firstFrameMs = frameTimes[0] - streamStartedAt;
  await new Promise((resolve) => setTimeout(resolve, 2000));
  const baselineStart = Date.now();
  await new Promise((resolve) => setTimeout(resolve, 4000));
  const recentFrames = frameTimes.filter(time => time >= baselineStart && time < baselineStart + 4000).length;
  const observedFps = Number((recentFrames / 4).toFixed(1));
  // A deliberately paused raw WebSocket verifies one slow observer does not hold the producer.
  // 故意暂停读取的原始 WebSocket 验证慢观察端不会占住画面生产者。
  const slowTicket = await rpc('browser.stream.ticket', { sessionId: session.sessionId });
  const slowUrl = new URL(streamUrl);
  slowUrl.searchParams.set('ticket', slowTicket.ticket);
  slowUrl.searchParams.set('subscriptionId', `w09-slow-${crypto.randomUUID()}`);
  const slowSocket = await new Promise((resolve, reject) => {
    const candidate = createConnection({ host: slowUrl.hostname, port: Number(slowUrl.port) });
    candidate.once('error', reject);
    candidate.once('connect', () => {
      const key = randomBytes(16).toString('base64');
      candidate.write(`GET ${slowUrl.pathname}${slowUrl.search} HTTP/1.1\r\nHost: ${slowUrl.host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ${key}\r\nSec-WebSocket-Version: 13\r\n\r\n`);
    });
    candidate.once('data', data => {
      if (!data.toString('latin1').startsWith('HTTP/1.1 101')) {
        reject(new Error(`Slow observer upgrade refused: ${data.toString('latin1').slice(0, 160)}`));
        candidate.destroy();
        return;
      }
      candidate.pause();
      resolve(candidate);
    });
  });
  const slowStart = Date.now();
  await new Promise((resolve) => setTimeout(resolve, 6000));
  const slowFrameCount = frameTimes.filter(time => time >= slowStart && time < slowStart + 6000).length;
  const observerFpsWhileSlow = Number((slowFrameCount / 6).toFixed(1));
  slowSocket.destroy();
  assert.ok(observerFpsWhileSlow >= Math.max(10, observedFps * 0.7),
    `Normal observer slowed from ${observedFps} to ${observerFpsWhileSlow} fps`);
  // input sends a real pointer down/up pair under the frame's generations.
  // input 使用此帧的代次发送真实指针按下和释放事件。
  for (const [sequence, type] of [['1', 'down'], ['2', 'up']]) {
    const input = await rpc('browser.input.dispatch', {
      sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
      pageGeneration: frame.pageGeneration, viewportGeneration: frame.viewportGeneration,
      sequence, kind: 'pointer', payload: { type, x: 20, y: 20, button: 'left' },
    });
    assert.equal(input.outcome, 'applied', JSON.stringify(input));
  }
  await until(() => { assert.equal(clicks, 1); return true; }, 5000);
  // staged contains path-free metadata for a Chinese-named upload.
  // staged 包含中文文件名上传的不含路径元数据。
  const staged = await rpc('browser.upload.stage', {
    sessionId: session.sessionId, leaseSecret: lease.leaseSecret,
    name: '中文文件.txt', dataBase64: Buffer.from('中文内容', 'utf8').toString('base64'),
  });
  assert.equal(staged.displayName, '中文文件.txt');
  assert.equal('stagedPath' in staged, false);
  await rpc('browser.upload.attach', {
    sessionId: session.sessionId, pageId: page.pageId,
    leaseSecret: lease.leaseSecret, uploadId: staged.uploadId,
  });
  await until(() => { assert.deepEqual(uploaded, { name: '中文文件.txt', text: '中文内容' }); return true; }, 5000);
  // A real page click must create a Core-owned download with authenticated content.
  // 真实页面点击必须生成由 Core 持有且仅能鉴权读取内容的下载。
  for (const [sequence, type] of [['3', 'down'], ['4', 'up']]) {
    const input = await rpc('browser.input.dispatch', {
      sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
      pageGeneration: frame.pageGeneration, viewportGeneration: frame.viewportGeneration,
      sequence, kind: 'pointer', payload: { type, x: 30, y: 320, button: 'left' },
    });
    assert.equal(input.outcome, 'applied', JSON.stringify(input));
  }
  // download is the path-free record persisted by the owning Core.
  // download 是由归属 Core 保存且不包含宿主路径的记录。
  const download = await until(async () => {
    const records = await rpc('browser.download.list', { sessionId: session.sessionId });
    const completed = records.find((record) => record.fileName === 'fixture-download.txt' && record.status === 'completed');
    assert.ok(completed, JSON.stringify(records));
    return completed;
  }, 10_000);
  assert.equal(download.fileName, 'fixture-download.txt');
  assert.equal(download.sha256, createHash('sha256').update('core-download-content').digest('hex'));
  assert.equal('path' in download, false);
  const contentUrl = `${discovery.baseUrl}/v1/browser/downloads/${download.downloadId}/content`;
  const unauthenticated = await fetch(contentUrl);
  assert.equal(unauthenticated.status, 401);
  const queryCredential = await fetch(`${contentUrl}?token=${encodeURIComponent(discovery.serviceToken)}`);
  assert.equal(queryCredential.status, 401);
  const content = await fetch(contentUrl, { headers: {
    'x-vulcan-service-token': discovery.serviceToken,
    'x-vulcan-browser-owner-session-id': ownerSessionId,
  } });
  assert.equal(content.status, 200);
  assert.equal(content.headers.get('content-disposition'), 'attachment; filename*=UTF-8\'\'fixture-download.txt');
  assert.equal(await content.text(), 'core-download-content');
  const wrongOwner = await fetch(contentUrl, { headers: {
    'x-vulcan-service-token': discovery.serviceToken,
    'x-vulcan-browser-owner-session-id': 'conversation-other',
  } });
  assert.equal(wrongOwner.status, 404);
  const head = await fetch(contentUrl, {
    method: 'HEAD',
    headers: {
      'x-vulcan-service-token': discovery.serviceToken,
      'x-vulcan-browser-owner-session-id': ownerSessionId,
    },
  });
  assert.equal(head.status, 200);
  assert.equal(head.headers.get('content-length'), String('core-download-content'.length));
  assert.equal(await head.text(), '');
  // A controller remains active during the longer Chromium download/dialog scenario.
  // 控制者在较长的 Chromium 下载与对话框场景期间持续保持租约。
  await rpc('browser.lease.heartbeat', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  for (const [sequence, type] of [['5', 'down'], ['6', 'up']]) {
    const input = await rpc('browser.input.dispatch', {
      sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
      pageGeneration: frame.pageGeneration, viewportGeneration: frame.viewportGeneration,
      sequence, kind: 'pointer', payload: { type, x: 30, y: 400, button: 'left' },
    });
    assert.equal(input.outcome, 'applied', JSON.stringify(input));
  }
  await until(() => { assert.equal(dialogClicks, 1); return true; }, 5000);
  const dialog = await until(async () => {
    const dialogs = await rpc('browser.dialog.list', { sessionId: session.sessionId });
    assert.equal(dialogs.length, 1, JSON.stringify({ dialogs, dialogClicks, dialogAnswer, dialogResultCount }));
    return dialogs[0];
  }, 5000);
  assert.equal(dialog.dialogType, 'alert');
  assert.equal(dialog.pageId, page.pageId);
  const observerAnswer = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method: 'browser.dialog.respond', params: { ownerSessionId,
      sessionId: session.sessionId, pageId: page.pageId, dialogId: dialog.dialogId,
      leaseSecret: 'observer-has-no-lease', accept: true, promptText: 'wrong',
    } }),
  });
  assert.equal((await observerAnswer.json()).ok, false);
  await rpc('browser.dialog.respond', {
    sessionId: session.sessionId, pageId: page.pageId, dialogId: dialog.dialogId,
    leaseSecret: lease.leaseSecret, accept: true,
  });
  await until(() => { assert.deepEqual(dialogAnswer, { answer: 'accepted' }); return true; }, 5000);
  await rpc('browser.lease.heartbeat', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  for (const [sequence, type] of [['7', 'down'], ['8', 'up']]) {
    const input = await rpc('browser.input.dispatch', {
      sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
      pageGeneration: frame.pageGeneration, viewportGeneration: frame.viewportGeneration,
      sequence, kind: 'pointer', payload: { type, x: 30, y: 480, button: 'left' },
    });
    assert.equal(input.outcome, 'applied', JSON.stringify(input));
  }
  const confirmDialog = await until(async () => {
    const dialogs = await rpc('browser.dialog.list', { sessionId: session.sessionId });
    assert.equal(dialogs.length, 1, JSON.stringify(dialogs));
    return dialogs[0];
  }, 5000);
  assert.equal(confirmDialog.dialogType, 'confirm');
  await rpc('browser.dialog.respond', {
    sessionId: session.sessionId, pageId: page.pageId, dialogId: confirmDialog.dialogId,
    leaseSecret: lease.leaseSecret, accept: false,
  });
  await until(() => { assert.deepEqual(dialogAnswer, { answer: false }); return true; }, 5000);
  await rpc('browser.lease.heartbeat', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  // Chromium prompt must retain its synchronous return value across Core's dialog RPC.
  // Chromium prompt 必须通过 Core 对话框 RPC 保留其同步返回值。
  for (const [sequence, type] of [['9', 'down'], ['10', 'up']]) {
    const input = await rpc('browser.input.dispatch', {
      sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
      pageGeneration: frame.pageGeneration, viewportGeneration: frame.viewportGeneration,
      sequence, kind: 'pointer', payload: { type, x: 30, y: 560, button: 'left' },
    });
    assert.equal(input.outcome, 'applied', JSON.stringify(input));
  }
  const promptDialog = await until(async () => {
    const dialogs = await rpc('browser.dialog.list', { sessionId: session.sessionId });
    assert.equal(dialogs.length, 1, JSON.stringify(dialogs));
    return dialogs[0];
  }, 5000);
  assert.equal(promptDialog.dialogType, 'prompt');
  assert.equal(promptDialog.defaultPrompt, 'initial');
  await rpc('browser.dialog.respond', {
    sessionId: session.sessionId, pageId: page.pageId, dialogId: promptDialog.dialogId,
    leaseSecret: lease.leaseSecret, accept: true, promptText: '来自 Core',
  });
  await until(() => { assert.deepEqual(dialogAnswer, { answer: '来自 Core' }); return true; }, 5000);
  await rpc('browser.lease.heartbeat', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  // Clicking the actual file input must be intercepted instead of opening a hidden native dialog.
  // 点击真实文件输入必须被拦截，而不是打开一个隐藏的本机对话框。
  for (const [sequence, type] of [['11', 'down'], ['12', 'up']]) {
    const input = await rpc('browser.input.dispatch', {
      sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
      pageGeneration: frame.pageGeneration, viewportGeneration: frame.viewportGeneration,
      sequence, kind: 'pointer', payload: { type, x: 30, y: 190, button: 'left' },
    });
    assert.equal(input.outcome, 'applied', JSON.stringify(input));
  }
  const chooser = await until(async () => {
    const records = await rpc('browser.fileChooser.list', { sessionId: session.sessionId });
    assert.equal(records.length, 1, JSON.stringify(records));
    return records[0];
  }, 5000);
  assert.equal(chooser.pageId, page.pageId);
  const selected = await rpc('browser.upload.stage', {
    sessionId: session.sessionId, leaseSecret: lease.leaseSecret,
    name: '选择文件.txt', dataBase64: Buffer.from('selection-content', 'utf8').toString('base64'),
  });
  await rpc('browser.upload.attach', {
    sessionId: session.sessionId, pageId: page.pageId, chooserId: chooser.chooserId,
    leaseSecret: lease.leaseSecret, uploadId: selected.uploadId,
  });
  await until(() => { assert.deepEqual(uploaded, { name: '选择文件.txt', text: 'selection-content' }); return true; }, 5000);
  // Generic Playwright creates an actual new target; Core must register and retire its stable page.
  // 通用 Playwright 创建真实新目标；Core 必须登记并注销其稳定页面。
  // Repeated visible mutations measure the p95 input-to-frame delay on the same page.
  // 在同一页面重复可见变化，测量输入到新画面的第 95 百分位延迟。
  const inputLatencies = [];
  // inputRpcDurations records the down/up RPC wait independently of frame delivery.
  // inputRpcDurations 独立于画面交付记录按下和抬起 RPC 的等待时间。
  const inputRpcDurations = [];
  // inputStartedAt pairs each actual page callback with its input request start.
  // inputStartedAt 将每个真实页面回调与其输入请求起点配对。
  const inputStartedAt = [];
  for (let index = 0; index < 20; index += 1) {
    if (index % 5 === 0) await rpc('browser.lease.heartbeat', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
    const previousHash = frameHashes.at(-1)?.hash;
    const sentAt = Date.now();
    inputStartedAt.push(sentAt);
    for (const [offset, type] of [[0, 'down'], [1, 'up']]) {
      const rpcStartedAt = Date.now();
      const input = await rpc('browser.input.dispatch', {
        sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
        pageGeneration: latestFrame.pageGeneration, viewportGeneration: latestFrame.viewportGeneration,
        sequence: String(13 + index * 2 + offset), kind: 'pointer',
        payload: { type, x: 600, y: 50, button: 'left' },
      });
      inputRpcDurations.push(Date.now() - rpcStartedAt);
      assert.equal(input.outcome, 'applied', JSON.stringify(input));
    }
    const delay = await until(() => {
      const changed = frameHashes.find(candidate => candidate.time >= sentAt && candidate.hash !== previousHash);
      assert.ok(changed);
      return changed.time - sentAt;
    }, 1500);
    inputLatencies.push(delay);
  }
  const p95InputToFrameMs = inputLatencies.toSorted((left, right) => left - right)[Math.ceil(inputLatencies.length * 0.95) - 1];
  // Core creates an additional page through the authenticated service contract. The separate
  // MCP acceptance exercises Playwright's private one-use relay; no public CDP port is expected.
  // Core 通过鉴权服务契约创建附加页面。独立 MCP 验收覆盖 Playwright 私有一次性中继；
  // 此处不应期待对外开放的 CDP 端口。
  const pagesAfterCreate = await rpc('browser.page.create', {
    sessionId: session.sessionId, leaseSecret: lease.leaseSecret, url: pageUrl,
  });
  const createdPage = pagesAfterCreate.find(record => record.pageId !== page.pageId && record.url === pageUrl);
  assert.ok(createdPage, JSON.stringify(pagesAfterCreate));
  const otherOwnerPages = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method: 'browser.pages.list', params: {
      ownerSessionId: 'conversation-other', sessionId: session.sessionId,
    } }),
  });
  assert.equal((await otherOwnerPages.json()).ok, false);
  await until(async () => {
    const current = await rpc('browser.session.snapshot', { sessionId: session.sessionId });
    assert.equal(latestFrame?.pageGeneration, current.pageGeneration);
    return true;
  }, 5000);
  // Measure the original observed page again after another Core page exists and generation settles.
  // 附加 Core 页面存在且页面代次稳定后，再次测量原观察页面的输入到画面延迟。
  const multiPageInputLatencies = [];
  // multiPageInputRpcDurations isolates host-control contention after another page exists.
  // multiPageInputRpcDurations 隔离附加页面存在后的宿主控制通道争用。
  const multiPageInputRpcDurations = [];
  for (let index = 0; index < 12; index += 1) {
    if (index % 5 === 0) await rpc('browser.lease.heartbeat', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
    const previousHash = frameHashes.at(-1)?.hash;
    const sentAt = Date.now();
    inputStartedAt.push(sentAt);
    for (const [offset, type] of [[0, 'down'], [1, 'up']]) {
      const rpcStartedAt = Date.now();
      const input = await rpc('browser.input.dispatch', {
        sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
        pageGeneration: latestFrame.pageGeneration, viewportGeneration: latestFrame.viewportGeneration,
        sequence: String(53 + index * 2 + offset), kind: 'pointer',
        payload: { type, x: 600, y: 50, button: 'left' },
      });
      multiPageInputRpcDurations.push(Date.now() - rpcStartedAt);
      assert.equal(input.outcome, 'applied', JSON.stringify(input));
    }
    const delay = await until(() => {
      const changed = frameHashes.find(candidate => candidate.time >= sentAt && candidate.hash !== previousHash);
      assert.ok(changed);
      return changed.time - sentAt;
    }, 1500);
    multiPageInputLatencies.push(delay);
  }
  const p95InputToFrameWithTwoPagesMs = multiPageInputLatencies.toSorted((left, right) => left - right)[Math.ceil(multiPageInputLatencies.length * 0.95) - 1];
  assert.ok(p95InputToFrameMs <= 200,
    `Single-page input-to-frame p95 exceeded 200 ms: ${p95InputToFrameMs}`);
  assert.ok(p95InputToFrameWithTwoPagesMs <= 200,
    `Two-page input-to-frame p95 exceeded 200 ms: ${p95InputToFrameWithTwoPagesMs}`);
  await until(() => {
    assert.equal(latencyClickTimes.length, inputStartedAt.length);
    assert.ok(inputStartedAt.every((_, index) => Number.isFinite(latencyClickTimes[index])));
    return true;
  }, 5000);
  // callbackLagMs is an independent upper bound for input reaching the page event handler.
  // callbackLagMs 是输入抵达页面事件处理器的独立延迟上界。
  const callbackLagMs = latencyClickTimes.map((receivedAt, index) => receivedAt - inputStartedAt[index]);
  // Earlier fixtures already consumed slots. Test through the limit and the first rejected download.
  // 此前的夹具已经占用了名额，继续验证达到上限以及首次拒绝下载。
  const priorDownloads = await rpc('browser.download.list', { sessionId: session.sessionId });
  const remainingSlots = 16 - priorDownloads.filter(record => record.status === 'completed').length;
  for (let index = 0; index <= remainingSlots; index += 1) {
    if (index % 5 === 0) await rpc('browser.lease.heartbeat', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
    // The previous IDs distinguish this click from every earlier host download event.
    // 先前的身份集合将本次点击与所有更早的宿主下载事件区分开。
    const previousIds = new Set((await rpc('browser.download.list', { sessionId: session.sessionId }))
      .map(record => record.downloadId));
    for (const [offset, type] of [[0, 'down'], [1, 'up']]) {
      const input = await rpc('browser.input.dispatch', {
        sessionId: session.sessionId, pageId: page.pageId, leaseSecret: lease.leaseSecret,
        pageGeneration: latestFrame.pageGeneration, viewportGeneration: latestFrame.viewportGeneration,
        sequence: String(77 + index * 2 + offset), kind: 'pointer',
        payload: { type, x: 30, y: 320, button: 'left' },
      });
      assert.equal(input.outcome, 'applied', JSON.stringify(input));
    }
    // A terminal record proves the actual Chromium DownloadItem was accepted or rejected.
    // 终态记录证明真实 Chromium DownloadItem 已被接受或拒绝。
    const newDownload = await until(async () => {
      const records = await rpc('browser.download.list', { sessionId: session.sessionId });
      const added = records.filter(record => !previousIds.has(record.downloadId));
      assert.equal(added.length, 1, JSON.stringify(records));
      assert.notEqual(added[0].status, 'pending');
      return added[0];
    }, 10_000);
    assert.equal(newDownload.status, index < remainingSlots ? 'completed' : 'failed');
    if (index === remainingSlots) {
      // The rejected UUID has no file even though Core retains its path-free failed record.
      // 被拒绝的 UUID 不留下文件，Core 仍保留不含路径的失败记录。
      const rejectedPath = path.join(root, 'browser-profiles', 'control', session.sessionId, 'downloads', newDownload.downloadId);
      await assert.rejects(stat(rejectedPath), { code: 'ENOENT' });
    }
  }
  // cancelledStream records the actual WebSocket error and close after Core explicitly revokes it.
  // cancelledStream 记录 Core 显式撤销后真实 WebSocket 发出的错误和关闭事件。
  const cancelledStream = new Promise((resolve, reject) => {
    let interruption = null;
    const timer = setTimeout(() => reject(new Error('Cancelled browser stream stayed connected.')), 5_000);
    socket.addEventListener('message', (event) => {
      if (typeof event.data !== 'string') return;
      const message = JSON.parse(event.data);
      if (message.kind === 'error') interruption = message;
    });
    socket.addEventListener('close', () => {
      clearTimeout(timer);
      if (interruption?.code === 'browser_stream_interrupted') resolve(interruption);
      else reject(new Error(`Cancelled stream closed without interruption: ${JSON.stringify(interruption)}`));
    });
  });
  await rpc('browser.observe.unsubscribe', {
    sessionId: session.sessionId, subscriptionId: streamSubscriptionId,
  });
  const cancellation = await cancelledStream;
  await rpc('browser.session.close', { sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  // slowHostOperations records only screenshot and input timing diagnostics from this isolated host.
  // slowHostOperations 仅记录本次隔离宿主的截图和输入耗时诊断。
  const slowHostOperations = diagnostics.join('').split(/\r?\n/)
    .filter(line => line.includes('[vulcan-chromium-host] slow '));
  console.log(JSON.stringify({ result: 'passed', root, sessionId: session.sessionId, pageId: page.pageId, createdPageId: createdPage.pageId, frame: `${frame.width}x${frame.height}`, firstFrameMs, observedFps, observerFpsWhileSlow, p95InputToFrameMs, p95InputToFrameWithTwoPagesMs, inputLatencies, multiPageInputLatencies, callbackLagMs, inputRpcDurations, multiPageInputRpcDurations, slowHostOperations, clicks, uploaded, download: { id: download.downloadId, size: download.byteLength }, dialogAnswer, chooser: chooser.chooserId, rpcSnapshotReusable: true, cancelledStream: cancellation.code }));
} catch (error) {
  console.error(diagnostics.join(''));
  throw error;
} finally {
  service.kill();
  fixture.close();
  // Verify both generated roots and detach junctions before recursively deleting test files.
  // 递归删除测试文件前校验两个生成根目录，并先解除目录联接。
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
  for (const relative of [
    'desktop/runtime', 'browser/chromium', 'webui', 'dependencies',
    'compiled/node_modules',
  ]) await unlink(path.join(layoutRoot, relative));
  await rm(layoutRoot, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
}
