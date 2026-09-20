// Exercise the pinned official Playwright MCP through the host's private named-pipe relay.
// 通过宿主的私有命名管道中继验证固定版本的官方 Playwright MCP。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { access, mkdtemp, realpath, rm } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

import { PlaywrightProxy } from '../dist/chromium-host/playwright-proxy.js';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
// An explicit asset root keeps compatibility checks off stale shared output artifacts.
// 显式资源根让兼容性验收不依赖陈旧的共享 output 产物。
const assetRoot = process.env.VULCAN_W08_ASSET_ROOT ?? path.join(repository, 'output');
const managedNode = path.join(assetRoot, 'dependencies/runtimes/node/node-24.18.0-windows-x64/node.exe');
const mcpCli = path.join(assetRoot, 'plugins/vulcan-browser/node/node_modules/@playwright/mcp/cli.js');
const chromiumExecutable = path.join(assetRoot, 'browser/chromium/chrome-win64/chrome.exe');
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-w08-official-mcp-'));
const pageEvents = [];
const fixture = createServer((request, response) => {
  if (request.url === '/clicked') {
    pageEvents.push('clicked');
    response.writeHead(204).end();
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }).end(
    '<!doctype html><html><head><title>Vulcan MCP compatibility</title></head><body>' +
    '<h1>Real browser fixture</h1><button onclick="fetch(\'/clicked\');this.textContent=\'Clicked\'">Click here</button>' +
    '</body></html>',
  );
});

/** Waits for one asynchronous operation and labels a stalled protocol phase. */
/** 等待一个异步操作，并标记发生停滞的协议阶段。 */
async function bounded(operation, phase, timeout = 20_000) {
  let timer;
  try {
    return await Promise.race([operation, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${phase} timed out after ${timeout}ms`)), timeout);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

/** Owns one official MCP child and its line-delimited JSON-RPC requests. */
/** 管理一个官方 MCP 子进程及其按行分隔的 JSON-RPC 请求。 */
class StdioMcpClient {
  pending = new Map();
  nextId = 0;
  stderr = '';

  /** Starts MCP with the exact one-use Playwright endpoint grant. */
  /** 使用精确的一次性 Playwright 端点授权启动 MCP。 */
  constructor(endpoint, outputDirectory) {
    this.child = spawn(managedNode, [mcpCli, '--endpoint', endpoint, '--output-dir', outputDirectory], {
      cwd: root,
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    this.child.stderr.on('data', chunk => { this.stderr = (this.stderr + chunk.toString()).slice(-12_000); });
    createInterface({ input: this.child.stdout }).on('line', line => {
      let message;
      try { message = JSON.parse(line); }
      catch { this.stderr = (this.stderr + `\nNon-JSON stdout: ${line}`).slice(-12_000); return; }
      if (message.id === undefined) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(JSON.stringify(message.error)));
      else pending.resolve(message.result);
    });
    this.child.once('exit', (code, signal) => {
      for (const pending of this.pending.values()) pending.reject(new Error(`MCP exited ${code}/${signal}: ${this.stderr}`));
      this.pending.clear();
    });
  }

  /** Sends a request and waits for its exact JSON-RPC response. */
  /** 发送请求并等待对应的 JSON-RPC 响应。 */
  async request(method, params = {}) {
    const id = ++this.nextId;
    const operation = new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    });
    try { return await bounded(operation, method); }
    finally { this.pending.delete(id); }
  }

  /** Sends a JSON-RPC notification without waiting for a response. */
  /** 发送无需等待响应的 JSON-RPC 通知。 */
  notify(method, params = {}) {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  /** Stops the disposable MCP child process and waits briefly for exit. */
  /** 停止可丢弃的 MCP 子进程，并短暂等待其退出。 */
  async close() {
    if (this.child.exitCode !== null) return;
    // rmcp ends stdin before its bounded kill fallback; reproduce that graceful shutdown here.
    // rmcp 先结束 stdin 再执行有时限的强制终止；此处复现该正常关闭行为。
    const exited = new Promise(resolve => this.child.once('exit', resolve));
    this.child.stdin.end();
    await bounded(exited, 'MCP process exit', 5_000).catch(async () => {
      this.child.kill();
      await exited;
    });
  }
}

/** Finds one exact official browser tool descriptor. */
/** 查找一个精确的官方浏览器工具描述。 */
function requireTool(tools, name) {
  const tool = tools.find(candidate => candidate.name === name);
  assert.ok(tool, `Official MCP did not register ${name}`);
  return tool;
}

/** Calls one browser tool and rejects MCP-level failures. */
/** 调用一个浏览器工具，并拒绝 MCP 层面的失败结果。 */
async function callTool(client, name, args = {}) {
  const result = await client.request('tools/call', { name, arguments: args });
  assert.notEqual(result.isError, true, `${name}: ${JSON.stringify(result).slice(0, 3000)}`);
  return result;
}

await new Promise(resolve => fixture.listen(0, '127.0.0.1', resolve));
let context;
let proxy;
let client;
try {
  context = await chromium.launchPersistentContext(path.join(root, 'profile'), {
    executablePath: chromiumExecutable,
    headless: true,
    acceptDownloads: true,
  });
  // Block local-file and non-web navigation at the context boundary before any page can read host files.
  // 在上下文边界拦截本地文件和非 Web 导航，避免页面读取宿主文件。
  await context.route('**/*', async route => {
    const request = route.request();
    const scheme = new URL(request.url()).protocol;
    if (scheme === 'file:' || (request.isNavigationRequest() && scheme !== 'http:' && scheme !== 'https:'))
      await route.abort('blockedbyclient');
    else await route.continue();
  });
  const browser = context.browser();
  assert.ok(browser, 'Persistent context did not expose its browser owner.');
  proxy = new PlaywrightProxy(browser);
  const grant = await proxy.open('w08-official-mcp');
  const discovery = await fetch(grant.endpoint.replace(/^ws:/, 'http:'));
  assert.equal(discovery.status, 404, 'The one-use relay must not expose /json discovery.');
  await assert.rejects(access(path.join(root, 'profile', 'DevToolsActivePort')));
  const browserProcess = context._browser?.options?.browserProcess?.process;
  const spawnargs = browserProcess?.spawnargs ?? [];
  assert.equal(spawnargs.some(argument => argument.startsWith('--remote-debugging-port=')), false);

  client = new StdioMcpClient(grant.endpoint, path.join(root, 'mcp-output'));
  const initialized = await client.request('initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: { name: 'vulcan-w08-acceptance', version: '1.0.0' },
  });
  assert.ok(initialized.serverInfo?.name);
  client.notify('notifications/initialized');
  const listed = await client.request('tools/list');
  for (const name of ['browser_navigate', 'browser_snapshot', 'browser_take_screenshot', 'browser_click'])
    requireTool(listed.tools, name);
  assert.deepEqual(requireTool(listed.tools, 'browser_click').inputSchema.required, ['target']);

  const port = fixture.address().port;
  const navigated = await callTool(client, 'browser_navigate', { url: `http://127.0.0.1:${port}/` });
  assert.match(JSON.stringify(navigated.content), /Vulcan MCP compatibility/);
  const snapshot = await callTool(client, 'browser_snapshot');
  const snapshotText = snapshot.content.filter(item => item.type === 'text').map(item => item.text).join('\n');
  assert.match(snapshotText, /Click here/);
  const buttonReference = /button "Click here" \[ref=([^\]]+)\]/.exec(snapshotText)?.[1];
  assert.ok(buttonReference, `No clickable button reference in snapshot: ${snapshotText.slice(0, 3000)}`);
  const screenshot = await callTool(client, 'browser_take_screenshot', { type: 'png' });
  const image = screenshot.content.find(item => item.type === 'image');
  assert.ok(image, `Screenshot returned no image: ${JSON.stringify(screenshot).slice(0, 1000)}`);
  assert.deepEqual([...Buffer.from(image.data, 'base64').subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  await callTool(client, 'browser_click', { element: 'Click here button', target: buttonReference });
  await new Promise(resolve => setTimeout(resolve, 100));
  assert.deepEqual(pageEvents, ['clicked']);

  // Reproduce the production turn boundary: stop MCP before revoking its exact grant.
  // 复现生产回合边界：先停止 MCP，再撤销其精确授权。
  await client.close();
  assert.equal((await proxy.close('w08-official-mcp')).status, 'revoked');
  assert.ok(context.pages().some(page => page.url().startsWith(`http://127.0.0.1:${port}/`)), 'Turn completion closed the owned context.');
  // A second turn must reuse the same pages through a fresh one-use endpoint.
  // 第二轮必须通过新的一次性端点复用相同页面。
  const nextGrant = await proxy.open('w08-official-mcp');
  client = new StdioMcpClient(nextGrant.endpoint, path.join(root, 'mcp-output-next'));
  await client.request('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'vulcan-w08-next-turn', version: '1.0.0' } });
  client.notify('notifications/initialized');
  await callTool(client, 'browser_snapshot');

  const fileNavigation = await client.request('tools/call', {
    name: 'browser_navigate', arguments: { url: 'file:///C:/Windows/win.ini' },
  });
  assert.equal(fileNavigation.isError, true, 'file: navigation must be blocked by the host context.');
  assert.equal((await proxy.close('w08-official-mcp')).revoked, true);
  let refused = false;
  try {
    const later = await client.request('tools/call', { name: 'browser_navigate', arguments: { url: `http://127.0.0.1:${port}/after-close` } });
    refused = later.isError === true;
  } catch { refused = true; }
  assert.equal(refused, true, 'MCP continued to control Chromium after gateway revocation.');
  assert.ok(context.pages().some(page => page.url().startsWith(`http://127.0.0.1:${port}/`)), 'Revoking MCP must not close the owned context.');
  console.log(JSON.stringify({
    result: 'passed',
    mcpVersion: '0.0.82',
    server: initialized.serverInfo,
    toolNames: ['browser_navigate', 'browser_snapshot', 'browser_take_screenshot', 'browser_click'],
    screenshotBytes: Buffer.from(image.data, 'base64').length,
    hasRawCdpPort: spawnargs.some(argument => argument.startsWith('--remote-debugging-port=')),
    devToolsActivePort: false,
    relayHttpStatus: discovery.status,
    revoked: true,
  }, null, 2));
} catch (error) {
  console.error(`Official MCP compatibility failed: ${error.stack ?? error}`);
  if (client?.stderr) console.error(`Official MCP stderr:\n${client.stderr}`);
  throw error;
} finally {
  await client?.close();
  await proxy?.shutdown();
  await context?.close();
  await new Promise(resolve => fixture.close(resolve));
  const realRoot = await realpath(root);
  const realTemp = await realpath(tmpdir());
  assert.ok(realRoot.startsWith(realTemp + path.sep));
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
}
