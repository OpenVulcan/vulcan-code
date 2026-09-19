// Exercise the packaged Core, standalone Chromium host, stream, and input against one local page.
// 使用一个本地页面验证打包 Core、独立 Chromium 宿主、投影流和输入。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { createServer } from 'node:http';
import { cp, link, mkdir, mkdtemp, readFile, realpath, rm, stat, symlink, unlink, writeFile } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

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
await mkdir(path.join(layoutRoot, 'compiled/node_modules'), { recursive: true });
await Promise.all([
  link(path.join(repository, 'target/debug/vulcan-code-service.exe'), path.join(layoutRoot, 'bin/vulcan-code-service.exe')),
  symlink(path.join(assetRoot, 'desktop/runtime'), path.join(layoutRoot, 'desktop/runtime'), 'junction'),
  symlink(path.join(assetRoot, 'browser/chromium'), path.join(layoutRoot, 'browser/chromium'), 'junction'),
  symlink(path.join(assetRoot, 'webui'), path.join(layoutRoot, 'webui'), 'junction'),
  symlink(await realpath(path.join(repository, 'apps/vulcan-code-desktop/node_modules/playwright-core')),
    path.join(layoutRoot, 'compiled/node_modules/playwright-core'), 'junction'),
  symlink(await realpath(path.join(repository, 'apps/vulcan-code-desktop/node_modules/ws')),
    path.join(layoutRoot, 'compiled/node_modules/ws'), 'junction'),
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
    const completed = records.find((record) => record.status === 'completed');
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
  // The initial completed download uses one slot; fifteen more succeed and the seventeenth is
  // reported as failed without leaving a file in Core's private control directory.
  // 最初的已完成下载占用一个名额；再有十五次成功，第十七次明确失败且不会留下私有控制目录文件。
  for (let index = 0; index < 16; index += 1) {
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
    assert.equal(newDownload.status, index < 15 ? 'completed' : 'failed');
    if (index === 15) {
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
    'desktop/runtime', 'browser/chromium', 'webui',
    'compiled/node_modules/playwright-core', 'compiled/node_modules/ws',
  ]) await unlink(path.join(layoutRoot, relative));
  await rm(layoutRoot, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
}
