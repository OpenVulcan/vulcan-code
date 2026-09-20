// Compare the exact pinned Electron and Chromium runtimes without modifying production routing.
// 对比精确锁定的 Electron 与 Chromium 运行时，不修改生产路由。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { connect } from 'node:net';
import { createRequire } from 'node:module';
import { createInterface } from 'node:readline';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, _electron } from 'playwright-core';
import { PlaywrightProxy } from '../dist/chromium-host/playwright-proxy.js';
import { waitForCdpEndpoint } from '../dist/browser-host/cdp-discovery.js';
import { BROWSER_HOST_PROTOCOL_VERSION, ControlFrameDecoder, encodeControlFrame } from '../dist/browser-host/host-protocol.js';

// Upgrade this tuple only in a dedicated adaptation change followed by the full compatibility run.
// 仅在独立适配变更并完成整组兼容性验收后升级此版本组合。
const PINNED = Object.freeze({ electron: '44.4.1', electronChromium: '152.0.7977.78',
  playwright: '1.64.0-alpha-2026-09-14', chromium: '154.0.8037.0', chromiumRevision: '1244', mcp: '0.0.81', node: '24.18.0' });
// Resolve all inputs explicitly; a missing input must never select another installed browser.
// 显式解析全部输入；缺失输入绝不选择其他已安装浏览器。
const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repository = path.resolve(desktop, '../..');
const requireDesktop = createRequire(path.join(desktop, 'package.json'));
const inputs = new Map(process.argv.slice(2).map(value => {
  const match = /^--(evidence-dir|mcp-root|chromium-executable)=(.+)$/.exec(value);
  assert.ok(match, `Unknown argument: ${value}`);
  assert.ok(path.isAbsolute(match[2]), 'All test paths must be absolute.');
  return [match[1], match[2]];
}));
assert.equal(inputs.size, 3, 'Required: --evidence-dir, --mcp-root, --chromium-executable');
const evidence = inputs.get('evidence-dir');
await mkdir(evidence, { recursive: true });
const runRoot = await mkdtemp(path.join(evidence, 'run-'));
const mcpRoot = inputs.get('mcp-root');
const mcpRequire = createRequire(path.join(mcpRoot, 'package.json'));
const electronExecutable = requireDesktop('electron');
const managedNode = path.join(repository, 'output/dependencies/runtimes/node/node-24.18.0-windows-x64/node.exe');
// Results retain every real failure, including unsupported operations; none are waived as a passing gate.
// 结果保留全部真实失败，包括不支持的操作；不会豁免失败而宣称关卡通过。
const report = { pinned: PINNED, startedAt: new Date().toISOString(), environment: {}, cases: [], cleanup: [] };

/** Bound an operation by phase; return its value or reject on timeout. */
/** 按阶段限制操作时长；返回操作结果或在超时时拒绝。 */
async function bounded(operation, phase, timeout = 15_000) {
  let timer;
  try { return await Promise.race([operation, new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${phase} timed out after ${timeout}ms`)), timeout);
  })]); } finally { clearTimeout(timer); }
}

/** Record the actual outcome of one named assertion without aborting unrelated checks. */
/** 记录一项具名断言的实际结果，不中断无依赖的检查。 */
async function check(runtime, name, operation) {
  try {
    const detail = await bounded(operation(), name);
    report.cases.push({ runtime, name, status: 'pass', detail: detail ?? null });
    console.log(`PASS ${runtime}/${name}`);
    return true;
  } catch (error) {
    report.cases.push({ runtime, name, status: 'fail', detail: String(error.message) });
    console.log(`FAIL ${runtime}/${name}: ${error.message}`);
    return false;
  }
}

/** Read one atomic discovery file until it appears; propagate malformed content and permission errors. */
/** 等待单个原子发现文件出现；格式错误与权限错误直接上报。 */
async function discovery(file, child) {
  const deadline = Date.now() + 15_000;
  for (;;) {
    if (child.exitCode !== null) throw new Error(`Host exited ${child.exitCode} before discovery.`);
    try { return JSON.parse(await readFile(file, 'utf8')); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    assert.ok(Date.now() < deadline, `Discovery timeout: ${file}`);
    await new Promise(resolve => setTimeout(resolve, 50));
  }
}

/** Start a real, isolated Electron host using the current Core framing and authenticate its control socket. */
/** 使用当前 Core 帧协议启动真实隔离的 Electron 宿主，并鉴权控制连接。 */
async function electronHost(name, initialUrl) {
  // Every host has an independent profile and process while reusing the same executable files.
  // 每个宿主具有独立画像和进程，同时复用相同可执行文件。
  const profile = path.join(runRoot, name);
  const identity = { sessionId: name, hostGeneration: randomUUID(), connectionSecret: randomUUID() };
  const env = { ...process.env }; delete env.ELECTRON_RUN_AS_NODE;
  const child = spawn(electronExecutable, [path.join(desktop, 'dist/browser-host/main.js'),
    `--session-id=${identity.sessionId}`, `--host-generation=${identity.hostGeneration}`,
    `--control-secret=${identity.connectionSecret}`, `--user-data-dir=${profile}`, `--initial-url=${initialUrl}`],
  { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let diagnostics = '';
  child.stdout.resume();
  child.stderr.on('data', chunk => { diagnostics += chunk.toString(); });
  const exited = new Promise(resolve => child.once('exit', (code, signal) => resolve({ code, signal })));
  const pending = new Map();
  let socket;
  let browser;
  /** Stop only this test-owned host and record whether graceful cleanup succeeded. */
  /** 只停止本次测试持有的宿主，并记录优雅清理是否成功。 */
  async function close() {
    socket?.end();
    try { await bounded(exited, `${name} shutdown`, 8_000); }
    catch (error) { child.kill(); await bounded(exited, `${name} forced exit`, 5_000); throw error; }
    finally { await writeFile(path.join(runRoot, `${name}.log`), diagnostics); }
    report.cleanup.push({ name, pid: child.pid, exited: child.exitCode !== null || child.signalCode !== null });
  }
  try {
    const endpoint = await discovery(path.join(profile, 'browser-host-control.json'), child);
    assert.equal(endpoint.protocolVersion, BROWSER_HOST_PROTOCOL_VERSION);
    socket = connect(endpoint.controlPort, endpoint.controlHost);
    const decoder = new ControlFrameDecoder();
    let handshakeResolve;
    const handshake = new Promise(resolve => { handshakeResolve = resolve; });
    socket.on('data', chunk => {
      for (const frame of decoder.push(chunk)) {
        if (frame.protocolVersion === BROWSER_HOST_PROTOCOL_VERSION && frame.sessionId === name) handshakeResolve(frame);
        if (typeof frame.requestId === 'string' && pending.has(frame.requestId)) {
          const request = pending.get(frame.requestId); pending.delete(frame.requestId);
          if (frame.ok) request.resolve(frame.result); else request.reject(new Error(JSON.stringify(frame.error)));
        }
      }
    });
    socket.on('error', error => { for (const request of pending.values()) request.reject(error); pending.clear(); });
    await bounded(new Promise((resolve, reject) => { socket.once('connect', resolve); socket.once('error', reject); }), 'control connect');
    socket.write(encodeControlFrame({ kind: 'coreHello', protocolVersion: BROWSER_HOST_PROTOCOL_VERSION, ...identity }));
    assert.equal((await bounded(handshake, 'Core handshake')).pid, child.pid);
    const cdp = await waitForCdpEndpoint(profile, 10_000);
    browser = await chromium.connectOverCDP(`http://${cdp.host}:${cdp.port}`, { timeout: 10_000 });
    const context = browser.contexts()[0]; assert.ok(context);
    context.setDefaultTimeout(5_000);
    return { context, browser, pid: child.pid, close, rawCdp: cdp,
      /** Send a current-protocol Core operation; return its exact correlated result. */
      /** 发送当前协议的 Core 操作；返回精确关联的结果。 */
      async request(operation, params = {}) {
        const requestId = randomUUID();
        try { return await bounded(new Promise((resolve, reject) => {
          pending.set(requestId, { resolve, reject });
          socket.write(encodeControlFrame({ requestId, sessionId: name, hostGeneration: identity.hostGeneration, operation, params }));
        }), operation); } finally { pending.delete(requestId); }
      },
    };
  } catch (error) { await close().catch(() => {}); throw error; }
}

/** One pinned MCP subprocess; requests have deadlines and exit failures reject their pending callers. */
/** 一个固定版本的 MCP 子进程；请求有时限，进程退出使等待的调用失败。 */
class McpClient {
  // State belongs exclusively to this subprocess, never to a running user session.
  // 状态只属于本子进程，绝不属于正在运行的用户会话。
  pending = new Map(); nextId = 0; stderr = '';
  /** Launch the exact MCP entry with its one-use gateway; output goes to the isolated evidence root. */
  /** 使用一次性网关启动精确 MCP 入口；输出写入隔离证据目录。 */
  constructor(endpoint, directory) {
    this.child = spawn(managedNode, [path.join(mcpRoot, 'cli.js'), '--endpoint', endpoint, '--output-dir', directory],
      { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: '1' } });
    this.exited = new Promise(resolve => this.child.once('exit', resolve));
    this.child.stderr.on('data', chunk => { this.stderr = (this.stderr + chunk.toString()).slice(-12_000); });
    createInterface({ input: this.child.stdout }).on('line', line => {
      const message = JSON.parse(line);
      if (!this.pending.has(message.id)) return;
      const request = this.pending.get(message.id); this.pending.delete(message.id);
      if (message.error) request.reject(new Error(JSON.stringify(message.error))); else request.resolve(message.result);
    });
    this.child.on('error', error => { for (const request of this.pending.values()) request.reject(error); this.pending.clear(); });
    this.child.once('exit', () => {
      for (const request of this.pending.values()) request.reject(new Error(`MCP exited: ${this.stderr}`));
      this.pending.clear();
    });
  }
  /** Send one JSON-RPC request and return the matching result or a bounded error. */
  /** 发送一项 JSON-RPC 请求，返回对应结果或有界错误。 */
  async request(method, params = {}) {
    const id = ++this.nextId;
    try { return await bounded(new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    }), method); } finally { this.pending.delete(id); }
  }
  /** Call a browser tool; return its content only when the MCP response reports success. */
  /** 调用浏览器工具；仅在 MCP 响应报告成功时返回内容。 */
  async tool(name, args = {}) {
    const result = await this.request('tools/call', { name, arguments: args });
    assert.notEqual(result.isError, true, JSON.stringify(result).slice(0, 2400));
    return result;
  }
  /** Stop the test child and await its exit; preserve stderr for diagnosis. */
  /** 停止测试子进程并等待退出；保留标准错误供诊断。 */
  async close() {
    if (this.child.exitCode === null) this.child.kill();
    await bounded(this.exited, 'MCP shutdown', 5_000);
  }
}

/** Exercise identical Playwright and official MCP operations against each real runtime. */
/** 针对每个真实运行时执行相同的 Playwright 与官方 MCP 操作。 */
async function exercise(name, context, origin) {
  const page = context.pages()[0]; assert.ok(page);
  context.setDefaultTimeout(5_000);
  await check(name, 'navigation-input-frame', async () => {
    await page.goto(origin);
    await page.getByLabel('Value').fill('locked-version');
    await page.getByRole('button', { name: 'Click here', exact: true }).click();
    assert.equal(await page.getByLabel('Value').inputValue(), 'locked-version');
    assert.equal(await page.locator('#counter').textContent(), '1');
    assert.equal(await page.frameLocator('iframe').locator('h1').textContent(), 'Fixture frame');
  });
  await check(name, 'playwright-hidden-screenshot', async () => {
    const screenshot = await page.screenshot();
    assert.deepEqual([...screenshot.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    return { screenshotBytes: screenshot.length };
  });
  await check(name, 'playwright-new-page', async () => {
    const added = await context.newPage(); await added.goto(`${origin}/new`); await added.close();
  });
  // Binding and tools are separate checks: a working proxy must not hide a failed MCP operation.
  // 绑定与工具分别检查：网关成功不能掩盖 MCP 操作失败。
  const proxy = new PlaywrightProxy(context.browser());
  let client;
  try {
    const granted = await check(name, 'official-mcp-connect', async () => {
      const grant = await proxy.open(name);
      assert.equal((await fetch(grant.endpoint.replace(/^ws:/, 'http:'))).status, 404);
      client = new McpClient(grant.endpoint, path.join(runRoot, `${name}-mcp`));
      const initialized = await client.request('initialize', { protocolVersion: '2025-06-18', capabilities: {},
        clientInfo: { name: 'shared-runtime-compatibility', version: '1.0.0' } });
      client.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
      const listed = await client.request('tools/list');
      assert.ok(listed.tools.some(tool => tool.name === 'browser_tabs'));
      await client.tool('browser_navigate', { url: origin });
      return { server: initialized.serverInfo, tools: listed.tools.length };
    });
    if (!granted) return;
    await check(name, 'official-mcp-snapshot-click', async () => {
      const snapshot = await client.tool('browser_snapshot');
      const text = snapshot.content.filter(item => item.type === 'text').map(item => item.text).join('\n');
      const reference = /button "Click here" \[ref=([^\]]+)\]/.exec(text);
      assert.ok(reference, text);
      await client.tool('browser_click', { target: reference[1], element: 'Click here' });
      assert.equal(await page.locator('#counter').textContent(), '1');
    });
    await check(name, 'official-mcp-hidden-screenshot', async () => {
      const image = await client.tool('browser_take_screenshot', { type: 'png' });
      assert.ok(image.content.some(item => item.type === 'image'));
    });
    await check(name, 'official-mcp-new-tab', async () => {
      await client.tool('browser_tabs', { action: 'new' });
      assert.ok(context.pages().length >= 2);
    });
    await check(name, 'official-mcp-revoke-preserves-host', async () => {
      assert.equal((await proxy.close(name)).revoked, true);
      const result = await client.request('tools/call', { name: 'browser_navigate', arguments: { url: `${origin}/revoked` } });
      assert.equal(result.isError, true);
      assert.ok(context.pages().length >= 1);
      assert.equal(context.pages().some(item => item.url() === `${origin}/revoked`), false);
    });
  } finally {
    if (client) { await client.close(); await writeFile(path.join(runRoot, `${name}-mcp.log`), client.stderr); }
    await proxy.shutdown();
  }
}

// One deterministic local website supplies the same oracle to Electron and standalone Chromium.
// 同一个确定性本地网站为 Electron 与独立 Chromium 提供相同断言依据。
const fixture = createServer((request, response) => {
  response.setHeader('content-type', 'text/html; charset=utf-8');
  response.end(request.url === '/frame' ? '<h1>Fixture frame</h1>' :
    '<!doctype html><title>Shared runtime fixture</title><h1>Shared runtime fixture</h1>' +
    '<label>Value<input></label><button onclick="document.querySelector(\'#counter\').textContent=Number(document.querySelector(\'#counter\').textContent)+1">Click here</button>' +
    '<output id="counter">0</output><iframe src="/frame"></iframe>');
});
let primary;
let secondary;
let standalone;
let electronApp;
try {
  const packageJson = JSON.parse(await readFile(path.join(desktop, 'package.json'), 'utf8'));
  assert.equal(packageJson.devDependencies.electron, PINNED.electron);
  assert.equal(packageJson.devDependencies['playwright-core'], PINNED.playwright);
  assert.equal(requireDesktop('electron/package.json').version, PINNED.electron);
  assert.equal(requireDesktop('playwright-core/package.json').version, PINNED.playwright);
  assert.equal(mcpRequire('./package.json').version, PINNED.mcp);
  assert.equal(mcpRequire('playwright-core/package.json').version, PINNED.playwright);
  assert.equal(mcpRequire('playwright/package.json').version, PINNED.playwright);
  const browserDescriptor = JSON.parse(await readFile(path.join(path.dirname(requireDesktop.resolve('playwright-core/package.json')), 'browsers.json'), 'utf8'))
    .browsers.find(browser => browser.name === 'chromium');
  assert.equal(browserDescriptor.revision, PINNED.chromiumRevision);
  assert.equal(browserDescriptor.browserVersion, PINNED.chromium);
  assert.equal(JSON.parse(await readFile(path.join(repository, 'plugins/vulcan-browser/node/package.json'), 'utf8')).dependencies['@playwright/mcp'], PINNED.mcp);
  assert.equal(process.version, `v${PINNED.node}`, 'Run this probe with the managed pinned Node executable.');
  await new Promise(resolve => fixture.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${fixture.address().port}`;
  primary = await electronHost('electron-primary', origin);
  report.environment.electronChromium = primary.browser.version();
  assert.equal(report.environment.electronChromium, PINNED.electronChromium);
  report.environment.electronExecutable = electronExecutable;
  await exercise('electron-cdp', primary.context, origin);
  await check('electron-cdp', 'core-owned-page-create', async () => {
    const created = await primary.request('page.create', { url: `${origin}/core-created` });
    await bounded(new Promise(resolve => {
      const timer = setInterval(() => {
        if (primary.context.pages().some(item => item.url() === `${origin}/core-created`)) { clearInterval(timer); resolve(); }
      }, 20);
      setTimeout(() => clearInterval(timer), 5_000).unref();
    }), 'Core-created page', 6_000);
    return created;
  });
  await check('electron-cdp', 'core-native-hidden-capture', async () => {
    const listing = await primary.request('page.list');
    const capture = await primary.request('page.capture', { targetId: listing.pages[0].targetId });
    assert.ok(capture.dataBase64.length > 0);
    return { mimeType: capture.mimeType, byteLength: capture.byteLength };
  });
  secondary = await electronHost('electron-secondary', origin);
  await check('electron-cdp', 'independent-process-cookie-isolation', async () => {
    assert.notEqual(primary.pid, secondary.pid);
    await primary.context.addCookies([{ name: 'owner', value: 'primary', url: origin }]);
    assert.equal((await primary.context.cookies(origin)).find(cookie => cookie.name === 'owner').value, 'primary');
    assert.equal((await secondary.context.cookies(origin)).some(cookie => cookie.name === 'owner'), false);
  });
  await check('electron-cdp', 'closing-one-host-preserves-other', async () => {
    await secondary.close(); secondary = null;
    await primary.context.pages()[0].goto(`${origin}/still-alive`);
    assert.equal(await primary.context.pages()[0].title(), 'Shared runtime fixture');
  });
  // The official Electron API is a separate candidate, so its outcome cannot be inferred from CDP.
  // 官方 Electron API 是另一候选接入方式，不能从 CDP 结果推断其表现。
  const electronEnv = { ...process.env }; delete electronEnv.ELECTRON_RUN_AS_NODE;
  electronApp = await _electron.launch({ executablePath: electronExecutable,
    args: [path.join(desktop, 'tools/fixtures/shared-runtime-electron.cjs'), `--profile=${path.join(runRoot, 'electron-api')}`,
      `--user-data-dir=${path.join(runRoot, 'electron-api')}`],
    env: electronEnv, timeout: 15_000 });
  const electronPage = await electronApp.firstWindow();
  await check('electron-api', 'native-window-and-input', async () => {
    await electronPage.goto(origin);
    await electronPage.getByLabel('Value').fill('electron-api');
    assert.equal(await electronPage.getByLabel('Value').inputValue(), 'electron-api');
    assert.equal(await electronApp.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().every(window => !window.isVisible())), true);
    report.environment.electronApiHasBrowserOwner = electronApp.context().browser() !== null;
  });
  await check('electron-api', 'playwright-new-page', async () => {
    const added = await electronApp.context().newPage(); await added.close();
  });
  await check('electron-api', 'playwright-hidden-screenshot', async () => {
    const bytes = await electronPage.screenshot({ timeout: 10_000 });
    assert.ok(bytes.length > 0); return { byteLength: bytes.length };
  });
  // Check whether the official Electron initialization also makes the existing MCP relay compatible.
  // 检查官方 Electron 初始化是否同时使现有 MCP 中继兼容。
  const electronApiEndpoint = await waitForCdpEndpoint(path.join(runRoot, 'electron-api'), 10_000);
  const electronApiBrowser = await chromium.connectOverCDP(`http://${electronApiEndpoint.host}:${electronApiEndpoint.port}`);
  try { await exercise('electron-api-cdp', electronApiBrowser.contexts()[0], origin); }
  finally { await electronApiBrowser.close(); }
  await electronApp.close(); electronApp = null;
  standalone = await chromium.launchPersistentContext(path.join(runRoot, 'chromium'), {
    executablePath: inputs.get('chromium-executable'), headless: true, acceptDownloads: true,
  });
  report.environment.standaloneChromium = standalone.browser().version();
  assert.equal(report.environment.standaloneChromium, PINNED.chromium);
  await exercise('standalone-chromium', standalone, origin);
} catch (error) {
  report.cases.push({ runtime: 'harness', name: 'setup', status: 'fail', detail: String(error.stack) });
} finally {
  for (const [name, close] of [['electron-api', () => electronApp?.close()], ['standalone', () => standalone?.close()], ['secondary', () => secondary?.close()], ['primary', () => primary?.close()]]) {
    try { await close(); } catch (error) { report.cleanup.push({ name, exited: false, error: String(error) }); }
  }
  fixture.closeAllConnections();
  await new Promise(resolve => fixture.close(resolve));
  report.finishedAt = new Date().toISOString();
  report.passed = report.cases.length > 0 && report.cases.every(item => item.status === 'pass') && report.cleanup.every(item => item.exited);
  await writeFile(path.join(runRoot, 'results.json'), JSON.stringify(report, null, 2));
  console.log(`Evidence: ${path.join(runRoot, 'results.json')}`);
  process.exitCode = report.passed ? 0 : 1;
}
