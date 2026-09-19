// W02 browser-runtime compatibility harness.
// W02 浏览器运行时兼容性验证工具。
//
// This harness runs one isolated Electron browser host and measures what Playwright can and
// cannot do over CDP against that exact runtime. It records success, failure, or
// unverified for every required behaviour; it never substitutes a fabricated pass.
// 本工具运行一个隔离的 Electron 浏览器宿主，并测量 Playwright 针对该精确运行时经 CDP
// 能做与不能做的行为。它为每项必需行为记录成功、失败或未验证，绝不伪造通过。
//
// Usage / 用法:
//   node tools/w02-compatibility.mjs \
//     --desktop-runtime=<electron executable> \
//     --desktop-entry=<browser-host compiled entry> \
//     --evidence-dir=<isolated absolute directory>

import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { connect } from 'node:net';
import { createServer } from 'node:http';
import { mkdir, mkdtemp, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright-core');

const HARNESS_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIRECTORY = path.join(HARNESS_DIRECTORY, 'fixtures');
const HOST_HANDSHAKE_TIMEOUT_MS = 30_000;
const CHECK_TIMEOUT_MS = 20_000;

// Measured automation-library boundaries are recorded as failures but do not by themselves fail
// the gate, because a verified product-level path covers the same required capability.
// 已实测的自动化库边界记录为失败，但其自身不会使关卡失败，因为已验证的产品级路径覆盖同一必需能力。
const AUTOMATION_LIBRARY_BOUNDARY_CASES = new Set(['PAGE_CREATE_VIA_LIBRARY']);

/** Returns whether a filesystem path exists without reading its contents. */
async function pathExists(candidate) {
  try {
    await stat(candidate);
    return true;
  } catch {
    return false;
  }
}

/** Audits whether any automation library silently downloaded its own browser runtime. */
async function auditImplicitBrowserDownloads() {
  const candidates = [
    process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, 'ms-playwright') : null,
    path.join(os.homedir(), '.cache', 'ms-playwright'),
    path.join(os.homedir(), 'Library', 'Caches', 'ms-playwright'),
    path.join(os.homedir(), '.cache', 'ms-playwright-go'),
  ].filter(Boolean);
  const present = [];
  for (const candidate of candidates) {
    if (await pathExists(candidate)) {
      present.push(candidate);
    }
  }
  return present;
}

/** Reads one required `--name=value` argument or fails the run. */
function readRequiredArgument(name) {
  const prefix = `--${name}=`;
  const match = process.argv.find((argument) => argument.startsWith(prefix));
  if (!match || match.slice(prefix.length).trim().length === 0) {
    throw new Error(`Missing required argument ${prefix}<value>.`);
  }
  return match.slice(prefix.length);
}

/** Runs one promise with a bounded timeout. */
function withTimeout(promise, timeoutMs, label) {
  let timer = null;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} timed out after ${timeoutMs}ms.`)), timeoutMs);
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer) {
      clearTimeout(timer);
    }
  });
}

/** Reads one bundled runtime version from the Electron binary using its Node mode. */
async function readElectronVersionField(desktopRuntime, field) {
  const output = await new Promise((resolve) => {
    const child = spawn(desktopRuntime, ['-p', `process.versions.${field}`], {
      env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' },
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
    });
    let text = '';
    child.stdout.on('data', (chunk) => {
      text += chunk.toString('utf8');
    });
    child.on('error', () => resolve(''));
    child.on('exit', () => resolve(text.trim()));
  });
  return output.length > 0 ? output : '未确认';
}

/** Serves the fixture directory on an ephemeral loopback port. */
async function startFixtureServer() {
  const server = createServer(async (request, response) => {
    const requestPath = decodeURIComponent((request.url ?? '/').split('?')[0]);
    const relative = requestPath === '/' ? 'w02-page.html' : requestPath.replace(/^\/+/, '');
    const resolved = path.resolve(FIXTURE_DIRECTORY, relative);
    if (!resolved.startsWith(path.resolve(FIXTURE_DIRECTORY))) {
      response.writeHead(403).end('forbidden');
      return;
    }
    try {
      const body = await readFile(resolved);
      response.writeHead(200, {
        'content-type': resolved.endsWith('.html') ? 'text/html; charset=utf-8' : 'text/plain; charset=utf-8',
        'content-length': String(body.length),
      });
      response.end(body);
    } catch {
      response.writeHead(404).end('not found');
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  return { server, baseUrl: `http://127.0.0.1:${address.port}` };
}

/** Encodes one JSON control frame with its 4-byte big-endian length prefix. */
function encodeFrame(value) {
  const body = Buffer.from(JSON.stringify(value), 'utf8');
  const frame = Buffer.allocUnsafe(4 + body.length);
  frame.writeUInt32BE(body.length, 0);
  body.copy(frame, 4);
  return frame;
}

/** Decodes one length-prefixed control frame from a byte accumulator. */
function createFrameReader(onValue) {
  let pending = Buffer.alloc(0);
  return (chunk) => {
    pending = Buffer.concat([pending, chunk]);
    for (;;) {
      if (pending.length < 4) {
        return;
      }
      const length = pending.readUInt32BE(0);
      if (pending.length < 4 + length) {
        return;
      }
      const body = pending.subarray(4, 4 + length);
      pending = pending.subarray(4 + length);
      onValue(JSON.parse(body.toString('utf8')));
    }
  };
}

/** Sums the byte size of every file below one directory. */
async function directorySize(directory) {
  let total = 0;
  let entries = [];
  try {
    entries = await readdir(directory, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const entry of entries) {
    const child = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      total += await directorySize(child);
    } else if (entry.isFile()) {
      try {
        total += (await stat(child)).size;
      } catch {
        // Unreadable files are excluded from the measured total instead of failing the run.
      }
    }
  }
  return total;
}

/** Collects per-case results for the compatibility report. */
class CompatibilityResults {
  constructor() {
    this.cases = [];
  }

  /** Records one measured case. */
  record(id, title, status, detail, evidence = null) {
    this.cases.push({ id, title, status, detail, evidence });
    const marker = status === 'pass' ? 'PASS' : status === 'fail' ? 'FAIL' : 'UNVERIFIED';
    process.stderr.write(`[w02] ${marker} ${id} ${title} :: ${detail}\n`);
  }

  /** Runs one case function, converting thrown errors into a recorded failure. */
  async run(id, title, body) {
    try {
      const outcome = await withTimeout(body(), CHECK_TIMEOUT_MS, id);
      this.record(id, title, outcome.status, outcome.detail, outcome.evidence ?? null);
    } catch (error) {
      this.record(id, title, 'fail', String(error?.message ?? error));
    }
  }
}

/** Creates one correlated control channel client over an accepted control socket. */
function createControlClient(socket, sessionId, hostGeneration) {
  const pending = new Map();
  const events = [];
  let nextRequestIndex = 0;
  let resolveHandshake = null;
  let rejectHandshake = null;
  const handshake = new Promise((resolve, reject) => {
    resolveHandshake = resolve;
    rejectHandshake = reject;
  });
  socket.on(
    'data',
    createFrameReader((value) => {
      if (value && typeof value.protocolVersion === 'number' && value.cdpEndpoint) {
        resolveHandshake(value);
        return;
      }
      if (value && typeof value.requestId === 'string' && typeof value.ok === 'boolean') {
        const entry = pending.get(value.requestId);
        if (entry) {
          pending.delete(value.requestId);
          if (value.ok) {
            entry.resolve(value.result);
          } else {
            entry.reject(new Error(`${value.error?.code ?? 'error'}: ${value.error?.message ?? 'unknown'}`));
          }
        }
        return;
      }
      events.push(value);
    }),
  );
  socket.on('close', () => {
    rejectHandshake(new Error('Control connection closed before the handshake.'));
  });
  return {
    handshake,
    events,
    request(operation, params = {}) {
      nextRequestIndex += 1;
      const requestId = `w02-${nextRequestIndex}`;
      return withTimeout(
        new Promise((resolve, reject) => {
          pending.set(requestId, { resolve, reject });
          socket.write(encodeFrame({ requestId, sessionId, hostGeneration, operation, params }));
        }),
        CHECK_TIMEOUT_MS,
        operation,
      );
    },
  };
}

/** Spawns one throwaway host and proves a wrong control secret is rejected. */
async function verifyControlSecretIsRequired(options, profileDirectory, sessionId, hostGeneration) {
  const isolatedRoot = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w02-negative-'));
  const negativeProfile = path.join(isolatedRoot, 'profile');
  await mkdir(negativeProfile, { recursive: true });
  const host = spawn(
    options.desktopRuntime,
    [
      options.desktopEntry,
      `--session-id=${sessionId}`,
      `--host-generation=${hostGeneration}`,
      `--user-data-dir=${negativeProfile}`,
      '--cdp-port=0',
      `--control-secret=${randomUUID()}`,
      '--initial-url=about:blank',
    ],
    { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true },
  );
  host.stderr.resume();
  try {
    const discoveryPath = path.join(negativeProfile, 'browser-host-control.json');
    const deadline = Date.now() + HOST_HANDSHAKE_TIMEOUT_MS;
    let document = null;
    while (!document && Date.now() < deadline) {
      try {
        document = JSON.parse(await readFile(discoveryPath, 'utf8'));
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    }
    if (!document) {
      return { status: 'fail', detail: 'Negative-control host did not publish its control endpoint.' };
    }
    const outcome = await new Promise((resolve) => {
      const socket = connect({ host: document.controlHost, port: document.controlPort }, () => {
        socket.write(
          encodeFrame({
            kind: 'coreHello',
            protocolVersion: document.protocolVersion,
            connectionSecret: randomUUID(),
            sessionId,
            hostGeneration,
          }),
        );
      });
      let handshakeSeen = false;
      socket.on(
        'data',
        createFrameReader(() => {
          handshakeSeen = true;
        }),
      );
      socket.on('close', () => resolve(handshakeSeen ? 'handshake' : 'rejected'));
      socket.on('error', () => resolve('rejected'));
      setTimeout(() => resolve(handshakeSeen ? 'handshake' : 'timeout'), 4000);
    });
    return {
      status: outcome === 'rejected' ? 'pass' : 'fail',
      detail: `wrong-secret outcome=${outcome}`,
    };
  } finally {
    host.kill();
  }
}

/** Runs the whole isolated experiment and returns its report data. */
async function runCompatibilityExperiment(options) {
  const results = new CompatibilityResults();
  const isolatedRoot = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w02-'));
  const profileDirectory = path.join(isolatedRoot, 'browser-profile');
  await mkdir(profileDirectory, { recursive: true });
  const downloadDirectory = path.join(isolatedRoot, 'downloads');
  await mkdir(downloadDirectory, { recursive: true });

  const electronDist = path.dirname(options.desktopRuntime);
  const electronDistBytes = await directorySize(electronDist);

  const { server, baseUrl } = await startFixtureServer();
  const sessionId = 'w02-session';
  const hostGeneration = '1';
  const connectionSecret = randomUUID();
  const host = spawn(
    options.desktopRuntime,
    [
      options.desktopEntry,
      `--session-id=${sessionId}`,
      `--host-generation=${hostGeneration}`,
      `--user-data-dir=${profileDirectory}`,
      '--cdp-port=0',
      `--control-secret=${connectionSecret}`,
      `--initial-url=${baseUrl}/w02-page.html`,
    ],
    { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true },
  );
  const hostDiagnostics = [];
  host.stderr.on('data', (chunk) => {
    const text = chunk.toString('utf8');
    hostDiagnostics.push(text);
    // Host diagnostics are mirrored immediately so a stalled host is diagnosable.
    // 立即转发宿主诊断，使停滞的宿主可被诊断。
    process.stderr.write(`[w02:host] ${text}`);
  });

  // The host publishes its control endpoint through an atomic discovery file inside its own
  // user-data directory: this platform gives the Electron main process neither a readable
  // standard-input pipe nor a clean standard-output stream.
  // 宿主通过其自有用户数据目录内的原子发现文件发布控制端点：本平台上 Electron 主进程既拿不到
  // 可读的标准输入管道，也拿不到干净的标准输出流。
  const endpointFrame = await withTimeout(
    (async () => {
      const discoveryPath = path.join(profileDirectory, 'browser-host-control.json');
      const deadline = Date.now() + HOST_HANDSHAKE_TIMEOUT_MS;
      for (;;) {
        try {
          const document = JSON.parse(await readFile(discoveryPath, 'utf8'));
          if (document && typeof document.controlPort === 'number' && document.controlPort > 0) {
            return document;
          }
        } catch {
          // The host may not have published yet.
        }
        if (Date.now() >= deadline) {
          throw new Error(
            `Browser host did not publish its control discovery file within ${HOST_HANDSHAKE_TIMEOUT_MS}ms.`,
          );
        }
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    })(),
    HOST_HANDSHAKE_TIMEOUT_MS,
    'browser host control endpoint',
  );

  const control = await withTimeout(
    new Promise((resolve, reject) => {
      const socket = connect({ host: endpointFrame.controlHost, port: endpointFrame.controlPort }, () => {
        socket.write(
          encodeFrame({
            kind: 'coreHello',
            protocolVersion: endpointFrame.protocolVersion,
            connectionSecret,
            sessionId,
            hostGeneration,
          }),
        );
        resolve(socket);
      });
      socket.once('error', reject);
    }),
    HOST_HANDSHAKE_TIMEOUT_MS,
    'browser host control connection',
  );

  const controlClient = createControlClient(control, sessionId, hostGeneration);
  const handshake = await withTimeout(controlClient.handshake, HOST_HANDSHAKE_TIMEOUT_MS, 'browser host handshake');

  results.record(
    'HOST_CONTROL_CHANNEL',
    'Host accepts one authenticated loopback control connection and reports its identity',
    handshake.protocolVersion === 1 && handshake.sessionId === sessionId && handshake.hostGeneration === hostGeneration
      ? 'pass'
      : 'fail',
    `protocolVersion=${handshake.protocolVersion} sessionId=${handshake.sessionId} hostGeneration=${handshake.hostGeneration}`,
    { capabilities: handshake.capabilities },
  );
  results.record(
    'HOST_CDP_ENDPOINT',
    'Host publishes a local CDP endpoint discovered from its own user-data directory',
    handshake.cdpEndpoint?.port > 0 ? 'pass' : 'fail',
    `pid=${handshake.pid} runtime=${handshake.runtimeVersion} cdp=${handshake.cdpEndpoint?.host}:${handshake.cdpEndpoint?.port}`,
    { endpoint: handshake.cdpEndpoint },
  );
  await results.run(
    'HOST_CONTROL_SECRET_REQUIRED',
    'Host rejects a control connection that presents the wrong spawn-time secret',
    async () => verifyControlSecretIsRequired(options, profileDirectory, sessionId, hostGeneration),
  );

  const cdpUrl = `http://${handshake.cdpEndpoint.host}:${handshake.cdpEndpoint.port}`;
  let browser = null;
  try {
    await results.run('CDP_CONNECT', 'Playwright connects to the existing host over CDP', async () => {
      browser = await chromium.connectOverCDP(cdpUrl);
      return {
        status: browser.isConnected() ? 'pass' : 'fail',
        detail: `connectOverCDP(${cdpUrl}) connected=${browser.isConnected()}`,
      };
    });

    if (!browser) {
      results.record('ALL_DEPENDENT', 'Remaining checks', 'unverified', 'CDP connection failed, so no dependent check could run.');
      return { results, isolatedRoot, hostDiagnostics, electronDistBytes, baseUrl, sessionId, hostGeneration };
    }

    const context = browser.contexts()[0];
    await results.run('DEFAULT_CONTEXT', 'Host exposes one default browser context', async () => ({
      status: browser.contexts().length === 1 && context ? 'pass' : 'fail',
      detail: `contexts=${browser.contexts().length}`,
    }));

    await results.run('PAGE_ENUMERATION', 'Existing host page is enumerable through CDP', async () => {
      const pages = context.pages();
      return {
        status: pages.length >= 1 ? 'pass' : 'fail',
        detail: `pages=${pages.length} urls=${pages.map((page) => page.url()).join(',')}`,
      };
    });

    let workingPage = context.pages()[0] ?? null;
    await results.run('PAGE_CREATE_VIA_LIBRARY', 'Automation library can create a page by itself', async () => {
      try {
        const created = await context.newPage();
        return { status: 'pass', detail: `newPage() succeeded at ${created.url()}` };
      } catch (error) {
        return {
          status: 'fail',
          detail: `Library-owned page creation is unsupported on this runtime: ${String(error?.message ?? error)}`,
        };
      }
    });

    let createdByHost = null;
    await results.run('PAGE_CREATE_HOST_OWNED', 'Host-owned page creation produces a page automation can see', async () => {
      const before = context.pages().length;
      createdByHost = await controlClient.request('page.create', { url: `${baseUrl}/w02-iframe.html` });
      const deadline = Date.now() + 8000;
      let matched = null;
      while (!matched && Date.now() < deadline) {
        matched = context.pages().find((page) => page.url() === createdByHost.url) ?? null;
        if (!matched) {
          await new Promise((resolve) => setTimeout(resolve, 100));
        }
      }
      return {
        status: matched ? 'pass' : 'fail',
        detail: `hostCreatedUrl=${createdByHost.url} pagesBefore=${before} pagesAfter=${context.pages().length} automationSawPage=${Boolean(matched)}`,
      };
    });

    await results.run('HOST_NAVIGATION', 'Core control navigates the explicitly named page', async () => {
      const targetId = String(createdByHost.webContentsId);
      const destination = `${baseUrl}/w02-page.html`;
      const result = await controlClient.request('page.navigate', { targetId, url: destination });
      const page = context.pages().find((candidate) => candidate.url() === destination);
      return {
        status: result.targetId === targetId && result.url === destination && Boolean(page) ? 'pass' : 'fail',
        detail: `targetId=${result.targetId} url=${result.url} observed=${Boolean(page)}`,
      };
    });

    await results.run('HOST_NAVIGATION_SCHEME_GUARD', 'Core host rejects navigation to a privileged scheme', async () => {
      try {
        await controlClient.request('page.navigate', {
          targetId: String(createdByHost.webContentsId),
          url: 'file:///C:/Windows/win.ini',
        });
        return { status: 'fail', detail: 'file URL was accepted' };
      } catch (error) {
        return {
          status: String(error).includes('invalid_request') ? 'pass' : 'fail',
          detail: String(error),
        };
      }
    });

    await results.run('HOST_VIEWPORT', 'Core host changes the real content viewport for existing and future pages', async () => {
      const applied = await controlClient.request('viewport.set', { width: 1024, height: 768 });
      const created = await controlClient.request('page.create', { url: `${baseUrl}/w02-page.html` });
      const currentPage = context.pages().find((candidate) => candidate.url() === created.url);
      const dimensions = currentPage ? await currentPage.evaluate(() => [innerWidth, innerHeight]) : [];
      return {
        status: applied.width === 1024 && applied.height === 768
          && dimensions[0] === 1024 && dimensions[1] === 768 ? 'pass' : 'fail',
        detail: `applied=${applied.width}x${applied.height} created=${dimensions.join('x')}`,
      };
    });

    if (!workingPage) {
      results.record('ALL_PAGE_CHECKS', 'Page-dependent checks', 'unverified', 'No page was available.');
      return { results, isolatedRoot, hostDiagnostics, electronDistBytes, baseUrl, sessionId, hostGeneration };
    }

    await results.run('NAVIGATION', 'Navigation and title observation work', async () => {
      await workingPage.goto(`${baseUrl}/w02-page.html`, { waitUntil: 'domcontentloaded' });
      const title = await workingPage.title();
      return {
        status: title.includes('W02') ? 'pass' : 'fail',
        detail: `url=${workingPage.url()} title=${title}`,
      };
    });

    await results.run('LOCATOR_CLICK', 'Locator click mutates real page state', async () => {
      await workingPage.click('#click-target');
      const text = await workingPage.textContent('#click-count');
      return { status: text === '1' ? 'pass' : 'fail', detail: `#click-count=${text}` };
    });

    await results.run('KEYBOARD_INPUT', 'Keyboard text entry reaches the page input', async () => {
      await workingPage.fill('#text-input', 'vulcan-cdp-input');
      const value = await workingPage.inputValue('#text-input');
      return { status: value === 'vulcan-cdp-input' ? 'pass' : 'fail', detail: `value=${value}` };
    });

    await results.run('CONTENT_READ', 'Page content retrieval works', async () => {
      const content = await workingPage.content();
      return {
        status: content.includes('fixture-title') ? 'pass' : 'fail',
        detail: `contentBytes=${content.length}`,
      };
    });

    await results.run('SCREENSHOT', 'Screenshot capture returns real image bytes', async () => {
      const image = await workingPage.screenshot();
      return {
        status: image.length > 0 ? 'pass' : 'fail',
        detail: `screenshotBytes=${image.length}`,
      };
    });

    await results.run('IFRAME_ACCESS', 'Iframe content is reachable', async () => {
      const frameText = await workingPage.frameLocator('#fixture-frame').locator('#frame-text').textContent();
      return { status: frameText === 'frame-content' ? 'pass' : 'fail', detail: `frameText=${frameText}` };
    });

    await results.run('POPUP_PAGE', 'Popup target is registered as a new page', async () => {
      const popupPromise = context.waitForEvent('page', { timeout: CHECK_TIMEOUT_MS });
      await workingPage.click('#popup-link');
      const popup = await popupPromise;
      await popup.waitForLoadState('domcontentloaded');
      return { status: popup.url().includes('w02-iframe.html') ? 'pass' : 'fail', detail: `popupUrl=${popup.url()}` };
    });

    // Verify native visibility after all three creation paths, not just a hidden HTML element.
    // 在三种创建路径之后验证原生可见性，而非仅检查隐藏的网页元素。
    await results.run('HIDDEN_HOST_WINDOWS', 'Initial, Core-created, and popup windows stay hidden', async () => {
      const listing = await controlClient.request('page.list');
      return {
        status: listing.pages.length >= 3 && listing.pages.every(page => page.visible === false) ? 'pass' : 'fail',
        detail: JSON.stringify(listing.pages.map(page => ({ targetId: page.targetId, visible: page.visible }))),
      };
    });

    await results.run('DIALOG_HANDLING', 'JavaScript dialog can be observed and answered', async () => {
      let observed = null;
      const handler = async (dialog) => {
        observed = `${dialog.type()}:${dialog.message()}`;
        await dialog.accept();
      };
      workingPage.on('dialog', handler);
      try {
        await workingPage.evaluate(() => {
          setTimeout(() => window.alert('vulcan-dialog'), 0);
        });
        const deadline = Date.now() + 5000;
        while (observed === null && Date.now() < deadline) {
          await new Promise((resolve) => setTimeout(resolve, 50));
        }
      } finally {
        workingPage.off('dialog', handler);
      }
      return { status: observed ? 'pass' : 'fail', detail: `observed=${observed}` };
    });

    await results.run('FILE_UPLOAD', 'File upload accepts a Chinese-named local fixture', async () => {
      const uploadPath = path.join(FIXTURE_DIRECTORY, 'w02-upload-中文.txt');
      await workingPage.setInputFiles('#file-input', uploadPath);
      const names = await workingPage.evaluate(() =>
        Array.from(document.querySelector('#file-input').files ?? []).map((file) => file.name),
      );
      return {
        status: names.length === 1 && names[0] === 'w02-upload-中文.txt' ? 'pass' : 'fail',
        detail: `files=${JSON.stringify(names)}`,
      };
    });

    await results.run('DOWNLOAD_CAPTURE', 'Download can be captured through the automation connection', async () => {
      let download = null;
      const downloadPromise = workingPage
        .waitForEvent('download', { timeout: 5000 })
        .then((value) => {
          download = value;
        })
        .catch(() => {});
      await workingPage.click('#download-link');
      await downloadPromise;
      if (!download) {
        return { status: 'fail', detail: 'No download event was delivered over CDP for this runtime.' };
      }
      const savedTo = path.join(downloadDirectory, download.suggestedFilename());
      await download.saveAs(savedTo);
      const body = await readFile(savedTo, 'utf8');
      return {
        status: body.includes('vulcan-w02-download-payload') ? 'pass' : 'fail',
        detail: `suggestedFilename=${download.suggestedFilename()} bytes=${body.length}`,
      };
    });

    await results.run('COOKIE_AND_LOGIN_STATE', 'Login state written to this context is visible to automation', async () => {
      await context.addCookies([
        { name: 'vulcan_w02_session', value: 'w02-secret-value', url: baseUrl },
      ]);
      const cookies = await context.cookies(baseUrl);
      const observed = cookies.find((cookie) => cookie.name === 'vulcan_w02_session');
      return {
        status: observed?.value === 'w02-secret-value' ? 'pass' : 'fail',
        detail: `cookiePresent=${Boolean(observed)} contextCount=${browser.contexts().length}`,
      };
    });

    let pageUrlBeforeDisconnect = workingPage.url();
    await results.run('DISCONNECT_KEEPS_SESSION', 'Disconnecting automation does not close the user session', async () => {
      pageUrlBeforeDisconnect = workingPage.url();
      await browser.close();
      const stillAlive = host.exitCode === null;
      return {
        status: stillAlive ? 'pass' : 'fail',
        detail: `hostExitCode=${host.exitCode} pageUrlBeforeDisconnect=${pageUrlBeforeDisconnect}`,
      };
    });

    await results.run('RECONNECT_FINDS_PAGE', 'Reconnecting finds the same page in the same context', async () => {
      browser = await chromium.connectOverCDP(cdpUrl);
      const reconnectedContext = browser.contexts()[0];
      const reconnectedPages = reconnectedContext.pages();
      const urls = reconnectedPages.map((page) => page.url());
      const matched = reconnectedPages.find((page) => page.url() === pageUrlBeforeDisconnect);
      return {
        status: matched ? 'pass' : 'fail',
        detail: `pages=${reconnectedPages.length} urls=${urls.join(',')}`,
      };
    });

    await results.run('SESSION_ISOLATION', 'One host exposes exactly one browser context', async () => ({
      status: browser.contexts().length === 1 ? 'pass' : 'fail',
      detail: `contexts=${browser.contexts().length}`,
    }));
  } finally {    try {
      await browser?.close();
    } catch {
      // A failed disconnect is reported by its own case instead of aborting the report.
    }
    // Closing the control connection is the authoritative "Core exited" signal for the host.
    // 关闭控制连接是宿主唯一权威的“Core 已退出”信号。
    try {
      control.end();
    } catch {
      // The host may already have exited.
    }
    await withTimeout(
      new Promise((resolve) => host.once('exit', resolve)),
      8000,
      'browser host exit',
    ).catch(() => {
      host.kill();
    });
    server.close();
  }

  return { results, isolatedRoot, hostDiagnostics, electronDistBytes, baseUrl, sessionId, hostGeneration };
}

/** Renders the human-readable compatibility report. */
function renderReport(report) {
  const lines = [];
  lines.push('# W02 browser runtime compatibility report');
  lines.push('');
  lines.push('Generated by `apps/vulcan-code-desktop/tools/w02-compatibility.mjs`.');
  lines.push('');
  lines.push('## Environment');
  lines.push('');
  lines.push(`- Platform: ${report.environment.platform} ${report.environment.architecture}`);
  lines.push(`- Operating system: ${report.environment.osRelease}`);
  lines.push(`- Electron runtime: ${report.environment.electronVersion}`);
  lines.push(`- Electron bundled Chromium: ${report.environment.chromiumVersion ?? '未确认'}`);
  lines.push(`- Electron bundled Node.js: ${report.environment.nodeVersion ?? '未确认'}`);
  lines.push(`- Playwright (playwright-core): ${report.environment.playwrightVersion}`);
  lines.push(`- Host Node.js: ${report.environment.hostNodeVersion}`);
  lines.push(`- Python: ${report.environment.pythonVersion}`);
  lines.push(`- Browser Use: ${report.environment.browserUseVersion}`);
  lines.push(`- Electron distribution bytes: ${report.environment.electronDistBytes}`);
  lines.push(`- Isolated root: ${report.environment.isolatedRoot}`);
  lines.push(`- Fixture base URL: ${report.environment.fixtureBaseUrl}`);
  lines.push('');
  lines.push('## Cases');
  lines.push('');
  lines.push('| Case | Behaviour | Status | Detail |');
  lines.push('| --- | --- | --- | --- |');
  for (const item of report.cases) {
    lines.push(`| ${item.id} | ${item.title} | ${item.status} | ${item.detail.replaceAll('|', '\\|')} |`);
  }
  lines.push('');
  lines.push('## Gate');
  lines.push('');
  lines.push(`- Gate result: ${report.gate.passed ? 'pass' : 'fail'}`);
  lines.push(
    `- Documented automation-library boundaries: ${report.gate.boundaryCaseIds.join(', ') || 'none'}`,
  );
  lines.push(
    `- Blocking failures: ${report.gate.blockingFailures.join(', ') || 'none'}`,
  );
  lines.push('');
  lines.push('## Host diagnostics');
  lines.push('');
  lines.push('```text');
  lines.push(report.hostDiagnostics.join('').slice(0, 4000));
  lines.push('```');
  lines.push('');
  return lines.join('\n');
}

/** Runs the harness entry point. */
async function main() {
  const options = {
    desktopRuntime: readRequiredArgument('desktop-runtime'),
    desktopEntry: readRequiredArgument('desktop-entry'),
    evidenceDirectory: readRequiredArgument('evidence-dir'),
  };
  await mkdir(options.evidenceDirectory, { recursive: true });
  const playwrightPackage = require('playwright-core/package.json');

  const outcome = await runCompatibilityExperiment(options);
  const implicitDownloads = await auditImplicitBrowserDownloads();
  outcome.results.record(
    'IMPLICIT_BROWSER_DOWNLOAD_AUDIT',
    'No automation library silently installed a second browser runtime',
    implicitDownloads.length === 0 ? 'pass' : 'fail',
    implicitDownloads.length === 0
      ? 'no playwright browser cache directory was found on this machine'
      : `unexpected browser caches: ${implicitDownloads.join(', ')}`,
  );

  const environment = {
    platform: process.platform,
    architecture: process.arch,
    osRelease: os.release(),
    electronVersion:
      (
        await readFile(path.join(path.dirname(options.desktopRuntime), '..', 'package.json'), 'utf8')
          .then((body) => JSON.parse(body))
          .catch(() => ({ version: '未确认' }))
      ).version ?? '未确认',
    chromiumVersion: await readElectronVersionField(options.desktopRuntime, 'chrome'),
    nodeVersion: await readElectronVersionField(options.desktopRuntime, 'node'),
    playwrightVersion: playwrightPackage.version,
    hostNodeVersion: process.version,
    pythonVersion: process.env.VULCAN_W02_PYTHON_VERSION ?? '未确认',
    browserUseVersion: process.env.VULCAN_W02_BROWSER_USE_VERSION ?? '未确认',
    electronDistBytes: outcome.electronDistBytes,
    isolatedRoot: outcome.isolatedRoot,
    fixtureBaseUrl: outcome.baseUrl,
  };

  const reportData = {
    environment,
    cases: outcome.results.cases,
    hostDiagnostics: outcome.hostDiagnostics,
    sessionId: outcome.sessionId,
    hostGeneration: outcome.hostGeneration,
  };

  reportData.gate = {
    boundaryCaseIds: [...AUTOMATION_LIBRARY_BOUNDARY_CASES],
    blockingFailures: outcome.results.cases
      .filter((item) => item.status === 'fail' && !AUTOMATION_LIBRARY_BOUNDARY_CASES.has(item.id))
      .map((item) => item.id),
  };
  reportData.gate.passed = reportData.gate.blockingFailures.length === 0;

  await writeFile(
    path.join(options.evidenceDirectory, 'w02-compatibility-evidence.json'),
    JSON.stringify(reportData, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(options.evidenceDirectory, 'w02-compatibility-report.md'),
    renderReport(reportData),
    'utf8',
  );

  const failures = outcome.results.cases.filter((item) => item.status === 'fail').length;
  const passes = outcome.results.cases.filter((item) => item.status === 'pass').length;
  process.stderr.write(
    `[w02] completed: ${passes} pass, ${failures} fail; gate=${reportData.gate.passed ? 'pass' : 'fail'}` +
      (reportData.gate.blockingFailures.length > 0
        ? ` blocking=${reportData.gate.blockingFailures.join(',')}`
        : ''),
  );
  process.stderr.write('\n');
  process.exitCode = reportData.gate.passed ? 0 : 1;
}

await main();
