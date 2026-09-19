// Verify that a copied release distribution runs without reading repository sources or user data.
// 验证复制后的发行包无需读取仓库源码或真实用户数据即可运行。
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { access, cp, mkdir, mkdtemp, readFile, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { _electron as electron } from 'playwright-core';

// sourceRoot is used only by this test harness to identify the package input.
// sourceRoot 仅由测试框架用于定位发行包输入。
const sourceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
// packageRoot is the explicit, already packaged distribution to copy away from the repository.
// packageRoot 是显式指定、已打包且将被复制到仓库外的发行目录。
const packageRoot = path.resolve(process.argv[2] ?? path.join(sourceRoot, 'target/w10-release-distribution'));
// tempParent is the resolved operating-system temporary directory that owns the disposable copy.
// tempParent 是持有可丢弃副本的操作系统临时目录的解析路径。
const tempParent = await realpath(tmpdir());
// testRoot is a newly created directory used only by this test process.
// testRoot 是仅供本测试进程使用的新建目录。
const testRoot = await mkdtemp(path.join(tempParent, 'vulcan-w10-detached-'));
// distribution is the copied package, deliberately outside the repository source tree.
// distribution 是故意放在仓库源码树外的发行包副本。
const distribution = path.join(testRoot, 'distribution');
// stateRoot isolates release service and GUI state from the user's real profile.
// stateRoot 将发行服务和图形界面状态与用户真实配置隔离。
const stateRoot = path.join(testRoot, 'state');
// service retains the child process for deterministic shutdown.
// service 保留子进程句柄以便确定性关闭。
let service = null;
// gui retains the packaged Electron window for deterministic shutdown.
// gui 保留发行包 Electron 窗口以便确定性关闭。
let gui = null;
// activeTestNotificationId limits failure cleanup to this run's exact native toast.
// activeTestNotificationId 将失败清理限制在本次运行的精确原生通知。
let activeTestNotificationId = null;
// diagnostics capture startup errors without printing service credentials.
// diagnostics 捕获启动错误，但不打印服务凭据。
const diagnostics = [];
// execFileAsync runs only the exact native acceptance helper through the repository command proxy.
// execFileAsync 仅通过仓库命令代理运行精确的原生验收辅助程序。
const execFileAsync = promisify(execFile);

// Read Windows Action Center directly because Electron's Windows presenter leaves getHistory empty.
// 直接读取 Windows 通知中心，因为 Electron 的 Windows 呈现器未实现 getHistory。
// Returns exact native tags for the branded desktop AUMID.
// 返回品牌化桌面应用 ID 对应的精确原生标签。
async function windowsToastTags() {
  const command = '$manager=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];'
    + '$manager::History.GetHistory("ai.vulcan.code.desktop") | ForEach-Object { $_.Tag }';
  const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', command], { timeout: 10_000 });
  return stdout.split(/\r?\n/).map((tag) => tag.trim()).filter(Boolean);
}

// Read the exact Windows toast XML to confirm a body click carries its correlation id.
// 读取精确的 Windows 通知 XML，确认点击主体会携带关联 ID。
// notificationId selects only the generated toast from this run.
// notificationId 只选取本次运行生成的通知。
async function windowsToastXml(notificationId) {
  assert.match(notificationId, /^w10-notification-[0-9a-f-]{36}$/);
  const command = '$manager=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];'
    + `$manager::History.GetHistory("ai.vulcan.code.desktop") | Where-Object { $_.Tag -eq "${notificationId}" }`
    + ' | Select-Object -First 1 | ForEach-Object { $_.Content.GetXml() }';
  const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', command], { timeout: 10_000 });
  return stdout.trim();
}

// Remove only this test's exact toast from Windows Action Center after verification.
// 验证后只从 Windows 通知中心移除本次测试的精确通知。
// notificationId is a generated ASCII UUID prefixed for this test.
// notificationId 是带有本测试前缀的 ASCII UUID。
async function removeWindowsToast(notificationId) {
  assert.match(notificationId, /^w10-notification-[0-9a-f-]{36}$/);
  const command = '$manager=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];'
    + `$manager::History.Remove("${notificationId}","Notifications","ai.vulcan.code.desktop")`;
  await execFileAsync('powershell.exe', ['-NoProfile', '-Command', command], { timeout: 10_000 });
}

// Abort the opt-in toast probe when a real installed desktop identity is already active.
// 若已有真实安装的桌面身份在用，则在可选通知探测前终止，以免覆盖用户注册。
// A dead shortcut or CLSID left by an earlier isolated probe is safe to replace.
// 先前隔离探测遗留且目标已失效的快捷方式或 CLSID 可被替换。
async function assertNoActiveToastRegistration() {
  const command = '$p=Join-Path ([Environment]::GetFolderPath("Programs")) "Vulcan Code Desktop.lnk";'
    + 'if(Test-Path -LiteralPath $p){$t=(New-Object -ComObject WScript.Shell).CreateShortcut($p).TargetPath;'
    + 'if(Test-Path -LiteralPath $t){Write-Output $t}};'
    + '$k="Registry::HKEY_CURRENT_USER\\Software\\Classes\\CLSID\\{AB3EF79F-A068-4BA3-9B40-814138499202}\\LocalServer32";'
    + 'if(Test-Path -LiteralPath $k){$t=(Get-Item -LiteralPath $k).GetValue("");'
    + 'if(Test-Path -LiteralPath $t){Write-Output $t}}';
  const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', command], { timeout: 10_000 });
  assert.equal(stdout.trim(), '', `An active Windows toast registration would be overwritten: ${stdout.trim()}`);
}

// Remove only the shortcut and COM registration that still point to this deleted test copy.
// 只清除仍指向本次测试副本的快捷方式和 COM 注册。
// The exact distribution path is passed through the child environment, not interpolated into shell code.
// 精确发行路径通过子进程环境传入，不拼接进 Shell 代码。
async function cleanTestToastRegistration(distributionRoot) {
  const command = '$target=Join-Path $env:VULCAN_TEST_DISTRIBUTION "desktop\\runtime\\VulcanCodeDesktop.exe";'
    + '$p=Join-Path ([Environment]::GetFolderPath("Programs")) "Vulcan Code Desktop.lnk";'
    + 'if(Test-Path -LiteralPath $p){$actual=(New-Object -ComObject WScript.Shell).CreateShortcut($p).TargetPath;'
    + 'if($actual -eq $target){Remove-Item -LiteralPath $p}};'
    + '$k="Registry::HKEY_CURRENT_USER\\Software\\Classes\\CLSID\\{AB3EF79F-A068-4BA3-9B40-814138499202}";'
    + 'if(Test-Path -LiteralPath "$k\\LocalServer32"){$actual=(Get-Item -LiteralPath "$k\\LocalServer32").GetValue("");'
    + 'if($actual -eq $target){Remove-Item -LiteralPath $k -Recurse}}';
  await execFileAsync('powershell.exe', ['-NoProfile', '-Command', command], {
    env: { ...process.env, VULCAN_TEST_DISTRIBUTION: distributionRoot }, timeout: 10_000,
  });
}

// until retries a read-only observation for a bounded amount of time.
// until 在有界时间内重试只读观察。
// observe returns the accepted evidence and timeoutMs limits the wait.
// observe 返回已接受的证据，timeoutMs 限制等待时间。
async function until(observe, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (service && service.exitCode !== null) {
      throw new Error(`Release service exited ${service.exitCode}: ${diagnostics.join('').slice(-2500)}`);
    }
    try { return await observe(); } catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise((resolve) => setTimeout(resolve, 150));
    }
  }
}

try {
  await access(path.join(packageRoot, 'runtime-layout.json'));
  await cp(packageRoot, distribution, { recursive: true, dereference: true });
  await mkdir(stateRoot);
  // layout is resolved only from the detached copy and controls every child entry point.
  // layout 仅从脱离源码的副本解析，并控制每个子进程入口。
  const layout = JSON.parse(await readFile(path.join(distribution, 'runtime-layout.json'), 'utf8'));
  assert.equal(layout.profile, 'release');
  assert.ok(layout.capabilities.includes('browser-host'));
  // entry prevents an altered layout from escaping its detached package root.
  // entry 防止被改动的布局清单逃逸出独立发行目录。
  const entry = (relative) => {
    const resolved = path.resolve(distribution, relative);
    assert.ok(resolved.startsWith(distribution + path.sep), `Package path escaped: ${relative}`);
    return resolved;
  };
  // packageSourceEntries proves this copied runtime is not a second repository checkout.
  // packageSourceEntries 证明复制的运行时不是另一个仓库检出。
  const packageSourceEntries = await Promise.all(['Cargo.toml', 'apps', '.git'].map(async (name) => {
    try { await access(path.join(distribution, name)); return name; } catch { return null; }
  }));
  assert.deepEqual(packageSourceEntries.filter(Boolean), []);
  // serviceExecutable and desktopExecutable come from the copied manifest, not a source fallback.
  // serviceExecutable 与 desktopExecutable 来自复制后的清单，而非源码回退路径。
  const serviceExecutable = entry(layout.serviceExecutable);
  const desktopExecutable = entry(layout.desktopExecutable);
  if (process.env.VULCAN_W10_TEST_NOTIFICATION === '1') {
    await assertNoActiveToastRegistration();
  }
  await access(entry(layout.browserHostEntry));
  await access(entry(layout.chromiumExecutable));
  const environment = { ...process.env, VULCAN_RUNTIME_LAYOUT: path.join(distribution, 'runtime-layout.json'),
    VULCAN_CODE_OUTPUT_ROOT: distribution };
  service = spawn(serviceExecutable, ['--headless', `--test-root=${stateRoot}`], {
    cwd: distribution, env: environment, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  });
  service.stdout.on('data', (chunk) => diagnostics.push(chunk.toString()));
  service.stderr.on('data', (chunk) => diagnostics.push(chunk.toString()));
  // discovery identifies the exact isolated release service instance.
  // discovery 标识精确的隔离发行服务实例。
  const discovery = await until(async () => {
    const value = JSON.parse(await readFile(path.join(stateRoot, 'user-vulcan/code/webui-service.json'), 'utf8'));
    const response = await fetch(`${value.baseUrl}/v1/service/status`, {
      headers: { 'x-vulcan-service-token': value.serviceToken },
    });
    assert.equal(response.status, 200);
    const status = await response.json();
    assert.equal(status.lifecycle, 'ready');
    return value;
  });
  // index and one hashed asset prove static WebUI resources came from the copied package.
  // index 与一个带哈希的资源证明静态网页文件来自复制后的发行包。
  const indexResponse = await fetch(`${discovery.baseUrl}/`);
  assert.equal(indexResponse.status, 200);
  const indexHtml = await indexResponse.text();
  const assetPath = indexHtml.match(/(?:src|href)="(\/assets\/[^\"]+)"/)?.[1];
  assert.ok(assetPath, 'Detached WebUI index has no bundled asset.');
  const assetResponse = await fetch(`${discovery.baseUrl}${assetPath}`);
  assert.equal(assetResponse.status, 200);
  // rpc uses only this service's private token and a fixed test owner identity.
  // rpc 仅使用当前服务的私有令牌与固定测试所有者身份。
  const owner = randomUUID();
  async function rpc(method, params) {
    const response = await fetch(`${discovery.baseUrl}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
      body: JSON.stringify({ method, params: { ownerSessionId: owner, ...params } }),
    });
    assert.equal(response.status, 200, `${method}: HTTP ${response.status}`);
    const body = await response.json();
    assert.equal(body.ok, true, `${method}: ${JSON.stringify(body.error)}`);
    return body.result;
  }
  // componentResponse reads the Core-wide optional browser status without inventing a conversation.
  // componentResponse 不虚构会话身份，直接读取 Core 全局可选浏览器组件状态。
  const componentResponse = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method: 'browser.runtime.status', params: {} }),
  });
  // componentStatus must reflect the copied Chromium and host files in this detached package.
  // componentStatus 必须反映此脱离仓库发行包中复制的 Chromium 与宿主文件。
  const componentStatus = await componentResponse.json();
  assert.equal(componentResponse.status, 200);
  assert.equal(componentStatus.ok, true, JSON.stringify(componentStatus.error));
  assert.equal(componentStatus.result.ready, true);
  // browserSession must start the copied host and copied Chromium, not a source-owned executable.
  // browserSession 必须启动复制后的宿主与 Chromium，而非源码目录内的程序。
  const browserSession = await rpc('browser.session.create', {});
  assert.equal(browserSession.status, 'running');
  assert.equal(browserSession.hostState, 'running');
  const lease = await rpc('browser.lease.acquire', { sessionId: browserSession.sessionId,
    holder: 'w10-detached-release-acceptance' });
  await rpc('browser.session.close', { sessionId: browserSession.sessionId,
    leaseSecret: lease.leaseSecret });
  // gui opens the copied Electron entry while its Core remains an independent process.
  // gui 打开复制后的 Electron 入口，而 Core 保持独立进程。
  gui = await electron.launch({ executablePath: desktopExecutable, cwd: distribution,
    args: [entry(layout.desktopEntry), `--service-executable=${serviceExecutable}`,
      `--webui-url=${discovery.baseUrl}`, `--test-root=${stateRoot}`,
      `--user-data-dir=${path.join(stateRoot, 'gui')}`], timeout: 60_000 });
  const page = await until(async () => {
    const current = gui.windows()[0];
    assert.ok(current);
    await current.locator('.workspace-sidebar-feature-button').first().waitFor({ timeout: 1000 });
    return current;
  });
  const connection = await page.evaluate(() => window.vulcanDesktop.service.getConnection());
  assert.equal(connection.pid, service.pid);
  assert.ok(page.url().startsWith(discovery.baseUrl));
  // nativeInspection records candidate drag hit regions and real display scales before native input tests.
  // nativeInspection 在原生输入测试前记录可拖动命中区域与真实显示器缩放。
  const nativeInspection = {
    window: await gui.evaluate(({ BrowserWindow, screen }) => ({
      bounds: BrowserWindow.getAllWindows()[0].getBounds(),
      handle: BrowserWindow.getAllWindows()[0].getNativeWindowHandle().readBigUInt64LE(0).toString(),
      displays: screen.getAllDisplays().map((display) => ({
        bounds: display.bounds, scaleFactor: display.scaleFactor,
      })),
    })),
    dragRegions: await page.locator('[data-tauri-drag-region]').evaluateAll((elements) => elements
      .filter((element) => getComputedStyle(element).getPropertyValue('-webkit-app-region') === 'drag')
      .map((element) => ({ className: String(element.className).slice(0, 100),
        bounds: element.getBoundingClientRect().toJSON() })).slice(0, 16)),
  };
  // nativeGesture dispatches physical Windows mouse input only after the helper confirms this HWND.
  // nativeGesture 仅在辅助程序确认该窗口句柄后派发物理 Windows 鼠标输入。
  async function nativeGesture(action, from, to = from) {
    const { stdout } = await execFileAsync('rtk', ['proxy', 'pwsh', '-NoProfile', '-File',
      path.join(sourceRoot, 'apps/vulcan-code-desktop/tools/native-window-input.ps1'),
      '-WindowHandle', nativeInspection.window.handle, '-Action', action,
      '-FromX', String(Math.round(from.x)), '-FromY', String(Math.round(from.y)),
      '-ToX', String(Math.round(to.x)), '-ToY', String(Math.round(to.y))],
    { cwd: distribution, windowsHide: true, timeout: 15_000 });
    return stdout.trim();
  }
  // readWindowBounds returns the actual top-level window rectangle after native input.
  // readWindowBounds 在原生输入后返回真实顶层窗口矩形。
  async function readWindowBounds() {
    return gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].getBounds());
  }
  // brandRegion is a real Chromium draggable area in the copied workbench.
  // brandRegion 是发行版工作台中的真实 Chromium 可拖动区域。
  const brandRegion = nativeInspection.dragRegions.find((region) => region.className.includes('sidebar-brand-drag-region'));
  assert.ok(brandRegion, 'The workbench has no draggable brand region.');
  const originalBounds = nativeInspection.window.bounds;
  const brandPoint = { x: originalBounds.x + brandRegion.bounds.left + brandRegion.bounds.width / 2,
    y: originalBounds.y + brandRegion.bounds.top + brandRegion.bounds.height / 2 };
  // brandProbe records the native hit-test before moving the window so an input failure is
  // distinguishable from a CSS drag-region failure without guessing from unchanged bounds.
  // brandProbe 在移动窗口前记录原生命中测试，以便在边界未变化时区分输入失败与拖动区失败。
  const brandProbe = JSON.parse(await nativeGesture('Probe', brandPoint));
  await nativeGesture('Drag', brandPoint, { x: brandPoint.x + 80, y: brandPoint.y + 50 });
  const movedBounds = await until(async () => {
    const bounds = await readWindowBounds();
    assert.ok(bounds.x >= originalBounds.x + 50 && bounds.y >= originalBounds.y + 25,
      JSON.stringify({ originalBounds, bounds, brandPoint, brandRegion, brandProbe }));
    return bounds;
  }, 5_000);
  // nativeDragResults records changes caused by three distinct real drag hit regions.
  // nativeDragResults 记录三个不同真实拖动命中区域造成的窗口位置变化。
  const nativeDragResults = [{ region: 'brand', before: originalBounds, after: movedBounds }];
  for (const [name, className] of [['title', 'workspace-header-title-text'],
    ['console', 'console-header-drag-fill']]) {
    const region = nativeInspection.dragRegions.find((candidate) => candidate.className.includes(className));
    assert.ok(region, `The workbench has no draggable ${name} region.`);
    const before = await readWindowBounds();
    const point = { x: before.x + region.bounds.left + region.bounds.width / 2,
      y: before.y + region.bounds.top + region.bounds.height / 2 };
    await nativeGesture('Drag', point, { x: point.x + 75, y: point.y + 45 });
    const after = await until(async () => {
      const bounds = await readWindowBounds();
      assert.ok(bounds.x >= before.x + 45 && bounds.y >= before.y + 20,
        JSON.stringify({ name, before, bounds }));
      return bounds;
    }, 5_000);
    nativeDragResults.push({ region: name, before, after });
  }
  // A real feature page must retain its own draggable title region after navigation.
  // 实际功能页切换后也必须保留独立的标题拖动区域。
  await page.locator('.workspace-sidebar-feature-button').first().click();
  const featureRegion = await until(async () => {
    const region = await page.locator('.workspace-header-title-text[data-tauri-drag-region]').evaluate((element) => ({
      drag: getComputedStyle(element).getPropertyValue('-webkit-app-region'),
      bounds: element.getBoundingClientRect().toJSON(),
    }));
    assert.equal(region.drag, 'drag');
    return region;
  }, 5_000);
  const beforeFeatureDrag = await readWindowBounds();
  const featurePoint = { x: beforeFeatureDrag.x + featureRegion.bounds.left + featureRegion.bounds.width / 2,
    y: beforeFeatureDrag.y + featureRegion.bounds.top + featureRegion.bounds.height / 2 };
  await nativeGesture('Drag', featurePoint, { x: featurePoint.x + 75, y: featurePoint.y + 45 });
  const afterFeatureDrag = await until(async () => {
    const bounds = await readWindowBounds();
    assert.ok(bounds.x >= beforeFeatureDrag.x + 45 && bounds.y >= beforeFeatureDrag.y + 20,
      JSON.stringify({ beforeFeatureDrag, bounds }));
    return bounds;
  }, 5_000);
  nativeDragResults.push({ region: 'feature', before: beforeFeatureDrag, after: afterFeatureDrag });
  // Native double-clicks must maximize and restore the same top-level window exactly once.
  // 原生双击必须让同一个顶层窗口恰好最大化并还原一次。
  const beforeDoubleClick = await readWindowBounds();
  const doublePoint = { x: beforeDoubleClick.x + brandRegion.bounds.left + brandRegion.bounds.width / 2,
    y: beforeDoubleClick.y + brandRegion.bounds.top + brandRegion.bounds.height / 2 };
  await nativeGesture('DoubleClick', doublePoint);
  await until(async () => assert.equal(await gui.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].isMaximized()), true), 5_000);
  const maximizedBounds = await readWindowBounds();
  await nativeGesture('DoubleClick', { x: maximizedBounds.x + brandRegion.bounds.left + brandRegion.bounds.width / 2,
    y: maximizedBounds.y + brandRegion.bounds.top + brandRegion.bounds.height / 2 });
  await until(async () => assert.equal(await gui.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].isMaximized()), false), 5_000);
  const restoredBounds = await readWindowBounds();
  assert.deepEqual(restoredBounds, beforeDoubleClick);
  // A programmatic reset is only test setup; the measured resize below comes from native input.
  // 程序化复位仅用于准备测试；下方被测的尺寸变化来自原生输入。
  const resizeBaseline = { x: 1000, y: 220, width: 1400, height: 900 };
  // resizeCases cover the four native edges and four native corners with real pointer gestures.
  // resizeCases 使用真实鼠标手势覆盖四条原生边缘和四个原生角。
  const resizeCases = [
    { name: 'left', horizontal: 'left', vertical: null, hit: 10, dx: -110, dy: 0 },
    { name: 'right', horizontal: 'right', vertical: null, hit: 11, dx: 110, dy: 0 },
    { name: 'top', horizontal: null, vertical: 'top', hit: 12, dx: 0, dy: -90 },
    { name: 'bottom', horizontal: null, vertical: 'bottom', hit: 15, dx: 0, dy: 90 },
    { name: 'topLeft', horizontal: 'left', vertical: 'top', hit: 13, dx: -110, dy: -90 },
    { name: 'topRight', horizontal: 'right', vertical: 'top', hit: 14, dx: 110, dy: -90 },
    { name: 'bottomLeft', horizontal: 'left', vertical: 'bottom', hit: 16, dx: -110, dy: 90 },
    { name: 'bottomRight', horizontal: 'right', vertical: 'bottom', hit: 17, dx: 110, dy: 90 },
  ];
  // nativeResizeResults records hit-test codes and measured outer-window movement for every case.
  // nativeResizeResults 记录每种情形的命中测试代码和实测外窗变化。
  const nativeResizeResults = [];
  for (const resizeCase of resizeCases) {
    await gui.evaluate(({ BrowserWindow }, bounds) => BrowserWindow.getAllWindows()[0].setBounds(bounds), resizeBaseline);
    const before = await readWindowBounds();
    const outer = JSON.parse(await nativeGesture('Probe', {
      x: before.x + before.width - 4, y: before.y + before.height / 2,
    })).rect;
    const hitTests = [];
    let point = null;
    for (const inset of [1, 2, 4, 6, 8, 10]) {
      const candidate = {
        x: resizeCase.horizontal === 'left' ? outer.left + inset
          : resizeCase.horizontal === 'right' ? outer.right - inset : (outer.left + outer.right) / 2,
        y: resizeCase.vertical === 'top' ? outer.top + inset
          : resizeCase.vertical === 'bottom' ? outer.bottom - inset : (outer.top + outer.bottom) / 2,
      };
      try {
        const probe = JSON.parse(await nativeGesture('Probe', candidate));
        hitTests.push({ inset, hit: probe.hit });
        if (probe.hit === resizeCase.hit) { point = candidate; break; }
      } catch (error) {
        hitTests.push({ inset, error: String(error.message).slice(0, 120) });
      }
    }
    assert.ok(point, `No native ${resizeCase.name} resize border: ${JSON.stringify(hitTests)}`);
    await nativeGesture('Drag', point, { x: point.x + resizeCase.dx, y: point.y + resizeCase.dy });
    const expected = {
      x: before.x + (resizeCase.horizontal === 'left' ? resizeCase.dx : 0),
      y: before.y + (resizeCase.vertical === 'top' ? resizeCase.dy : 0),
      width: before.width + (resizeCase.horizontal === 'left' ? -resizeCase.dx : resizeCase.dx),
      height: before.height + (resizeCase.vertical === 'top' ? -resizeCase.dy : resizeCase.dy),
    };
    const after = await until(async () => {
      const bounds = await readWindowBounds();
      for (const key of ['x', 'y', 'width', 'height']) {
        assert.ok(Math.abs(bounds[key] - expected[key]) <= 35,
          `${resizeCase.name}: ${JSON.stringify({ before, expected, bounds })}`);
      }
      return bounds;
    }, 5_000);
    nativeResizeResults.push({ name: resizeCase.name, hit: resizeCase.hit, before, after, hitTests });
  }
  // Shrinking a real bottom-right corner must stop at the configured native minimum size.
  // 真实拖动右下角缩小时必须停在配置的原生最小尺寸。
  await gui.evaluate(({ BrowserWindow }, bounds) => BrowserWindow.getAllWindows()[0].setBounds(bounds), resizeBaseline);
  const minimumBefore = await readWindowBounds();
  const minimumOuter = JSON.parse(await nativeGesture('Probe', {
    x: minimumBefore.x + minimumBefore.width - 4,
    y: minimumBefore.y + minimumBefore.height / 2,
  })).rect;
  const minimumPoint = { x: minimumOuter.right - 1, y: minimumOuter.bottom - 1 };
  assert.equal(JSON.parse(await nativeGesture('Probe', minimumPoint)).hit, 17);
  await nativeGesture('Drag', minimumPoint, { x: minimumPoint.x - 700, y: minimumPoint.y - 500 });
  const minimumAfter = await readWindowBounds();
  assert.ok(minimumAfter.width >= 960 && minimumAfter.height >= 600,
    JSON.stringify({ minimumBefore, minimumAfter }));
  assert.ok(minimumAfter.width < minimumBefore.width && minimumAfter.height < minimumBefore.height,
    JSON.stringify({ minimumBefore, minimumAfter }));
  // Native dragging moves the same GUI onto the second monitor before maximize and restore.
  // 原生拖动把同一个 GUI 移到第二显示器，再验证最大化与还原。
  await gui.evaluate(({ BrowserWindow }, bounds) => BrowserWindow.getAllWindows()[0].setBounds(bounds), resizeBaseline);
  const beforeCrossMonitor = await readWindowBounds();
  const crossStart = { x: beforeCrossMonitor.x + brandRegion.bounds.left + brandRegion.bounds.width / 2,
    y: beforeCrossMonitor.y + brandRegion.bounds.top + brandRegion.bounds.height / 2 };
  await nativeGesture('Drag', crossStart, { x: crossStart.x + 3100, y: crossStart.y + 55 });
  const movedToSecondMonitor = await until(async () => {
    const bounds = await readWindowBounds();
    assert.ok(bounds.x >= 3440, JSON.stringify({ beforeCrossMonitor, bounds }));
    return bounds;
  }, 5_000);
  const secondMonitorPoint = { x: movedToSecondMonitor.x + brandRegion.bounds.left + brandRegion.bounds.width / 2,
    y: movedToSecondMonitor.y + brandRegion.bounds.top + brandRegion.bounds.height / 2 };
  await nativeGesture('DoubleClick', secondMonitorPoint);
  await until(async () => assert.equal(await gui.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].isMaximized()), true), 5_000);
  const secondMonitorMaximized = await readWindowBounds();
  assert.ok(secondMonitorMaximized.x >= 3430,
    JSON.stringify({ movedToSecondMonitor, secondMonitorMaximized }));
  await nativeGesture('DoubleClick', { x: secondMonitorMaximized.x + brandRegion.bounds.left + brandRegion.bounds.width / 2,
    y: secondMonitorMaximized.y + brandRegion.bounds.top + brandRegion.bounds.height / 2 });
  await until(async () => assert.equal(await gui.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].isMaximized()), false), 5_000);
  const secondMonitorRestored = await readWindowBounds();
  assert.deepEqual(secondMonitorRestored, movedToSecondMonitor);
  await gui.close();
  gui = null;
  const stillReady = await fetch(`${discovery.baseUrl}/v1/service/status`, {
    headers: { 'x-vulcan-service-token': discovery.serviceToken },
  });
  assert.equal(stillReady.status, 200);
  assert.equal((await stillReady.json()).lifecycle, 'ready');
  // A packaged shortcut needs no script path; the test-root argument keeps this probe isolated.
  // 打包后的快捷方式不需要脚本路径；测试根参数保证此次探测保持隔离。
  gui = await electron.launch({ executablePath: desktopExecutable, cwd: distribution,
    args: [`--test-root=${stateRoot}`, `--user-data-dir=${path.join(stateRoot, 'gui-relaunch')}`],
    timeout: 60_000 });
  const relaunchedPage = await until(async () => {
    const current = gui.windows()[0];
    assert.ok(current);
    await current.locator('.workspace-sidebar-feature-button').first().waitFor({ timeout: 1000 });
    return current;
  });
  const relaunchConnection = await relaunchedPage.evaluate(() => window.vulcanDesktop.service.getConnection());
  assert.equal(relaunchConnection.pid, service.pid);
  const notificationIdentity = await gui.evaluate(({ app }) => ({
    name: app.getName(), activatorClsid: app.toastActivatorCLSID,
  }));
  assert.equal(notificationIdentity.name, 'Vulcan Code Desktop');
  assert.equal(notificationIdentity.activatorClsid.toUpperCase(), '{AB3EF79F-A068-4BA3-9B40-814138499202}');
  // This opt-in probe sends one real Windows toast and removes only its own exact native id.
  // 此显式开启的探测发送一条真实 Windows 通知，并只移除自己的精确原生 ID。
  let notificationEvidence = null;
  if (process.env.VULCAN_W10_TEST_NOTIFICATION === '1') {
    const notificationId = `w10-notification-${randomUUID()}`;
    activeTestNotificationId = notificationId;
    const delivery = await relaunchedPage.evaluate(async (id) => {
      window.__w10NotificationActivations = [];
      window.__w10UnsubscribeNotification = await window.vulcanDesktop.desktop
        .subscribeNotificationActivation((event) => {
          window.__w10NotificationActivations.push(event.notificationId);
        });
      await window.vulcanDesktop.desktop.showNotification({
        id, title: 'Vulcan Code Desktop test', body: 'Release notification delivery check',
      });
      return 'shown';
    }, notificationId);
    assert.equal(delivery, 'shown');
    const nativeHistory = await until(async () => {
      const ids = await windowsToastTags();
      assert.ok(ids.includes(notificationId), JSON.stringify(ids));
      return ids;
    }, 10_000);
    const toastXml = await windowsToastXml(notificationId);
    assert.ok(toastXml.includes(`launch="type=click&amp;tag=${notificationId}"`), toastXml);
    notificationEvidence = {
      delivered: true, historyContainsExactId: nativeHistory.includes(notificationId),
      bodyClickCarriesExactId: true,
    };
    await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].minimize());
    await until(async () => {
      assert.equal(await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].isMinimized()), true);
      return true;
    }, 5_000);
    // A manual run waits for the exact toast body; regular automation keeps the COM probe.
    // 人工运行等待点击精确通知主体；常规自动化仍使用 COM 回调探测。
    const manualNotificationClick = process.env.VULCAN_W10_MANUAL_NOTIFICATION_CLICK === '1';
    if (manualNotificationClick) {
      console.log(`MANUAL_NOTIFICATION_READY id=${notificationId}`);
    } else {
      await execFileAsync('rtk', [
        'proxy', 'pwsh', '-NoProfile', '-File', path.join(sourceRoot, 'apps/vulcan-code-desktop/tools/windows-toast-activate.ps1'),
        '-NotificationId', notificationId,
      ], { timeout: 10_000 });
    }
    await until(async () => {
      const activations = await relaunchedPage.evaluate(() => window.__w10NotificationActivations);
      assert.deepEqual(activations, [notificationId]);
      assert.equal(await gui.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].isMinimized()), false);
      return true;
    }, manualNotificationClick ? 120_000 : 10_000);
    const postActivationConnection = await relaunchedPage.evaluate(() => window.vulcanDesktop.service.getConnection());
    assert.equal(postActivationConnection.pid, service.pid);
    notificationEvidence.nativeActivationRestoredGui = true;
    notificationEvidence.businessIdDelivered = true;
    notificationEvidence.physicalClickObserved = manualNotificationClick;
    await relaunchedPage.evaluate(() => window.__w10UnsubscribeNotification());
    if (process.env.VULCAN_W10_INSPECT_NOTIFICATION === '1') {
      console.log(`Notification inspection hold: ${notificationId}`);
      await new Promise((resolve) => setTimeout(resolve, 60_000));
    }
    await removeWindowsToast(notificationId);
    activeTestNotificationId = null;
  }
  await gui.close();
  gui = null;
  console.log(JSON.stringify({ result: 'passed', detachedRoot: distribution,
    profile: layout.profile, copiedWebui: true, copiedBrowserHost: true,
    copiedChromium: true, copiedGui: true, guiClosePreservedCore: true,
    noScriptPathGuiRelaunchReusedCore: true, notificationIdentity, notificationEvidence,
    coreBrowserComponentReady: componentStatus.result.ready,
    nativeDrag: nativeDragResults, nativeDoubleClick: { maximizedBounds, restoredBounds },
    nativeResize: nativeResizeResults, nativeMinimum: { before: minimumBefore, after: minimumAfter },
    crossMonitor: { before: beforeCrossMonitor, moved: movedToSecondMonitor,
      maximized: secondMonitorMaximized, restored: secondMonitorRestored },
    nativeInspection }));
} catch (error) {
  console.error(diagnostics.join('').slice(-3500));
  throw error;
} finally {
  if (gui) await gui.close().catch(() => {});
  if (service) {
    service.kill();
    await new Promise((resolve) => {
      if (service.exitCode !== null) resolve();
      else { service.once('exit', resolve); setTimeout(resolve, 5_000); }
    });
  }
  if (process.env.VULCAN_W10_TEST_NOTIFICATION === '1') {
    if (activeTestNotificationId !== null) {
      await removeWindowsToast(activeTestNotificationId).catch(() => {});
    }
    await cleanTestToastRegistration(distribution);
  }
  // The resolved delete target must be this test's own direct child of the OS temporary root.
  // 删除目标的解析路径必须是操作系统临时目录中本测试专属的直接子目录。
  const resolvedRoot = await realpath(testRoot);
  assert.equal(path.dirname(resolvedRoot), tempParent);
  assert.ok(path.basename(resolvedRoot).startsWith('vulcan-w10-detached-'));
  await rm(resolvedRoot, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
}
