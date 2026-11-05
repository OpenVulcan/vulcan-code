// Run one explicit desktop integration suite and retain a structured, isolated evidence manifest.
// 执行一组显式桌面集成验收，并保留结构化、隔离的证据清单。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { access, mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir, platform, release, arch } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { _electron as electron } from 'playwright-core';

// The integration executable and known suites are fixed so unknown requests cannot run arbitrary scripts.
// 固定集成入口与已知验收组，避免未知请求运行任意脚本。
const desktop = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repository = path.resolve(desktop, '../..');
const suites = new Set(['startup', 'window', 'browser', 'release', 'capacity']);
const execFileAsync = promisify(execFile);

// Parse exact --key=value inputs and reject duplicate, missing or unknown keys.
// 解析精确的 --key=value 输入，拒绝重复、缺失或未知键。
// argv contains CLI arguments; returns the validated suite and absolute artifact paths.
// argv 是命令行参数；返回已验证的验收组与绝对产物路径。
function parseArguments(argv) {
  const allowed = new Set(['suite', 'service-exe', 'desktop-runtime', 'desktop-entry',
    'evidence-dir', 'asset-root', 'runtime-layout']);
  const values = new Map();
  for (const argument of argv) {
    const match = /^--([a-z-]+)=(.+)$/.exec(argument);
    assert.ok(match && allowed.has(match[1]) && !values.has(match[1]), `Invalid integration argument: ${argument}`);
    values.set(match[1], match[2]);
  }
  for (const key of ['suite', 'service-exe', 'desktop-runtime', 'desktop-entry', 'evidence-dir']) {
    assert.ok(values.has(key), `Missing --${key}=...`);
  }
  assert.ok(suites.has(values.get('suite')), `Unknown integration suite: ${values.get('suite')}`);
  for (const key of ['service-exe', 'desktop-runtime', 'desktop-entry', 'evidence-dir',
    'asset-root', 'runtime-layout']) {
    if (values.has(key)) assert.ok(path.isAbsolute(values.get(key)), `--${key} must be absolute.`);
  }
  return Object.fromEntries(values);
}

// Execute a fixed child command while retaining its output under the run's evidence directory.
// 执行固定子命令，并将输出保存在本次运行的证据目录。
// executable and args are trusted suite targets; env is a controlled override; returns exit and output.
// executable 与 args 是已知验收目标；env 是受控覆盖；返回退出状态及输出。
async function runChild(executable, args, env, timeoutMs) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { cwd: repository, env, windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    // timedOut keeps a killed child from being mistaken for a passing suite by its final output.
    // timedOut 防止超时终止的子进程因残留输出而被误判为通过。
    let timedOut = false;
    // A timed-out suite must not leave its Core, GUI and browser descendants behind.
    // 超时验收不能遗留 Core、GUI 和浏览器子进程。
    const timer = setTimeout(() => {
      timedOut = true;
      if (process.platform === 'win32') {
        const killer = spawn('rtk', ['proxy', 'taskkill', '/PID', String(child.pid), '/T', '/F'], {
          windowsHide: true, stdio: 'ignore',
        });
        killer.on('error', () => child.kill());
        killer.on('exit', (code) => { if (code !== 0) child.kill(); });
      } else {
        child.kill('SIGTERM');
      }
    }, timeoutMs);
    child.stdout.on('data', (chunk) => { stdout += chunk.toString('utf8'); });
    child.stderr.on('data', (chunk) => { stderr += chunk.toString('utf8'); });
    child.once('error', (error) => { clearTimeout(timer); reject(error); });
    child.once('exit', (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal, timedOut, stdout, stderr });
    });
  });
}

// Wait for one read-only condition without turning a late child failure into an infinite wait.
// 有界等待一个只读条件，避免子进程迟发故障造成无限等待。
// observe returns accepted evidence or throws; timeoutMs is the maximum wait.
// observe 返回认可的证据或抛错；timeoutMs 是最长等待时间。
async function until(observe, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try { return await observe(); } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
  }
}

// Read the exact isolated Core discovery record and corroborate it with a live HTTP status.
// 读取精确隔离 Core 发现记录，并用实时 HTTP 状态交叉核验。
// testRoot is the unique runner-owned state directory; returns discovery and service status.
// testRoot 是运行器独占的状态目录；返回发现记录和服务状态。
async function liveDiscovery(testRoot) {
  const discovery = JSON.parse(await readFile(path.join(testRoot, 'user-vulcan/code/webui-service.json'), 'utf8'));
  const response = await fetch(`${discovery.baseUrl}/v1/service/status`, {
    headers: { 'x-vulcan-service-token': discovery.serviceToken }, signal: AbortSignal.timeout(3_000),
  });
  assert.equal(response.status, 200);
  const status = await response.json();
  assert.equal(status.lifecycle, 'ready', JSON.stringify({
    lifecycle: status.lifecycle, lastFailure: status.lastFailure,
  }));
  return { discovery, status };
}

// Exercise Core reuse and the live desktop window bridge using the exact selected artifacts.
// 使用精确指定的产物检验 Core 复用和真实桌面窗口桥。
// selected paths identify service and GUI files; testRoot isolates data; record stores case evidence.
// selected 路径指明服务和 GUI 文件；testRoot 隔离数据；record 保存用例证据。
async function runDesktopSuite(selected, testRoot, record) {
  const diagnostics = [];
  // The test-only hold makes starting visible long enough for two real GUI processes to attach.
  // 仅测试使用的延迟让 starting 阶段足够长，使两个真实 GUI 进程都能接入。
  const serviceEnvironment = { ...process.env,
    ...(selected['asset-root'] ? { VULCAN_CODE_OUTPUT_ROOT: selected['asset-root'] } : {}),
    ...(selected.suite === 'startup' ? { VULCAN_TEST_HOLD_STARTING: '1' } : {}),
  };
  const service = spawn(selected['service-exe'], ['--headless', `--test-root=${testRoot}`], {
    cwd: repository, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    env: serviceEnvironment,
  });
  service.stdout.on('data', (chunk) => diagnostics.push(chunk.toString('utf8')));
  service.stderr.on('data', (chunk) => diagnostics.push(chunk.toString('utf8')));
  let first = null;
  let second = null;
  try {
    // Launch both GUIs while Core still declares starting; neither may initialize a second Core.
    // Core 仍声明 starting 时启动两个 GUI；它们都不得初始化第二份 Core。
    let startingWindows = null;
    // launchGui opens one isolated desktop client against the selected Core URL.
    // launchGui 打开一个连接到选定 Core 地址的隔离桌面客户端。
    // name selects its private profile and baseUrl is the exact Core endpoint; returns the Electron application.
    // name 选择私有画像，baseUrl 是精确 Core 端点；返回 Electron 应用。
    const launchGui = (name, baseUrl) => electron.launch({ executablePath: selected['desktop-runtime'],
      args: [selected['desktop-entry'], `--service-executable=${selected['service-exe']}`,
        `--test-root=${testRoot}`, `--webui-url=${baseUrl}`,
        `--user-data-dir=${path.join(testRoot, name)}`], timeout: 60_000 });
    if (selected.suite === 'startup') {
      const startingDiscovery = await until(async () => {
        const discovery = JSON.parse(await readFile(path.join(testRoot, 'user-vulcan/code/webui-service.json'), 'utf8'));
        const response = await fetch(`${discovery.baseUrl}/v1/service/status`, {
          headers: { 'x-vulcan-service-token': discovery.serviceToken },
          signal: AbortSignal.timeout(3_000),
        });
        assert.equal(response.status, 200);
        assert.equal((await response.json()).lifecycle, 'starting');
        return discovery;
      });
      first = await launchGui('gui-first', startingDiscovery.baseUrl);
      second = await launchGui('gui-second', startingDiscovery.baseUrl);
      // startupPage waits for one GUI's native startup view before Core becomes ready.
      // startupPage 在 Core 就绪前等待某个 GUI 的原生启动视图。
      // application is the Electron owner; returns the startup URL after it appears.
      // application 是 Electron 所有者；返回出现后的启动视图地址。
      const startupPage = (application) => until(async () => {
        const window = application.windows()[0];
        assert.ok(window?.url().startsWith('file:') && new URL(window.url()).pathname.endsWith('/desktop-startup.html'),
          'GUI did not show its startup page.');
        return window.url();
      }, 5_000);
      startingWindows = await Promise.all([startupPage(first), startupPage(second)]);
    }
    const { discovery, status } = await until(async () => {
      assert.equal(service.exitCode ?? service.signalCode, null,
        `Service exited early: ${diagnostics.join('').slice(-1200)}`);
      const result = await liveDiscovery(testRoot);
      assert.equal(result.status.pid, service.pid);
      return result;
    }, 120_000);
    record.cases.push({ id: 'STARTUP_CORE_READY', status: 'passed', detail: { pid: service.pid,
      lifecycle: status.lifecycle } });

    // Launch the workbench against this explicit Core URL; debug Vite is verified separately.
    // 工作台连接此精确 Core URL；debug Vite 的验证另行执行。
    if (!first) first = await launchGui('gui-first', discovery.baseUrl);
    // Validate the native notification owner of the exact executable used by this GUI suite.
    // 校验此 GUI 验收所用精确可执行文件的原生通知归属身份。
    if (process.platform === 'win32') {
      const identity = await first.evaluate(({ app }) => ({
        name: app.getName(),
        activatorClsid: app.toastActivatorCLSID,
      }));
      const releaseGui = path.basename(selected['desktop-runtime']).toLowerCase() === 'vulcancodedesktop.exe';
      if (releaseGui) {
        assert.deepEqual(identity, {
          name: 'Vulcan Code Desktop',
          activatorClsid: '{AB3EF79F-A068-4BA3-9B40-814138499202}',
        });
      } else {
        // suffix proves the debug shortcut and AUMID belong to the same exact executable path.
        // suffix 证明调试快捷方式与 AUMID 属于同一个精确可执行程序路径。
        const suffix = identity.name.match(/^Vulcan Code Desktop Debug ([0-9a-f]{12})$/)?.[1];
        assert.ok(suffix);
        assert.match(identity.activatorClsid.toUpperCase(),
          /^\{[0-9A-F]{8}-[0-9A-F]{4}-5[0-9A-F]{3}-[89AB][0-9A-F]{3}-[0-9A-F]{12}\}$/);
      }
      record.cases.push({ id: 'STARTUP_GUI_IDENTITY', status: 'passed', detail: identity });
    }
    // Preserve the child handle before Playwright drops its dispatcher when the GUI closes.
    // 在 GUI 关闭导致 Playwright 丢弃分发器前保留子进程句柄。
    const firstProcess = first.process();
    const page = await until(async () => {
      const current = first.windows()[0];
      assert.ok(current);
      await current.locator('.workspace-sidebar-feature-button').first().waitFor({ timeout: 1000 });
      return current;
    });
    const connection = await page.evaluate(() => window.vulcanDesktop.service.getConnection());
    assert.equal(connection.pid, service.pid);
    record.os.deviceScaleFactor = await first.evaluate(({ screen }) => screen.getPrimaryDisplay().scaleFactor);
    record.cases.push({ id: 'STARTUP_GUI_CONNECTS_CORE', status: 'passed', detail: { pid: connection.pid,
      pageUrl: page.url() } });
    if (selected.suite === 'startup') {
      if (!second) second = await launchGui('gui-second', discovery.baseUrl);
      const secondPage = await until(async () => {
        const current = second.windows()[0];
        assert.ok(current);
        await current.locator('.workspace-sidebar-feature-button').first().waitFor({ timeout: 1000 });
        return current;
      });
      const reused = await secondPage.evaluate(() => window.vulcanDesktop.service.getConnection());
      assert.equal(reused.pid, service.pid);
      record.cases.push({ id: 'STARTUP_SECOND_GUI_REUSES_CORE', status: 'passed', detail: {
        firstPid: connection.pid, secondPid: reused.pid } });
      record.cases.push({ id: 'S03_STARTING_GUI', status: 'passed', detail: {
        startingWindowUrls: startingWindows.map((url) => url.split('?')[0]),
        firstPid: connection.pid, secondPid: reused.pid, corePid: service.pid,
      } });
    } else if (selected.suite === 'capacity') {
      // Keep this resource test invisible and avoid replaying native window gestures.
      // 此资源测试保持窗口隐藏，不重复原生窗口手势。
      await first.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].hide());
      // opened stores the exact GUI-local tabs created by this isolated test.
      // opened 保存此隔离测试创建的精确 GUI 本地标签。
      await page.evaluate(() => window.vulcanDesktop.localBrowser.setOwner('capacity-conversation'));
      const opened = [];
      for (let index = 0; index < 16; index += 1) {
        opened.push(await page.evaluate(() => window.vulcanDesktop.localBrowser.open('about:blank')));
      }
      // countNativeViews observes the actual Electron child-view count, not just UI metadata.
      // countNativeViews 读取实际 Electron 子视图数量，而非仅检查 UI 元数据。
      // It returns the selected window's current native child count.
      // 返回所选窗口当前的原生子视图数量。
      const countNativeViews = () => first.evaluate(({ BrowserWindow }) =>
        BrowserWindow.getAllWindows()[0].contentView.children.length);
      assert.equal((await page.evaluate(() => window.vulcanDesktop.localBrowser.snapshot())).tabs.length, 16);
      assert.equal(await countNativeViews(), 16);
      await assert.rejects(page.evaluate(() => window.vulcanDesktop.localBrowser.open('about:blank')),
        /tab limit/);
      assert.equal(await countNativeViews(), 16);
      await page.evaluate((id) => window.vulcanDesktop.localBrowser.close(id), opened[0]);
      // replacement proves closing one tab restores exactly one usable native-view slot.
      // replacement 证明关闭一个标签后恢复恰好一个可用原生视图槽位。
      const replacement = await page.evaluate(() => window.vulcanDesktop.localBrowser.open('about:blank'));
      assert.equal(await countNativeViews(), 16);
      for (const id of [...opened.slice(1), replacement]) {
        await page.evaluate((tabId) => window.vulcanDesktop.localBrowser.close(tabId), id);
      }
      assert.equal(await countNativeViews(), 0);
      // A second conversation in the same GUI sees an empty local list and cannot select A's tab.
      // 同一 GUI 内的第二会话只看到自己的空列表，也不能选择 A 的标签。
      const conversationATab = await page.evaluate(() => window.vulcanDesktop.localBrowser.open('about:blank'));
      // A second GUI attached to the same Core and conversation still starts with no native tabs.
      // 第二个 GUI 即使连接同一 Core、同一会话，原生标签也必须从空列表开始。
      second = await launchGui('gui-second', discovery.baseUrl);
      const secondPage = await until(async () => {
        const current = second.windows()[0];
        assert.ok(current);
        await current.locator('.workspace-sidebar-feature-button').first().waitFor({ timeout: 1000 });
        return current;
      });
      await second.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].hide());
      const secondWindowEmpty = await secondPage.evaluate(() => window.vulcanDesktop.localBrowser.setOwner('capacity-conversation'));
      assert.equal(secondWindowEmpty.ownerSessionId, 'capacity-conversation');
      assert.deepEqual(secondWindowEmpty.tabs, []);
      const secondWindowTab = await secondPage.evaluate(() => window.vulcanDesktop.localBrowser.open('about:blank'));
      assert.deepEqual((await page.evaluate(() => window.vulcanDesktop.localBrowser.snapshot())).tabs.map((tab) => tab.id), [conversationATab]);
      assert.deepEqual((await secondPage.evaluate(() => window.vulcanDesktop.localBrowser.snapshot())).tabs.map((tab) => tab.id), [secondWindowTab]);
      await secondPage.evaluate((id) => window.vulcanDesktop.localBrowser.close(id), secondWindowTab);
      const conversationBEmpty = await page.evaluate(() => window.vulcanDesktop.localBrowser.setOwner('capacity-conversation-b'));
      assert.equal(conversationBEmpty.ownerSessionId, 'capacity-conversation-b');
      assert.deepEqual(conversationBEmpty.tabs, []);
      await assert.rejects(page.evaluate((id) => window.vulcanDesktop.localBrowser.select(id), conversationATab),
        /another conversation/);
      const conversationBTab = await page.evaluate(() => window.vulcanDesktop.localBrowser.open('about:blank'));
      assert.deepEqual((await page.evaluate(() => window.vulcanDesktop.localBrowser.snapshot())).tabs.map((tab) => tab.id), [conversationBTab]);
      // A conversation's website cookie must not reach another conversation in the same GUI.
      // 同一 GUI 中一个会话的网页 Cookie 不得进入另一个会话。
      const isolatedStorage = await first.evaluate(async ({ BrowserWindow }) => {
        const views = BrowserWindow.getAllWindows()[0].contentView.children;
        if (views.length !== 2) throw new Error('Expected one native tab for each conversation.');
        const origin = 'https://local-browser-isolation.invalid/';
        await views[0].webContents.session.cookies.set({ url: origin, name: 'owner', value: 'first' });
        const firstCookies = await views[0].webContents.session.cookies.get({ url: origin });
        const secondCookies = await views[1].webContents.session.cookies.get({ url: origin });
        await views[0].webContents.session.cookies.remove(origin, 'owner');
        return { first: firstCookies.some((cookie) => cookie.value === 'first'), second: secondCookies.length };
      });
      assert.deepEqual(isolatedStorage, { first: true, second: 0 });
      assert.deepEqual((await page.evaluate(() => window.vulcanDesktop.localBrowser.setOwner('capacity-conversation'))).tabs.map((tab) => tab.id), [conversationATab]);
      await page.evaluate((id) => window.vulcanDesktop.localBrowser.close(id), conversationATab);
      await page.evaluate(() => window.vulcanDesktop.localBrowser.setOwner('capacity-conversation-b'));
      await page.evaluate((id) => window.vulcanDesktop.localBrowser.close(id), conversationBTab);
      assert.equal(await countNativeViews(), 0);
      record.cases.push({ id: 'LOCAL_BROWSER_CAPACITY', status: 'passed', detail: {
        limit: 16, rejectedOverflow: true, slotReused: true, conversationIsolation: true, guiWindowIsolation: true,
        conversationStorageIsolation: true,
        nativeViewsAfterCleanup: 0,
      } });
    } else {
      const windowState = () => first.evaluate(({ BrowserWindow }) => ({
        maximized: BrowserWindow.getAllWindows()[0].isMaximized(),
        minimized: BrowserWindow.getAllWindows()[0].isMinimized(),
      }));
      await page.getByRole('button', { name: /^(最大化窗口|Maximize window)$/ }).click();
      await until(async () => assert.equal((await windowState()).maximized, true));
      record.cases.push({ id: 'G02', status: 'passed', detail: 'DOM control maximized the native window' });
      await page.getByRole('button', { name: /^(还原窗口|Restore window)$/ }).click();
      await until(async () => assert.equal((await windowState()).maximized, false));
      record.cases.push({ id: 'G03', status: 'passed', detail: 'DOM control restored the native window' });
      await page.getByRole('button', { name: /^(最小化窗口|Minimize window)$/ }).click();
      await until(async () => assert.equal((await windowState()).minimized, true));
      record.cases.push({ id: 'G05', status: 'passed', detail: 'DOM control minimized the native window' });
      await first.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].restore());
      await page.getByRole('button', { name: /^(关闭窗口|Close window)$/ }).click();
      await until(async () => assert.notEqual(firstProcess.exitCode, null));
      record.cases.push({ id: 'G06', status: 'passed', detail: 'GUI closed while Core stayed alive' });
    }
    assert.equal((await liveDiscovery(testRoot)).status.pid, service.pid);
    record.cases.push({ id: 'CORE_SURVIVES_GUI', status: 'passed', detail: { pid: service.pid } });
  } finally {
    if (second) await second.close().catch(() => {});
    if (first) await first.close().catch(() => {});
    if (service.exitCode === null && service.signalCode === null) {
      service.kill();
      await new Promise((resolve) => { service.once('exit', resolve); setTimeout(resolve, 5_000); });
      if (service.exitCode === null && service.signalCode === null) {
        if (process.platform === 'win32') {
          await execFileAsync('rtk', ['proxy', 'taskkill', '/PID', String(service.pid), '/T', '/F'])
            .catch(() => {});
        } else {
          service.kill('SIGKILL');
        }
        await until(async () => assert.ok(service.exitCode !== null || service.signalCode !== null), 5_000);
      }
    }
  }
}

// Verify that an unwritable discovery target aborts before any ready snapshot is published.
// 验证发现目标不可写时启动终止，且不会发布虚假的就绪快照。
// selected identifies the test-support service; runDirectory owns all disposable fault state.
// selected 指向带测试能力的服务程序；runDirectory 拥有全部可清理的故障状态。
async function runDiscoveryFailureCase(selected, runDirectory, record) {
  const faultRoot = path.join(runDirectory, 'fault-discovery');
  const discovery = path.join(faultRoot, 'user-vulcan', 'code', 'webui-service.json');
  await mkdir(discovery, { recursive: true });
  try {
    const env = selected['asset-root']
      ? { ...process.env, VULCAN_CODE_OUTPUT_ROOT: selected['asset-root'] } : process.env;
    const result = await runChild(selected['service-exe'],
      ['--headless', `--test-root=${faultRoot}`], env, 45_000);
    await writeFile(path.join(runDirectory, 'fault-discovery.stdout.log'), result.stdout);
    await writeFile(path.join(runDirectory, 'fault-discovery.stderr.log'), result.stderr);
    record.evidence.push('fault-discovery.stdout.log', 'fault-discovery.stderr.log');
    assert.equal(result.timedOut, false, 'Discovery publication failure must terminate promptly.');
    assert.equal(result.code, 1, `Unexpected service exit: ${result.stderr.slice(-800)}`);
    assert.match(result.stderr, /HTTP bridge failed|headless startup failed/);
    record.cases.push({ id: 'S05', status: 'passed', detail: {
      exitCode: result.code, discoveryTarget: 'directory, not a writable JSON file' } });
  } finally {
    const realFault = await realpath(faultRoot);
    assert.equal(path.dirname(realFault), await realpath(runDirectory));
    await rm(realFault, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  }
}

// Kill the test-only HTTP listener after acknowledgement and verify its owner does not stay ready.
// 在测试专用监听器确认后使其故障，验证所有者不会继续宣称就绪。
// selected identifies the test-support service; runDirectory owns the isolated fault data.
// selected 指向带测试能力的服务程序；runDirectory 拥有隔离的故障数据。
async function runHttpBridgeFailureCase(selected, runDirectory, record) {
  const faultRoot = path.join(runDirectory, 'fault-http-bridge');
  await mkdir(faultRoot);
  try {
    const env = { ...process.env, VULCAN_TEST_HTTP_BRIDGE_FAIL_AFTER_ACK: '1' };
    if (selected['asset-root']) env.VULCAN_CODE_OUTPUT_ROOT = selected['asset-root'];
    const result = await runChild(selected['service-exe'],
      ['--headless', `--test-root=${faultRoot}`], env, 60_000);
    await writeFile(path.join(runDirectory, 'fault-http-bridge.stdout.log'), result.stdout);
    await writeFile(path.join(runDirectory, 'fault-http-bridge.stderr.log'), result.stderr);
    record.evidence.push('fault-http-bridge.stdout.log', 'fault-http-bridge.stderr.log');
    assert.equal(result.timedOut, false, 'Failed HTTP owner must terminate promptly.');
    assert.equal(result.code, 1, `Unexpected service exit: ${result.stderr.slice(-800)}`);
    assert.match(result.stderr, /injected post-acknowledgement HTTP bridge failure/);
    const discovery = JSON.parse(await readFile(
      path.join(faultRoot, 'user-vulcan', 'code', 'webui-service.json'), 'utf8'));
    assert.equal(discovery.status, 'failed');
    record.cases.push({ id: 'S10_HEADLESS', status: 'passed', detail: {
      exitCode: result.code, finalDiscoveryStatus: discovery.status } });
  } finally {
    const realFault = await realpath(faultRoot);
    assert.equal(path.dirname(realFault), await realpath(runDirectory));
    await rm(realFault, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  }
}

// Deliver a real Windows console signal to an isolated headless Core and verify its exit.
// 向隔离无桌面 Core 投递真实 Windows 控制台信号并验证退出。
// selected supplies the exact binary and optional assets; retryFailure selects a one-shot fault.
// selected 提供精确二进制及可选资源；retryFailure 选择一次性故障。
async function runWindowsHeadlessSignalCase(selected, runDirectory, record, retryFailure = false) {
  if (process.platform !== 'win32') return;
  const args = ['proxy', 'pwsh', '-NoProfile', '-File',
    path.join(desktop, 'tools/w10-headless-windows-signal.ps1'),
    '-ServiceExecutable', selected['service-exe']];
  if (selected['asset-root']) args.push('-AssetRoot', selected['asset-root']);
  if (retryFailure) args.push('-ExpectFirstShutdownFailure');
  const result = await runChild('rtk', args, process.env, 120_000);
  const prefix = retryFailure ? 'headless-retry' : 'headless-signal';
  await writeFile(path.join(runDirectory, `${prefix}.stdout.log`), result.stdout);
  await writeFile(path.join(runDirectory, `${prefix}.stderr.log`), result.stderr);
  record.evidence.push(`${prefix}.stdout.log`, `${prefix}.stderr.log`);
  assert.equal(result.timedOut, false, 'Windows headless signal case timed out.');
  assert.equal(result.code, 0, `Windows signal case failed: ${result.stderr.slice(-800)}`);
  assert.match(result.stdout, new RegExp(
    `HEADLESS_SIGNAL_PASSED pid=\\d+ exit=0 portClosed=True discoveryRemoved=True firstFailureRetained=${retryFailure ? 'True' : 'False'}`));
  record.cases.push({ id: retryFailure ? 'S09_HEADLESS_RETRY' : 'S12_WINDOWS', status: 'passed', detail: {
    signal: 'Ctrl-C', exitCode: 0, portClosed: true, discoveryRemoved: true,
    firstFailureRetained: retryFailure } });
}

// Exercise a real isolated Windows tray after normal browser use or an HTTP bridge failure.
// 在正常浏览器使用后或 HTTP 桥故障后验收真实隔离的 Windows 托盘。
// selected supplies the debug service, asset root and layout; the flag selects the failure path.
// selected 提供调试服务、资源根和清单；标志选择故障路径。
async function runWindowsTrayExitCase(selected, runDirectory, record, injectBridgeFailure = false) {
  if (process.platform !== 'win32' || !selected['runtime-layout']) return;
  assert.ok(selected['asset-root'], 'Tray exit requires an explicit asset root.');
  const args = ['proxy', 'pwsh', '-NoProfile', '-File',
    path.join(desktop, 'tools/w11-tray-exit-windows.ps1'),
    '-ServiceExecutable', selected['service-exe'],
    '-RuntimeLayout', selected['runtime-layout'],
    '-AssetRoot', selected['asset-root']];
  if (injectBridgeFailure) args.push('-InjectBridgeFailure');
  const result = await runChild('rtk', args, process.env, 120_000);
  const prefix = injectBridgeFailure ? 'tray-bridge-failure' : 'tray-exit';
  await Promise.all([
    writeFile(path.join(runDirectory, `${prefix}.stdout.log`), result.stdout),
    writeFile(path.join(runDirectory, `${prefix}.stderr.log`), result.stderr),
  ]);
  record.evidence.push(`${prefix}.stdout.log`, `${prefix}.stderr.log`);
  assert.equal(result.timedOut, false, 'Windows tray exit case timed out.');
  assert.equal(result.code, 0, `Windows tray exit failed: ${result.stderr.slice(-800)}`);
  const detail = JSON.parse(result.stdout.trim().split(/\r?\n/).at(-1));
  assert.equal(detail.result, 'passed');
  assert.equal(detail.discoveryRemoved, true);
  assert.equal(detail.portClosed, true);
  assert.equal(detail.bridgeFailureShown, injectBridgeFailure);
  record.cases.push({ id: injectBridgeFailure ? 'S10_TRAY' : 'S08', status: 'passed', detail });
}

// Derive a packaged release root only after the three explicit paths match its manifest.
// 仅在三个显式路径与清单一致后推导打包发布根目录。
// selected contains exact artifact inputs; returns the validated release root.
// selected 包含精确产物输入；返回已验证的发布根目录。
async function packagedRoot(selected) {
  const root = path.dirname(path.dirname(selected['service-exe']));
  const layout = JSON.parse(await readFile(path.join(root, 'runtime-layout.json'), 'utf8'));
  assert.equal(layout.profile, 'release');
  for (const [key, selectedKey] of [
    ['serviceExecutable', 'service-exe'], ['desktopExecutable', 'desktop-runtime'],
    ['desktopEntry', 'desktop-entry'],
  ]) assert.equal(path.resolve(root, layout[key]), path.resolve(selected[selectedKey]),
    `Explicit ${selectedKey} differs from the release manifest.`);
  return root;
}

// Run a browser or release acceptance subprocess and retain its complete external evidence.
// 执行浏览器或发行验收子进程，并保留其完整外部证据。
// selected is the validated CLI selection; runDirectory holds logs and test data; record receives cases.
// selected 是已核验的命令选择；runDirectory 保存日志与测试数据；record 接收用例。
async function runDelegatedSuite(selected, runDirectory, record) {
  let args;
  let env = { ...process.env };
  if (selected.suite === 'browser') {
    assert.ok(selected['asset-root'], 'Browser suite requires an explicit --asset-root.');
    const testRoot = await mkdtemp(path.join(runDirectory, 'vulcan-w09-ui-'));
    record.testRoot = testRoot;
    env = { ...env,
      VULCAN_W09_ASSET_ROOT: selected['asset-root'], VULCAN_W09_TEST_ROOT: testRoot,
      VULCAN_W09_EVIDENCE_DIR: runDirectory,
      VULCAN_W09_SERVICE_EXE: selected['service-exe'],
      VULCAN_W09_DESKTOP_RUNTIME: selected['desktop-runtime'],
      VULCAN_W09_DESKTOP_ENTRY: selected['desktop-entry'],
    };
    args = [path.join(desktop, 'tools/w09-browser-ui-acceptance.mjs')];
  } else {
    const root = await packagedRoot(selected);
    args = [path.join(desktop, 'tools/w10-detached-release-acceptance.mjs'), root];
    // An existing Windows toast registration belongs to its current owner; an explicit opt-out
    // 现有 Windows 通知注册属于当前持有者；显式跳过时仍验收发行包的其他能力。
    if (process.platform === 'win32' && env.VULCAN_W10_TEST_NOTIFICATION !== '0') {
      env.VULCAN_W10_TEST_NOTIFICATION = '1';
    }
  }
  // The browser group includes two actual model turns, a deliberately stalled navigation,
  // and a durable Stop; keep its bounded deadline distinct from the model-free release group.
  // 浏览器组包含两个真实模型回合、一次故意挂起的导航及可靠 Stop；
  // 因此使用区别于无模型发行组的有界期限。
  const result = await runChild(process.execPath, args, env,
    selected.suite === 'browser' ? 480_000 : 300_000);
  await Promise.all([
    writeFile(path.join(runDirectory, 'suite.stdout.log'), result.stdout),
    writeFile(path.join(runDirectory, 'suite.stderr.log'), result.stderr),
  ]);
  record.evidence.push('suite.stdout.log', 'suite.stderr.log');
  const jsonLine = result.stdout.trim().split(/\r?\n/).reverse().find((line) => line.startsWith('{'));
  const detail = jsonLine ? JSON.parse(jsonLine) : { exitCode: result.code, signal: result.signal };
  if (selected.suite === 'browser') {
    record.os.deviceScaleFactor = detail.deviceScale?.rendererPixelRatio ?? null;
  } else {
    record.os.deviceScaleFactor = detail.nativeInspection?.window?.displays?.map((display) =>
      display.scaleFactor) ?? null;
  }
  const passed = !result.timedOut && result.code === 0 && detail.result === 'passed';
  record.cases.push({ id: selected.suite === 'browser' ? 'W09_BROWSER_INTEGRATION' : 'W10_RELEASE_INTEGRATION',
    status: passed ? 'passed' : 'failed', detail });
  if (!passed) throw new Error(`${selected.suite} suite failed with exit ${result.code}`);
}

// Run one selected suite and write the manifest even when an assertion or child fails.
// 执行一组所选验收，并在断言或子进程失败时仍写入清单。
// No parameters; process arguments provide the selection and the exit code reflects the result.
// 无参数；进程参数提供选择，退出码反映执行结果。
async function main() {
  const selected = parseArguments(process.argv.slice(2));
  for (const key of ['service-exe', 'desktop-runtime', 'desktop-entry']) await access(selected[key]);
  if (selected['asset-root']) await access(path.join(selected['asset-root'], 'runtime-layout.json'));
  if (selected['runtime-layout']) await access(selected['runtime-layout']);
  await mkdir(selected['evidence-dir'], { recursive: true });
  const runDirectory = await mkdtemp(path.join(selected['evidence-dir'], 'vulcan-integration-'));
  const testRoot = path.join(runDirectory, 'state');
  await mkdir(testRoot);
  const [head, dirty, rustc] = await Promise.all([
    execFileAsync('rtk', ['proxy', 'git', 'rev-parse', 'HEAD'], { cwd: repository }),
    execFileAsync('rtk', ['proxy', 'git', 'status', '--porcelain=v1', '-z'], { cwd: repository }),
    execFileAsync('rtk', ['proxy', 'rustc', '--version'], { cwd: repository }),
  ]);
  const desktopPackage = JSON.parse(await readFile(path.join(desktop, 'package.json'), 'utf8'));
  const record = { schemaVersion: 1, runId: randomUUID(), suite: selected.suite,
    startedAt: new Date().toISOString(), gitHead: head.stdout.trim(),
    dirtyFiles: dirty.stdout.split('\0').filter(Boolean),
    dependencies: { electron: desktopPackage.devDependencies.electron,
      playwrightCore: desktopPackage.devDependencies['playwright-core'],
      node: process.version, rustc: rustc.stdout.trim() },
    os: { platform: platform(), release: release(), arch: arch(), deviceScaleFactor: null },
    testRoot, arguments: { serviceExecutable: selected['service-exe'],
      desktopRuntime: selected['desktop-runtime'], desktopEntry: selected['desktop-entry'],
      assetRoot: selected['asset-root'] ?? null,
      runtimeLayout: selected['runtime-layout'] ?? null }, cases: [], errors: [], evidence: [],
  };
  try {
    if (selected.suite === 'startup' || selected.suite === 'window' || selected.suite === 'capacity') {
      await runDesktopSuite(selected, testRoot, record);
      if (selected.suite === 'startup') {
        await runDiscoveryFailureCase(selected, runDirectory, record);
        await runHttpBridgeFailureCase(selected, runDirectory, record);
        await runWindowsHeadlessSignalCase(selected, runDirectory, record);
        await runWindowsHeadlessSignalCase(selected, runDirectory, record, true);
        await runWindowsTrayExitCase(selected, runDirectory, record);
        await runWindowsTrayExitCase(selected, runDirectory, record, true);
      }
    } else {
      await runDelegatedSuite(selected, runDirectory, record);
    }
  } catch (error) {
    record.errors.push(String(error?.stack ?? error));
    if (!record.cases.some((item) => item.status === 'failed')) {
      record.cases.push({ id: `${selected.suite.toUpperCase()}_SUITE`, status: 'failed',
        detail: String(error?.message ?? error) });
    }
    process.exitCode = 1;
  } finally {
    record.finishedAt = new Date().toISOString();
    // This exact generated state directory is a direct child of the caller-named evidence run.
    // 此精确生成的状态目录是调用者指定证据运行目录的直接子目录。
    const realRun = await realpath(runDirectory);
    const realState = await realpath(testRoot);
    assert.equal(path.dirname(realState), realRun);
    await rm(realState, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
    if (selected.suite === 'browser' && record.testRoot !== testRoot) {
      try {
        const realBrowserRoot = await realpath(record.testRoot);
        assert.equal(path.dirname(realBrowserRoot), realRun);
        await rm(realBrowserRoot, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
      } catch (error) {
        if (error?.code !== 'ENOENT') throw error;
      }
    }
    await writeFile(path.join(runDirectory, 'manifest.json'), JSON.stringify(record, null, 2) + '\n');
    console.log(JSON.stringify({ result: process.exitCode ? 'failed' : 'passed',
      manifest: path.join(runDirectory, 'manifest.json'), suite: selected.suite,
      cases: record.cases.map(({ id, status }) => ({ id, status })) }));
  }
}

await main();
