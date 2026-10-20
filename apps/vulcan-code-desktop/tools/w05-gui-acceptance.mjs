// W05 desktop GUI acceptance harness for the mechanically observable G cases.
// W05 桌面 GUI 验收工具，覆盖可机械观察的 G 用例。
//
// The harness starts a real Core service and a real Electron GUI, then drives the GUI over CDP so
// that every assertion runs through the actual preload bridge and main-process handlers.
// Native drag, OS edge hit-testing, multi-monitor and display-scaling cases cannot be proven by
// DOM automation; those are recorded as requiring human sign-off instead of being faked.
// 工具启动真实 Core 服务与真实 Electron GUI，并通过 CDP 驱动 GUI，使每条断言都经过实际预加载桥与
// 主进程处理器。原生拖动、操作系统边缘命中、多显示器与显示缩放无法由 DOM 自动化证明；这些用例被
// 记录为需要人工确认，而不是伪造通过。

import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { chromium } from 'playwright-core';

const READY_TIMEOUT_MS = 60_000;
const GUI_READY_TIMEOUT_MS = 60_000;
const CDP_PORT = 9455;
const liveChildren = new Set();

/** Reads one required `--name=value` argument or fails the run. */
function readRequiredArgument(name) {
  const prefix = `--${name}=`;
  const match = process.argv.find((argument) => argument.startsWith(prefix));
  if (!match || match.slice(prefix.length).trim().length === 0) {
    throw new Error(`Missing required argument ${prefix}<value>.`);
  }
  return match.slice(prefix.length);
}

/** Collects per-case acceptance results. */
class AcceptanceResults {
  constructor() {
    this.cases = [];
  }

  record(id, title, status, detail) {
    this.cases.push({ id, title, status, detail });
    process.stderr.write(`[w05] ${status.toUpperCase()} ${id} ${title} :: ${detail}\n`);
  }

  async run(id, title, body) {
    try {
      const outcome = await body();
      this.record(id, title, outcome.status, outcome.detail);
    } catch (error) {
      this.record(id, title, 'fail', String(error?.message ?? error));
    }
  }
}

/** Waits for a bounded time. */
function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/** Registers one child process for guaranteed reaping. */
function trackChild(child, onExit) {
  liveChildren.add(child);
  child.once('exit', (code) => {
    liveChildren.delete(child);
    onExit?.(code);
  });
  return child;
}

/** Terminates every still-running child so the harness can exit promptly. */
function reapAllChildren() {
  for (const child of liveChildren) {
    try {
      child.kill('SIGKILL');
    } catch {
      // The child already exited.
    }
  }
  liveChildren.clear();
}

/** Reads the discovery snapshot inside one isolated test root. */
async function readDiscoverySnapshot(testRoot) {
  try {
    return JSON.parse(
      await readFile(path.join(testRoot, 'user-vulcan', 'code', 'webui-service.json'), 'utf8'),
    );
  } catch {
    return null;
  }
}

/** Confirms one live service instance through its own health endpoint. */
async function waitForLiveService(testRoot, deadlineMs) {
  const deadline = Date.now() + deadlineMs;
  for (;;) {
    const snapshot = await readDiscoverySnapshot(testRoot);
    if (snapshot?.baseUrl && typeof snapshot.pid === 'number') {
      try {
        const response = await fetch(`${snapshot.baseUrl}/health`, { cache: 'no-store' });
        if (response.ok) {
          const health = await response.json();
          if (health.pid === snapshot.pid) {
            return snapshot;
          }
        }
      } catch {
        // The service may still be starting.
      }
    }
    if (Date.now() >= deadline) {
      return null;
    }
    await delay(100);
  }
}

/** Returns whether the process is still alive. */
function isAlive(child) {
  return child.exitCode === null && child.signalCode === null;
}

/** Counts the native drag regions currently mounted in the workbench chrome. */
async function countDragRegions(page) {
  return page.evaluate(() => document.querySelectorAll('[data-tauri-drag-region]').length);
}

/** Runs every mechanically observable G case. */
async function runAcceptance(serviceExecutable, desktopRuntime, desktopEntry, results) {
  const testRoot = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w05-'));
  const service = trackChild(
    spawn(serviceExecutable, ['--headless', `--test-root=${testRoot}`], {
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
    }),
  );
  service.stdout.resume();
  service.stderr.resume();

  const instance = await waitForLiveService(testRoot, READY_TIMEOUT_MS);
  if (!instance) {
    results.record('ALL', 'Desktop GUI acceptance', 'fail', 'the Core service never became ready');
    return;
  }

  let guiExitCode = null;
  const gui = trackChild(
    spawn(
      desktopRuntime,
      [
        desktopEntry,
        `--service-executable=${serviceExecutable}`,
        `--test-root=${testRoot}`,
        // Keep the terminal-owned Core quiet and verify the GUI forwards this value-free flag.
        // 保持终端持有的 Core 安静，并验证 GUI 会转发这个无值开关。
        '--no-debug-console',
        `--webui-url=${instance.baseUrl}`,
        `--remote-debugging-port=${CDP_PORT}`,
      ],
      { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true },
    ),
    (code) => {
      guiExitCode = code;
    },
  );
  const guiDiagnostics = [];
  gui.stdout.resume();
  gui.stderr.on('data', (chunk) => guiDiagnostics.push(chunk.toString('utf8')));

  // Playwright drives the real GUI renderer over CDP, which is the same automation path used by
  // the browser compatibility gate.
  // Playwright 通过 CDP 驱动真实 GUI 渲染进程，与浏览器兼容关卡使用同一条自动化路径。
  let browser = null;
  const deadline = Date.now() + GUI_READY_TIMEOUT_MS;
  while (!browser && Date.now() < deadline) {
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${CDP_PORT}`);
    } catch {
      await delay(250);
    }
  }
  if (!browser) {
    results.record(
      'ALL',
      'Desktop GUI acceptance',
      'fail',
      `the GUI never exposed a debuggable renderer: ${guiDiagnostics.join('').trim().slice(0, 300)}`,
    );
    return;
  }

  try {
    const context = browser.contexts()[0];
    let page = null;
    const pageDeadline = Date.now() + 30_000;
    while (!page && Date.now() < pageDeadline) {
      page = context.pages().find((candidate) => candidate.url().startsWith(instance.baseUrl)) ?? null;
      if (!page) {
        await delay(250);
      }
    }
    if (!page) {
      results.record(
        'ALL',
        'Desktop GUI acceptance',
        'fail',
        `the workbench window never loaded the service origin: ${context.pages().map((entry) => entry.url()).join(', ')}`,
      );
      return;
    }

    // waitForBridge confirms the isolated preload mounted the complete contract in the renderer.
    // waitForBridge 确认隔离预加载在渲染进程中挂载了完整契约。
    const waitForBridge = async () => {
      await page.waitForFunction(() => {
        const desktop = window.vulcanDesktop?.desktop;
        return Boolean(
          desktop &&
            typeof desktop.toggleWindowMaximize === 'function' &&
            typeof desktop.isWindowMaximized === 'function' &&
            typeof desktop.minimizeWindow === 'function' &&
            typeof desktop.closeWindow === 'function',
        );
      }, null, { timeout: 20_000 });
    };
    await waitForBridge();
    // The window chrome must be mounted before drag-region and control assertions are meaningful.
    // 窗口外壳必须已挂载，拖动区域与控件断言才有意义。
    await page.waitForSelector('[data-tauri-drag-region]', { timeout: 20_000 });

    const bridgeCall = (method, ...args) =>
      page.evaluate(
        ([name, callArgs]) => window.vulcanDesktop.desktop[name](...callArgs),
        [method, args],
      );

    await results.run('G02', 'One maximize command from restored converges to maximized exactly once', async () => {
      if (await bridgeCall('isWindowMaximized')) {
        await bridgeCall('toggleWindowMaximize');
        await delay(800);
      }
      const restoredBefore = await bridgeCall('isWindowMaximized');
      const dragRegionsWhenRestored = await countDragRegions(page);
      await bridgeCall('toggleWindowMaximize');
      await delay(900);
      const maximizedAfter = await bridgeCall('isWindowMaximized');
      const confirmed = await bridgeCall('isWindowMaximized');
      // While maximized the title bar must stop being a native drag region, otherwise dragging the
      // maximized title bar corrupts the restore bounds.
      // 最大化时标题栏必须不再是原生拖动区域，否则拖动最大化标题栏会破坏还原边界。
      const dragRegionsWhenMaximized = await countDragRegions(page);
      await bridgeCall('toggleWindowMaximize');
      await delay(900);
      const restoredAgain = await bridgeCall('isWindowMaximized');
      const dragRegionsAfterRestore = await countDragRegions(page);
      return {
        status:
          restoredBefore === false &&
          maximizedAfter === true &&
          confirmed === true &&
          restoredAgain === false &&
          dragRegionsWhenRestored > 0 &&
          dragRegionsWhenMaximized === 0 &&
          dragRegionsAfterRestore > 0
          ? 'pass'
          : 'fail',
        detail: `restored=${restoredBefore} maximizedAfterOneToggle=${maximizedAfter} repeatedQuery=${confirmed} restoredAgain=${restoredAgain} dragRegions[restored=${dragRegionsWhenRestored} maximized=${dragRegionsWhenMaximized} restoredAgain=${dragRegionsAfterRestore}] nativeDoubleClickDeliveryNeedsHuman=true`,
      };
    });

    await results.run('G03', 'The maximized window restores to a sane original size', async () => {
      if (!(await bridgeCall('isWindowMaximized'))) {
        await bridgeCall('toggleWindowMaximize');
        await delay(900);
      }
      const maximizedBefore = await bridgeCall('isWindowMaximized');
      await bridgeCall('toggleWindowMaximize');
      await delay(1_000);
      const restored = await bridgeCall('isWindowMaximized');
      const bounds = await page.evaluate(() => ({
        width: window.outerWidth,
        height: window.outerHeight,
      }));
      // A corrupted restore shows up as a degenerate or unreasonably small window.
      // 还原边界损坏会表现为退化或过小的窗口。
      const saneSize = bounds.width >= 900 && bounds.height >= 500;
      return {
        status: maximizedBefore && restored === false && saneSize ? 'pass' : 'fail',
        detail: `maximizedBefore=${maximizedBefore} restoredAfterToggle=${restored} size=${bounds.width}x${bounds.height}`,
      };
    });

    await results.run('G04', 'Rapid mixed maximize input never leaves an inconsistent state', async () => {
      const start = await bridgeCall('isWindowMaximized');
      // Six concurrent toggles from a known state must converge to a single queryable state.
      // 从已知状态并发发出六次切换，必须收敛到单一可查询状态。
      await page.evaluate(() => {
        const desktop = window.vulcanDesktop.desktop;
        return Promise.all([
          desktop.toggleWindowMaximize(),
          desktop.toggleWindowMaximize(),
          desktop.toggleWindowMaximize(),
          desktop.toggleWindowMaximize(),
          desktop.toggleWindowMaximize(),
          desktop.toggleWindowMaximize(),
        ]);
      });
      await delay(1_200);
      const first = await bridgeCall('isWindowMaximized');
      const second = await bridgeCall('isWindowMaximized');
      // Even toggles from a known state must return to that state, and repeated queries must agree.
      // 从已知状态发出偶数次切换必须回到该状态，且重复查询必须一致。
      const converged = first === start;
      const stable = first === second;
      return {
        status: converged && stable ? 'pass' : 'fail',
        detail: `start=${start} afterSixToggles=${first} repeatedQuery=${second}`,
      };
    });

    await results.run('G05', 'Minimizing and restoring reports correct state without restarting Core', async () => {
      const servicePidBefore = (await readDiscoverySnapshot(testRoot))?.pid;
      await bridgeCall('minimizeWindow');
      await delay(900);
      const minimized = await bridgeCall('isWindowMinimized');
      const visibleWhileMinimized = await bridgeCall('isWindowVisible');
      const presentation = await page.evaluate(() =>
        window.vulcanDesktop.desktop.isWindowFocused(),
      );
      // Restore through the main process so the window is usable for later cases.
      // 通过主进程还原窗口，使后续用例可用。
      await page.evaluate(() => window.vulcanDesktop.desktop.toggleWindowMaximize());
      await delay(600);
      await page.evaluate(() => window.vulcanDesktop.desktop.toggleWindowMaximize());
      await delay(900);
      const minimizedAfter = await bridgeCall('isWindowMinimized');
      const servicePidAfter = (await readDiscoverySnapshot(testRoot))?.pid;
      return {
        status:
          minimized === true && minimizedAfter === false && servicePidBefore === servicePidAfter
            ? 'pass'
            : 'fail',
        detail: `minimized=${minimized} visibleWhileMinimized=${visibleWhileMinimized} focusedWhileMinimized=${presentation} minimizedAfterRestore=${minimizedAfter} servicePidStable=${servicePidBefore === servicePidAfter}`,
      };
    });

    await results.run('G09', 'Interactive window controls are not swallowed by the drag region', async () => {
      const regions = await page.evaluate(() => {
        const read = (element) => {
          if (!element) {
            return null;
          }
          return getComputedStyle(element).getPropertyValue('-webkit-app-region').trim();
        };
        return {
          hostMarker: document.documentElement.dataset.vulcanHost ?? null,
          titlebar: read(document.querySelector('[data-tauri-drag-region]')),
          controls: read(document.querySelector('.vk-winctl')),
          noDragAction: read(document.querySelector('.vk-stage-chrome')),
          buttonInsideTitlebar: read(document.querySelector('[data-tauri-drag-region] button')),
        };
      });
      // The title bar must drag while every control stays clickable.
      // 标题栏必须可拖动，同时每个控件保持可点击。
      const dragOk = regions.titlebar === 'drag';
      // The desktop host always mounts its own window controls, so a missing control row is a failure.
      // 桌面宿主总会挂载自有窗口控件，因此缺少控件行即为失败。
      const controlsOk = regions.controls === 'no-drag';
      const actionOk = regions.noDragAction === 'no-drag' || regions.noDragAction === null;
      const nestedOk =
        regions.buttonInsideTitlebar === 'no-drag' || regions.buttonInsideTitlebar === null;
      return {
        status: dragOk && controlsOk && actionOk && nestedOk ? 'pass' : 'fail',
        detail: `host=${regions.hostMarker} titlebar=${regions.titlebar} windowControls=${regions.controls} chromeActions=${regions.noDragAction} nestedButton=${regions.buttonInsideTitlebar}`,
      };
    });

    await results.run('G10', 'Drag regions stay inside the title chrome and never cover business surfaces', async () => {
      const geometry = await page.evaluate(() => {
        const regions = [...document.querySelectorAll('[data-tauri-drag-region]')];
        return {
          count: regions.length,
          // Every drag region must sit in the top chrome band, not over the workbench body.
          // 每个拖动区域都必须位于顶部外壳带内，而不是覆盖工作台主体。
          allInsideTopBand: regions.every((region) => region.getBoundingClientRect().bottom <= 80),
          lowestBottom: regions.reduce(
            (lowest, region) => Math.max(lowest, Math.round(region.getBoundingClientRect().bottom)),
            0,
          ),
          // No drag region may contain a separator or a text surface.
          noneContainsBusinessSurface: regions.every(
            (region) =>
              region.querySelector('[role="separator"], .terminal, .editor, textarea, input') === null,
          ),
        };
      });
      return {
        status:
          geometry.count > 0 && geometry.allInsideTopBand && geometry.noneContainsBusinessSurface
            ? 'pass'
            : 'fail',
        detail: `dragRegions=${geometry.count} allInsideTopBand=${geometry.allInsideTopBand} lowestBottomPx=${geometry.lowestBottom} noneContainsBusinessSurface=${geometry.noneContainsBusinessSurface}`,
      };
    });

    await results.run('G15', 'A plain browser gets no native window controls for the same page', async () => {
      // The plain-browser shell must report no native window controls so the WebUI hides them.
      // 普通浏览器壳必须报告没有原生窗口控制，使 WebUI 隐藏它们。
      const browserHost = await fetch(`${instance.baseUrl}/`).then((response) => response.text());
      const injectsBridge = browserHost.includes('vulcanDesktop');
      const guiHasBridge = await page.evaluate(
        () => typeof window.vulcanDesktop?.desktop?.toggleWindowMaximize === 'function',
      );
      return {
        status: !injectsBridge && guiHasBridge ? 'pass' : 'fail',
        detail: `plainBrowserDocumentInjectsBridge=${injectsBridge} guiRendererBridgePresent=${guiHasBridge}`,
      };
    });

    // G06 is destructive for the GUI process, so it runs last.
    // G06 会销毁 GUI 进程，因此最后运行。
    await results.run('G06', 'Closing the GUI window leaves the independent Core running', async () => {
      const servicePidBefore = (await readDiscoverySnapshot(testRoot))?.pid;
      // Closing the window destroys its renderer, so the pending evaluate promise rejects.
      // 关闭窗口会销毁其渲染进程，因此未完成的 evaluate 承诺会被拒绝。
      await page
        .evaluate(() => window.vulcanDesktop.desktop.closeWindow())
        .catch(() => undefined);
      const closeDeadline = Date.now() + 20_000;
      while (isAlive(gui) && Date.now() < closeDeadline) {
        await delay(200);
      }
      const guiClosed = !isAlive(gui);
      await delay(1_000);
      const health = await fetch(`${instance.baseUrl}/health`, { cache: 'no-store' })
        .then((response) => response.json())
        .catch(() => null);
      const servicePidAfter = (await readDiscoverySnapshot(testRoot))?.pid;
      return {
        status:
          guiClosed && isAlive(service) && health?.pid === servicePidBefore &&
          servicePidAfter === servicePidBefore
            ? 'pass'
            : 'fail',
        detail: `guiClosed=${guiClosed} guiExitCode=${guiExitCode} coreAlive=${isAlive(service)} corePid=${servicePidBefore}->${servicePidAfter} healthPid=${health?.pid}`,
      };
    });
  } finally {
    // Closing a CDP connection to a renderer whose process already exited races the connection
    // teardown, so the connection is only closed while the GUI is still alive.
    // 对进程已退出的渲染进程关闭 CDP 连接会与其连接拆除竞争，因此仅在 GUI 仍存活时关闭连接。
    if (isAlive(gui)) {
      await browser.close().catch(() => {});
    }
  }

  recordHumanSignOffCases(results);
}

/** Records the cases that DOM automation cannot prove. */
function recordHumanSignOffCases(results) {
  results.record(
    'G01',
    'Dragging brand, sidebar, feature page, and console title areas moves the window',
    'unverified',
    'The drag-region mechanism is verified (G09/G10), but real native window movement needs human observation.',
  );
  results.record(
    'G07',
    'Dragging each of the four edges resizes the native window',
    'unverified',
    'Requires operating-system edge hit-testing; DOM automation cannot perform a native resize drag.',
  );
  results.record(
    'G08',
    'Dragging each of the four corners hits correctly and honours the minimum size',
    'unverified',
    'The minimum-size constraint is configured (960x600) but corner hit-testing needs human observation.',
  );
  results.record(
    'G11',
    '100%, 125%, and 150% display scaling keep controls and native views correct',
    'unverified',
    'Requires a display-scaling change or an equivalent environment; not available in this run.',
  );
  results.record(
    'G12',
    'Maximize and restore across monitors keep the window and its restore size',
    'unverified',
    'Requires a multi-monitor desktop; not available in this run.',
  );
  results.record(
    'G13',
    'A temporary browser view never covers a workbench modal',
    'unverified',
    'The native browser WebContentsView does not exist yet (W05 step 7 depends on W06/W07).',
  );
  results.record(
    'G14',
    'Clicking a notification activates the GUI and locates the business item without a new Core',
    'unverified',
    'The activation handler is implemented, but clicking a real Windows notification needs human action.',
  );
}

/** Renders the acceptance report. */
function renderReport(report) {
  const lines = [
    '# W05 desktop GUI acceptance report',
    '',
    'Generated by `apps/vulcan-code-desktop/tools/w05-gui-acceptance.mjs`.',
    '',
    `- Electron runtime: ${report.desktopRuntime}`,
    `- GUI entry: ${report.desktopEntry}`,
    `- Service executable: ${report.serviceExecutable}`,
    `- Platform: ${report.platform} ${report.architecture}`,
    `- Node.js: ${report.nodeVersion}`,
    '',
    '| Case | Behaviour | Status | Detail |',
    '| --- | --- | --- | --- |',
  ];
  for (const item of report.cases) {
    lines.push(`| ${item.id} | ${item.title} | ${item.status} | ${item.detail.replaceAll('|', '\\|')} |`);
  }
  lines.push('');
  return lines.join('\n');
}

/** Runs the harness entry point. */
async function main() {
  const serviceExecutable = readRequiredArgument('service-exe');
  const desktopRuntime = readRequiredArgument('desktop-runtime');
  const desktopEntry = readRequiredArgument('desktop-entry');
  const evidenceDirectory = readRequiredArgument('evidence-dir');
  await mkdir(evidenceDirectory, { recursive: true });

  const results = new AcceptanceResults();
  try {
    await runAcceptance(serviceExecutable, desktopRuntime, desktopEntry, results);
  } finally {
    reapAllChildren();
  }

  const report = {
    serviceExecutable,
    desktopRuntime,
    desktopEntry,
    platform: process.platform,
    architecture: process.arch,
    nodeVersion: process.version,
    cases: results.cases,
  };
  await writeFile(
    path.join(evidenceDirectory, 'w05-gui-acceptance.json'),
    JSON.stringify(report, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(evidenceDirectory, 'w05-gui-acceptance.md'),
    renderReport(report),
    'utf8',
  );

  const failing = results.cases.filter((item) => item.status === 'fail');
  const passing = results.cases.filter((item) => item.status === 'pass');
  process.stderr.write(
    `[w05] completed: ${passing.length} pass, ${failing.length} fail, ${
      results.cases.length - passing.length - failing.length
    } unverified\n`,
  );
  // A short drain lets libuv finish closing connect and child handles before the process ends.
  // 短暂排空让 libuv 在进程结束前完成连接与子进程句柄的关闭。
  await delay(250);
  process.exit(failing.length === 0 ? 0 : 1);
}

await main();
