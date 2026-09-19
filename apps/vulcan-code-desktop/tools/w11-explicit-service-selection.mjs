// Verify that an explicit WebUI Core target never falls through to another live local Core.
// 验证 WebUI 明确指定的 Core 目标不会回退到另一份正在运行的本地 Core。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { chromium } from 'playwright-core';

// Arguments name only exact test artifacts and an evidence directory, never the user's Core.
// 参数只命名精确测试产物与证据目录，绝不指向用户的 Core。
const inputs = process.argv.slice(2);
assert.equal(inputs.length, 4,
  'Expected service executable, runtime asset root, WebUI static root, and evidence root.');
const [serviceExecutable, assetRoot, webUiRoot, evidenceRoot] = inputs.map((value) => path.resolve(value));

// Wait for a bounded read-only observation and report its last failure.
// 有界等待只读观察，并报告最后一次失败。
// observe returns the accepted value; timeoutMs is the maximum wait.
// observe 返回接受的值；timeoutMs 是最长等待时间。
async function until(observe, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try { return await observe(); } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
  }
}

// Read a generated discovery document and verify the exact child's live service status.
// 读取生成的发现文档，并验证精确子进程的实时服务状态。
// stateRoot is the isolated Core root and child is its process; returns the ready discovery.
// stateRoot 是隔离 Core 根目录，child 是其进程；返回就绪的发现文档。
async function readyDiscovery(stateRoot, child) {
  return until(async () => {
    assert.equal(child.exitCode ?? child.signalCode, null, 'Isolated Core exited before readiness.');
    const document = JSON.parse(await readFile(path.join(stateRoot,
      'user-vulcan/code/webui-service.json'), 'utf8'));
    assert.equal(document.pid, child.pid);
    assert.equal(document.status, 'ready', JSON.stringify({ lastFailure: document.lastFailure }));
    const response = await fetch(`${document.baseUrl}/v1/service/status`, {
      headers: { 'x-vulcan-service-token': document.serviceToken },
      signal: AbortSignal.timeout(3_000),
    });
    assert.equal(response.status, 200);
    assert.equal((await response.json()).lifecycle, 'ready');
    return document;
  });
}

// Terminate only a child created by this test, including its descendants on Windows.
// 只终止本测试创建的子进程，在 Windows 上同时结束其后代。
// child is the spawned Core; returns after it exits.
// child 是启动的 Core；退出后返回。
async function stopIsolatedCore(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  if (process.platform === 'win32') {
    await new Promise((resolve, reject) => {
      const killer = spawn('rtk', ['proxy', 'taskkill', '/PID', String(child.pid), '/T', '/F'], {
        windowsHide: true, stdio: 'ignore',
      });
      killer.once('error', reject);
      killer.once('exit', (code) => code === 0 ? resolve() : reject(new Error(`taskkill exited ${code}`)));
    });
  } else {
    child.kill('SIGTERM');
  }
  await until(() => {
    assert.notEqual(child.exitCode ?? child.signalCode, null);
    return true;
  }, 15_000);
}

// Await one workbench delta from the exact selected service socket.
// 等待精确选中服务套接字的一条工作台增量。
// socket is the current event connection; returns its parsed delta payload.
// socket 是当前事件连接；返回解析后的增量负载。
function nextWorkbenchEvent(socket) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      socket.removeEventListener('message', onMessage);
      reject(new Error('The replacement service socket received no workbench event.'));
    }, 15_000);
    // onMessage ignores unrelated service events and settles only the requested event.
    // onMessage 忽略无关服务事件，仅由指定事件结束等待。
    function onMessage(event) {
      const message = JSON.parse(event.data);
      if (message.kind !== 'workbenchDelta') return;
      clearTimeout(timer);
      socket.removeEventListener('message', onMessage);
      resolve(message.payload);
    }
    socket.addEventListener('message', onMessage);
  });
}

// Add one generated local project through the live Core RPC endpoint.
// 通过实时 Core RPC 端点添加一个生成的本地项目。
// discovery owns the exact Core token and projectPath is test-owned; returns the RPC result.
// discovery 持有精确 Core 令牌，projectPath 属于测试；返回 RPC 结果。
async function addProject(discovery, projectPath) {
  const response = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST', headers: { 'content-type': 'application/json',
      'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method: 'service_add_project_from_path', params: { path: projectPath } }),
  });
  assert.equal(response.status, 200);
  const result = await response.json();
  assert.equal(result.ok, true, JSON.stringify(result.error));
  return result.result;
}

// Materialize a test conversation without enabling any real model connection.
// 物化测试会话，同时不启用任何真实模型连接。
// discovery owns the isolated Core; projectId and marker identify one draft; returns its RPC envelope.
// discovery 持有隔离 Core；projectId 与 marker 标识一份草稿；返回 RPC 信封。
async function submitUnavailableModel(discovery, projectId, marker) {
  const response = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST', headers: { 'content-type': 'application/json',
      'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method: 'service_submit_conversation_prompt', params: {
      clientSubmissionId: marker, projectId, conversationId: null,
      draftSlotKey: `new:${projectId}`, prompt: marker,
      modelSelection: { providerId: 'w11-unavailable', modelId: 'w11-unavailable',
        connectionId: 'w11-unavailable' },
    } }),
  });
  assert.equal(response.status, 200);
  const result = await response.json();
  assert.equal(result.ok, false, 'The isolated fixture unexpectedly started a model turn.');
  return result;
}

// Read the authoritative workbench after the second socket receives its event.
// 在第二条套接字收到事件后读取权威工作台。
// discovery identifies the isolated Core; returns its current workbench snapshot.
// discovery 标识隔离 Core；返回其当前工作台快照。
async function workbenchSnapshot(discovery) {
  const response = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST', headers: { 'content-type': 'application/json',
      'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method: 'workbench.snapshot', params: null }),
  });
  assert.equal(response.status, 200);
  const result = await response.json();
  assert.equal(result.ok, true, JSON.stringify(result.error));
  return result.result;
}

// Run two real isolated Cores and assert that two invalid explicit targets never reach either one.
// 运行两份真实隔离 Core，断言两种无效显式目标均不会连接其中任何一份。
// No parameters; writes one credential-free manifest and sets the process exit code on failure.
// 无参数；写入不含凭据的清单，并在失败时设置进程退出码。
async function main() {
  const runRoot = await mkdtemp(path.join(evidenceRoot, 'vulcan-h05-'));
  const stateRoots = [path.join(runRoot, 'core-a'), path.join(runRoot, 'core-b')];
  await Promise.all(stateRoots.map((stateRoot) => mkdir(stateRoot)));
  const children = stateRoots.map((stateRoot) => spawn(serviceExecutable,
    ['--headless', `--test-root=${stateRoot}`], {
      cwd: path.dirname(serviceExecutable), windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, VULCAN_CODE_OUTPUT_ROOT: assetRoot,
        VULCAN_WEBUI_STATIC_DIR: webUiRoot },
    }));
  const childErrors = ['', ''];
  children.forEach((child, index) => child.stderr.on('data', (chunk) => {
    childErrors[index] += chunk.toString('utf8');
  }));
  let browser = null;
  const record = { result: 'failed', cases: [], errors: [] };
  let oldEvents = null;
  let currentEvents = null;
  try {
    const [first, second] = await Promise.all(stateRoots.map((root, index) =>
      readyDiscovery(root, children[index])));
    assert.notEqual(first.baseUrl, second.baseUrl);
    browser = await chromium.launch({
      executablePath: path.join(assetRoot, 'browser/chromium/chrome-win64/chrome.exe'),
      headless: true,
    });
    for (const [caseId, selected, expected] of [
      ['H05_UNAVAILABLE', 'http://127.0.0.1:1', 'unavailable'],
      ['H05_INVALID', 'not-a-url', 'invalid'],
    ]) {
      const page = await browser.newPage();
      const businessRequests = [];
      await page.addInitScript(() => {
        window.__w11ConnectionEvents = [];
        window.addEventListener('vulcan:browser-service-connection', (event) =>
          window.__w11ConnectionEvents.push(event.detail));
      });
      page.on('request', (request) => {
        const url = new URL(request.url());
        if ([first.baseUrl, second.baseUrl].includes(url.origin)
          && /^\/(health|rpc|events|v1\/)/.test(url.pathname)) {
          businessRequests.push({ origin: url.origin, path: url.pathname });
        }
      });
      await page.goto(`${first.baseUrl}/?service=${encodeURIComponent(selected)}`);
      const connection = await until(async () => {
        const events = await page.evaluate(() => window.__w11ConnectionEvents);
        const failed = events.find((event) => event.status === 'disconnected'
          && event.message?.includes(expected));
        assert.ok(failed, JSON.stringify(events));
        return failed;
      }, 30_000);
      await page.waitForTimeout(1200);
      assert.deepEqual(businessRequests, [], 'Explicit failure reached another Core business route.');
      const cache = await page.evaluate(() => ({
        baseUrl: window.localStorage.getItem('vulcan.service.url'),
        token: window.sessionStorage.getItem('vulcan.service.token'),
      }));
      assert.equal(cache.baseUrl, null);
      assert.equal(cache.token, null);
      record.cases.push({ id: caseId, status: 'passed', error: connection.message,
        businessRequests: businessRequests.length, cachedEndpoint: false });
      await page.close();
    }
    // The same browser lifetime reconnects while two independent event-producing RPCs run.
    // 同一浏览器生命周期在两个独立的产出事件 RPC 之间重连。
    const projectPath = path.join(runRoot, 'h07-project');
    await mkdir(projectPath);
    const workbench = await addProject(first, projectPath);
    const project = workbench.projects.find((item) => item.path.includes('h07-project'));
    assert.ok(project, 'Isolated project did not enter the workbench.');
    const clientId = randomUUID();
    const eventUrl = new URL(first.baseUrl);
    eventUrl.protocol = 'ws:';
    eventUrl.pathname = '/events';
    eventUrl.searchParams.set('token', first.serviceToken);
    eventUrl.searchParams.set('clientId', clientId);
    oldEvents = new WebSocket(eventUrl);
    await until(() => {
      assert.equal(oldEvents.readyState, WebSocket.OPEN);
      return true;
    }, 15_000);
    currentEvents = new WebSocket(eventUrl);
    await until(() => {
      assert.equal(currentEvents.readyState, WebSocket.OPEN);
      return true;
    }, 15_000);
    const firstEvent = nextWorkbenchEvent(currentEvents);
    await submitUnavailableModel(first, project.id, `w11-before-${randomUUID()}`);
    const firstDelta = await firstEvent;
    oldEvents.close();
    await until(() => {
      assert.equal(oldEvents.readyState, WebSocket.CLOSED);
      return true;
    }, 15_000);
    const secondEvent = nextWorkbenchEvent(currentEvents);
    await submitUnavailableModel(first, project.id, `w11-after-${randomUUID()}`);
    const secondDelta = await secondEvent;
    assert.ok(secondDelta.generation > firstDelta.generation);
    assert.equal(currentEvents.readyState, WebSocket.OPEN);
    const recoveredSnapshot = await workbenchSnapshot(first);
    assert.equal(recoveredSnapshot.conversations.filter((item) => item.projectId === project.id).length, 2);
    record.cases.push({ id: 'H07_EVENT_GENERATION', status: 'passed',
      oldSocketClosed: true, newSocketReceivedBeforeAndAfter: true,
      recoveredConversations: 2 });
    // The checked-in 48x32 Chromium-generated WebM avoids intermittent empty recorder output.
    // 已入库的 48x32 Chromium 生成 WebM 避免录制器偶发空输出。
    const videoBytes = await readFile(new URL('./fixtures/w11-sample.webm', import.meta.url));
    assert.ok(videoBytes.length > 32, 'The fixture is too short for a 32-byte range request.');
    const videoPath = path.join(runRoot, '旧视频 样例.webm');
    await writeFile(videoPath, videoBytes);
    const encodedVideoPath = Buffer.from(videoPath, 'utf8').toString('base64url');
    const mediaRoute = `${first.baseUrl}/asset/${encodedVideoPath}`;
    const deniedVideo = await fetch(mediaRoute);
    assert.equal(deniedVideo.status, 401);
    const videoUrl = `${mediaRoute}?token=${encodeURIComponent(first.serviceToken)}`;
    const rangedVideo = await fetch(videoUrl, { headers: { range: 'bytes=0-31' } });
    if (rangedVideo.status !== 206) {
      throw new Error(`Media range status ${rangedVideo.status}: ${await rangedVideo.text()}`);
    }
    assert.equal(rangedVideo.status, 206);
    assert.match(rangedVideo.headers.get('content-type') ?? '', /video\/webm/);
    assert.match(rangedVideo.headers.get('content-range') ?? '', /^bytes 0-31\//);
    assert.equal((await rangedVideo.arrayBuffer()).byteLength, 32);
    const playbackPage = await browser.newPage();
    const playback = await playbackPage.evaluate(async (source) => {
      const video = document.createElement('video');
      video.muted = true;
      video.playsInline = true;
      video.src = source;
      document.body.appendChild(video);
      await new Promise((resolve, reject) => {
        video.addEventListener('loadedmetadata', resolve, { once: true });
        video.addEventListener('error', () => reject(new Error(`Video load failed: ${video.error?.code}`)),
          { once: true });
      });
      await video.play();
      const started = Date.now();
      while (video.currentTime < 0.2 && Date.now() - started < 5000) {
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      return { width: video.videoWidth, height: video.videoHeight,
        currentTime: video.currentTime };
    }, videoUrl);
    await playbackPage.close();
    assert.equal(playback.width, 48);
    assert.equal(playback.height, 32);
    assert.ok(playback.currentTime >= 0.2);
    record.cases.push({ id: 'H08_LEGACY_VIDEO', status: 'passed',
      authorizedRange: 206, unauthorized: 401, bytes: videoBytes.length,
      playbackWidth: playback.width, playbackTime: playback.currentTime });
    record.result = 'passed';
  } catch (error) {
    record.errors.push(String(error?.stack ?? error));
    record.errors.push(...childErrors.map((value, index) => `core-${index}: ${value.slice(-1500)}`));
    process.exitCode = 1;
  } finally {
    oldEvents?.close();
    currentEvents?.close();
    if (browser) await browser.close();
    await Promise.all(children.map((child) => stopIsolatedCore(child)));
    const realRun = await realpath(runRoot);
    for (const stateRoot of stateRoots) {
      // Every recursive removal must resolve to a direct child of this test's generated root.
      // 每次递归移除前都须确认真实目标是本测试生成目录的直接子目录。
      const realState = await realpath(stateRoot);
      assert.equal(path.dirname(realState), realRun);
      await rm(realState, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
    }
    await writeFile(path.join(runRoot, 'manifest.json'), JSON.stringify(record, null, 2) + '\n');
    console.log(JSON.stringify({ result: record.result,
      manifest: path.join(runRoot, 'manifest.json'), cases: record.cases.map(({ id, status }) => ({ id, status })) }));
  }
}

await main();
