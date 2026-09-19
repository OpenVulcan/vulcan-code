// Verify sequential persistent Core browser reuse with real host generations and processes.
// 使用真实宿主代际和进程验收持久 Core 浏览器的顺序复用。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Resolve the repository and use the existing packaged runtime layout without synchronizing it.
// 解析仓库并使用现有运行时布局，不执行资源同步。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const serviceExecutable = path.join(repository, 'target/debug/vulcan-code-service.exe');
const runtimeLayout = path.join(repository, 'output/runtime-layout.json');
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-w07-persistent-'));
const ownerSessionId = 'w07-persistent-acceptance';
const diagnostics = [];

// Serve one deterministic local page so host startup registers a real page.
// 提供确定性的本地页面，使宿主启动时注册真实页面。
const fixture = createServer((_request, response) => {
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  response.end('<!doctype html><meta charset="utf-8"><title>persistent profile fixture</title>');
});
await new Promise(resolve => fixture.listen(0, '127.0.0.1', resolve));
const pageUrl = `http://127.0.0.1:${fixture.address().port}/`;

// Retry only read-only readiness or process-lifecycle observations.
// 仅重试只读就绪检查和进程生命周期观察。
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

// Return whether an exact host PID is still alive.
// 返回指定宿主 PID 是否仍然存活。
function processAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

// Check whether the raw Chromium endpoint still answers after the owning host closes.
// 检查归属宿主关闭后原始 Chromium 端点是否仍能响应。
async function cdpAlive(port) {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
      signal: AbortSignal.timeout(1_000),
    });
    return response.ok;
  } catch {
    return false;
  }
}

// Start one isolated service and keep all created profiles under its temporary root.
// 启动隔离服务，并将所有画像放在临时根目录下。
const service = spawn(serviceExecutable, ['--headless', `--test-root=${root}`], {
  env: { ...process.env, VULCAN_RUNTIME_LAYOUT: runtimeLayout },
  windowsHide: true,
  stdio: ['ignore', 'pipe', 'pipe'],
});
service.stdout.on('data', chunk => diagnostics.push(chunk.toString()));
service.stderr.on('data', chunk => diagnostics.push(chunk.toString()));

try {
  // Read and authenticate the exact isolated service instance.
  // 读取并认证精确的隔离服务实例。
  const discovery = await until(async () => {
    const document = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
    const health = await (await fetch(`${document.baseUrl}/health`)).json();
    assert.equal(health.status, 'ready');
    assert.equal(health.pid, service.pid);
    return document;
  });

  // Send an authenticated request scoped to the one VulcanCode conversation.
  // 发送一条限定到当前 VulcanCode 会话的认证请求。
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

  // Open one persistent session, start its real host, and capture endpoint evidence.
  // 打开一个持久会话，启动真实宿主并记录端点证据。
  async function openPersistent() {
    const session = await rpc('browser.session.create', { profileMode: 'persistent' });
    const lease = await rpc('browser.lease.acquire', { sessionId: session.sessionId, holder: 'w07-persistent' });
    const pages = await rpc('browser.pages.list', { sessionId: session.sessionId });
    assert.ok(pages.length > 0);
    await rpc('browser.session.navigate', {
      sessionId: session.sessionId,
      pageId: pages[0].pageId,
      leaseSecret: lease.leaseSecret,
      url: pageUrl,
    });
    const supervision = await rpc('browser.host.status', { sessionId: session.sessionId });
    assert.equal(supervision.state, 'running');
    assert.ok(Number.isInteger(supervision.pid) && supervision.pid > 0);
    const profileHash = createHash('sha256').update(ownerSessionId).digest('hex');
    const endpointPath = path.join(root, 'user-vulcan', 'code', 'browser-profiles', 'persistent', profileHash, 'DevToolsActivePort');
    const endpointText = await until(async () => {
      const value = await readFile(endpointPath, 'utf8');
      assert.ok(value.trim().length > 0);
      return value;
    });
    const port = Number(endpointText.split(/\r?\n/)[0]);
    assert.ok(Number.isInteger(port) && port > 0);
    return { session, lease, supervision, endpointPath, endpointText, port };
  }

  const first = await openPersistent();
  assert.equal(await cdpAlive(first.port), true);
  await rpc('browser.session.close', {
    sessionId: first.session.sessionId,
    leaseSecret: first.lease.leaseSecret,
  });
  const firstHostExited = await until(() => {
    assert.equal(processAlive(first.supervision.pid), false);
    return true;
  }, 20_000);
  const firstCdpClosed = await until(async () => {
    assert.equal(await cdpAlive(first.port), false);
    return true;
  }, 20_000);

  // The second session reuses the same profile only after the first host is gone.
  // 只有第一个宿主退出后，第二个会话才能复用同一个持久画像。
  const second = await openPersistent();
  assert.equal(second.endpointText === first.endpointText, false);
  // Host generations are scoped to a Core session; a new session starts at generation one.
  // 宿主代际按 Core 会话作用域编号；新会话从第一代开始。
  assert.equal(second.supervision.hostGeneration, '1');
  assert.notEqual(second.supervision.pid, first.supervision.pid);
  assert.equal(await cdpAlive(second.port), true);
  await rpc('browser.session.close', {
    sessionId: second.session.sessionId,
    leaseSecret: second.lease.leaseSecret,
  });
  const secondHostExited = await until(() => {
    assert.equal(processAlive(second.supervision.pid), false);
    return true;
  }, 20_000);
  const secondCdpClosed = await until(async () => {
    assert.equal(await cdpAlive(second.port), false);
    return true;
  }, 20_000);

  console.log(JSON.stringify({
    result: 'passed',
    root,
    profile: 'persistent',
    first: { sessionId: first.session.sessionId, hostGeneration: first.supervision.hostGeneration, pid: first.supervision.pid, port: first.port, hostExited: firstHostExited, cdpClosed: firstCdpClosed },
    second: { sessionId: second.session.sessionId, hostGeneration: second.supervision.hostGeneration, pid: second.supervision.pid, port: second.port, hostExited: secondHostExited, cdpClosed: secondCdpClosed },
    endpointChanged: first.endpointText !== second.endpointText,
  }));
} finally {
  service.kill();
  fixture.close();
}
