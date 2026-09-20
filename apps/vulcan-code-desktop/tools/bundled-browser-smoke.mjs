// Exercise the shipped Node host through its real Core protocol, without starting Electron.
// 通过真实 Core 协议验证随包 Node 宿主，不启动 Electron。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { createServer } from 'node:http';

// The supplied root must be a fully packaged distribution, independent of source dependencies.
// 给定根目录必须是完整发行物，与源码依赖独立。
const distribution = path.resolve(process.argv[2]);
const layout = JSON.parse(await readFile(path.join(distribution, 'runtime-layout.json'), 'utf8'));
assert.equal(layout.schemaVersion, 2);
assert.notEqual(layout.browserHostExecutable, layout.desktopExecutable);
const { ControlFrameDecoder, encodeControlFrame, BROWSER_HOST_PROTOCOL_VERSION } = await import(pathToFileURL(path.join(distribution, 'browser/runtime/browser-host/host-protocol.js')));
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-bundled-browser-'));
const sessionId = randomUUID();
const hostGeneration = randomUUID();
const connectionSecret = randomUUID();
const child = spawn(path.join(distribution, layout.browserHostExecutable), [path.join(distribution, layout.browserHostEntry),
  `--session-id=${sessionId}`, `--host-generation=${hostGeneration}`, `--control-secret=${connectionSecret}`,
  `--user-data-dir=${path.join(root, 'profile')}`, `--control-dir=${path.join(root, 'control')}`,
  `--chromium-executable=${path.join(distribution, layout.chromiumExecutable)}`], {
  windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, HTTP_PROXY: 'http://127.0.0.1:1', HTTPS_PROXY: 'http://127.0.0.1:1', npm_config_offline: 'true' },
});
// Capture diagnostics without allowing a stalled process to leave test resources behind.
// 捕获诊断，同时避免停滞进程遗留测试资源。
let stderr = '';
child.stderr.on('data', chunk => { stderr += chunk; });
let socket;
const pending = new Map();
// Metadata is observed on the real host channel, including same-URL document.title changes.
// 在真实宿主通道观察元数据，包括同地址的 document.title 变化。
const metadata = [];
const fixture = createServer((_request, response) => {
  response.setHeader('content-type', 'text/html; charset=utf-8');
  response.end('<title>Initial title</title><h1>Local fixture</h1><script>setTimeout(()=>document.title="Updated title",250)</script>');
});
fixture.listen(0, '127.0.0.1');
await once(fixture, 'listening');
// Keep an owned MCP process per test turn, independent of the browser host process.
// 每轮测试保留一个独占 MCP 进程，与浏览器宿主进程独立。
let mcp;
const deadline = setTimeout(() => { socket?.destroy(); child.kill(); }, 30_000);

// Match request IDs on the authenticated channel and reject real host errors.
// 在鉴权通道上匹配请求标识，并拒绝真实宿主错误。
async function request(operation, params = {}) {
  const requestId = randomUUID();
  const response = new Promise((resolve, reject) => pending.set(requestId, { resolve, reject }));
  socket.write(encodeControlFrame({ requestId, sessionId, hostGeneration, operation, params }));
  return response;
}

try {
  // Wait only for this child's explicit discovery file, never scan unrelated ports.
  // 仅等待本子进程的显式发现文件，绝不扫描无关端口。
  let discovery;
  for (let attempt = 0; attempt < 200; attempt++) {
    if (child.exitCode !== null) throw new Error(`Host exited: ${stderr}`);
    try { discovery = JSON.parse(await readFile(path.join(root, 'control/browser-host-control.json'), 'utf8')); break; }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  assert.ok(discovery, stderr);
  assert.equal(discovery.pid, child.pid);
  const decoder = new ControlFrameDecoder();
  socket = createConnection(discovery.controlPort, discovery.controlHost);
  socket.on('error', error => {
    stderr += `\nControl socket: ${error.message}`;
    for (const waiter of pending.values()) waiter.reject(error);
    pending.clear();
  });
  socket.on('data', chunk => {
    for (const message of decoder.push(chunk)) {
      if (message.kind === 'pageMetadataChanged') metadata.push(message.payload);
      const waiter = pending.get(message.requestId);
      if (waiter) {
        pending.delete(message.requestId);
        if (message.ok) waiter.resolve(message.result);
        else waiter.reject(Object.assign(new Error(JSON.stringify(message.error)), { code: message.error.code }));
      }
    }
  });
  socket.on('close', () => {
    for (const waiter of pending.values()) waiter.reject(new Error(`Host closed: ${stderr}`));
    pending.clear();
  });
  await once(socket, 'connect');
  socket.write(encodeControlFrame({ kind: 'coreHello', protocolVersion: BROWSER_HOST_PROTOCOL_VERSION, sessionId, hostGeneration, connectionSecret }));
  assert.equal((await request('host.ping')).pid, child.pid);
  await request('page.create', { url: 'about:blank' });
  await request('page.create', { url: 'about:blank' });
  const { pages } = await request('page.list');
  assert.ok(pages.length >= 2);
  // Wait for the browser's own title event, rather than deriving labels from an address.
  // 等待浏览器自身的标题事件，不从地址推导标题。
  const fixtureUrl = `http://127.0.0.1:${fixture.address().port}/`;
  await request('page.navigate', { targetId: pages[0].targetId, url: fixtureUrl });
  for (let attempt = 0; attempt < 100 && !metadata.some(item => item.title === 'Updated title'); attempt++) {
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  assert.ok(metadata.some(item => item.targetId === pages[0].targetId && item.url === fixtureUrl && item.title === 'Updated title'), JSON.stringify(metadata));
  const frame = await request('page.capture', { targetId: pages[0].targetId });
  assert.equal(frame.mimeType, 'image/jpeg');
  assert.ok(frame.byteLength > 0);
  // Keep the GUI observation path active while MCP teardown retires automation.
  // MCP 清理退休自动化期间保持 GUI 观察路径活跃。
  let capturing = true;
  const captures = (async () => {
    while (capturing) {
      await request('page.capture', { targetId: pages[0].targetId, deferred: true, pageGeneration: '1', viewportGeneration: '1' });
      await new Promise(resolve => setTimeout(resolve, 30));
    }
  })();
  // Observe failures immediately so a host crash cannot become an unhandled test rejection.
  // 立即观察失败，避免宿主崩溃演变成测试中的未处理拒绝。
  captures.catch(() => {});
  // Exercise two real MCP turn boundaries against the packaged host while preserving its pages.
  // 针对打包宿主验证两个真实 MCP 回合边界，同时保留其页面。
  for (let turn = 0; turn < Number(process.env.VULCAN_BROWSER_SMOKE_TURNS ?? 2); turn++) {
    const connectionId = randomUUID();
    const grant = await request('automation.open', { connectionId });
    mcp = spawn(path.join(distribution, layout.browserHostExecutable), [path.join(distribution, 'plugins/vulcan-browser/node/node_modules/@playwright/mcp/cli.js'), '--endpoint', grant.endpoint, '--output-dir', path.join(root, `mcp-${turn}`)], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    mcp.stderr.on('data', chunk => { stderr += chunk; });
    const mcpPending = new Map();
    createInterface({ input: mcp.stdout }).on('line', line => {
      const message = JSON.parse(line);
      const waiter = mcpPending.get(message.id);
      if (!waiter) return;
      mcpPending.delete(message.id);
      if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    });
    // Send a bounded JSON-RPC request to this turn's exact process.
    // 向本轮精确进程发送有时限的 JSON-RPC 请求。
    const mcpRequest = (method, params) => {
      const id = randomUUID();
      return new Promise((resolve, reject) => {
        mcpPending.set(id, { resolve, reject });
        mcp.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
      });
    };
    await mcpRequest('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'bundled-turn-test', version: '1.0.0' } });
    mcp.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
    // An explicit reproduction URL exercises live navigation without changing the default offline fixture.
    // 显式复现网址用于验证真实导航，不改变默认离线夹具。
    if (process.env.VULCAN_BROWSER_SMOKE_URL) {
      const navigation = await mcpRequest('tools/call', { name: 'browser_navigate', arguments: { url: process.env.VULCAN_BROWSER_SMOKE_URL } });
      assert.notEqual(navigation.isError, true, JSON.stringify(navigation));
    }
    const result = await mcpRequest('tools/call', { name: 'browser_snapshot', arguments: {} });
    assert.notEqual(result.isError, true, JSON.stringify(result));
    const exited = once(mcp, 'exit');
    mcp.stdin.end();
    // Core requests MCP shutdown asynchronously, so gateway retirement can overlap process teardown.
    // Core 异步请求 MCP 关闭，因此网关退休可能与进程清理重叠。
    const revoked = await request('automation.close', { connectionId });
    await exited;
    assert.equal(revoked.status, 'revoked', JSON.stringify(revoked));
    assert.equal((await request('page.list')).pages.length, pages.length);
    assert.equal((await request('host.ping')).pid, child.pid);
  }
  capturing = false;
  await captures;
  // Closing one exact page preserves the other targets and the host process.
  // 关闭一个精确页面，其他目标与宿主进程保持存在。
  const closed = await request('page.close', { targetId: pages[1].targetId });
  assert.equal(closed.closed, true);
  const remaining = (await request('page.list')).pages;
  assert.equal(remaining.length, pages.length - 1);
  assert.ok(remaining.every(page => page.targetId !== pages[1].targetId));
  assert.equal((await request('host.ping')).pid, child.pid);
  await request('host.shutdown');
  if (child.exitCode === null) await once(child, 'exit');
  assert.equal(child.exitCode, 0, stderr);
  console.log(JSON.stringify({ result: 'passed', runtimeVersion: discovery.runtimeVersion, pages: pages.length, screenshotBytes: frame.byteLength, host: layout.browserHostExecutable, electronStarted: false }));
} finally {
  clearTimeout(deadline);
  fixture.closeAllConnections();
  fixture.close();
  if (mcp && mcp.exitCode === null) { mcp.kill(); await once(mcp, 'exit'); }
  socket?.destroy();
  if (child.exitCode === null) { child.kill(); await once(child, 'exit'); }
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
}
