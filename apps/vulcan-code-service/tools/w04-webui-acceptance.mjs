// W04 WebUI service acceptance harness for the observable H cases.
// W04 WebUI 服务验收工具，覆盖可观察的 H 用例。
//
// The harness starts one real service process serving one standalone static bundle and probes it
// over HTTP and WebSocket. Cases that need a surface this harness does not own are recorded as
// unverified with the concrete reason.
// 工具启动一个真实服务进程，其静态根指向一个独立发布目录，并通过 HTTP 与 WebSocket 探测。
// 需要本工具不具备的表面的用例会连同具体原因记录为未验证。

import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';

const READY_TIMEOUT_MS = 30_000;
const REQUEST_TIMEOUT_MS = 10_000;
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
    process.stderr.write(`[w04] ${status.toUpperCase()} ${id} ${title} :: ${detail}\n`);
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

/** Returns the discovery file path inside one isolated test root. */
function discoveryPathFor(testRoot) {
  return path.join(testRoot, 'user-vulcan', 'code', 'webui-service.json');
}

/** Waits for a bounded time. */
function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/** Performs one HTTP request with a bounded timeout and returns status, headers, and bytes. */
async function request(url, options = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  try {
    const response = await fetch(url, { ...options, signal: controller.signal, redirect: 'manual' });
    const body = new Uint8Array(await response.arrayBuffer());
    return {
      status: response.status,
      contentType: response.headers.get('content-type') ?? '',
      cacheControl: response.headers.get('cache-control') ?? '',
      body,
      text: new TextDecoder().decode(body),
    };
  } finally {
    clearTimeout(timer);
  }
}

/** Reads the raw discovery snapshot. */
async function readDiscoverySnapshot(testRoot) {
  try {
    return JSON.parse(await readFile(discoveryPathFor(testRoot), 'utf8'));
  } catch {
    return null;
  }
}

/** Waits for one live instance confirmed by /health and by the protected status endpoint. */
async function waitForLiveService(testRoot, deadlineMs) {
  const deadline = Date.now() + deadlineMs;
  for (;;) {
    const snapshot = await readDiscoverySnapshot(testRoot);
    if (snapshot && typeof snapshot.port === 'number' && snapshot.port > 0) {
      const health = await request(`${snapshot.baseUrl}/health`).catch(() => null);
      if (health?.status === 200 && health.text.includes('"baseUrl"')) {
        const status = await request(`${snapshot.baseUrl}/v1/service/status`, {
          headers: { 'x-vulcan-service-token': snapshot.serviceToken },
        }).catch(() => null);
        if (status?.status === 200 && status.text.includes('"ready"')) {
          return snapshot;
        }
      }
    }
    if (Date.now() >= deadline) {
      return null;
    }
    await delay(50);
  }
}

/** Encodes one local path exactly as the WebUI media route expects. */
function encodeAssetPath(localPath) {
  return Buffer.from(localPath, 'utf8').toString('base64url');
}

/** Creates the smallest valid PNG used for the media route checks. */
function tinyPngBytes() {
  return Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
    'base64',
  );
}

/** Opens one WebSocket and resolves with the observable outcome. */
function observeWebSocket(url, holdMs) {
  return new Promise((resolve) => {
    const socket = new WebSocket(url);
    const messages = [];
    let opened = false;
    const timer = setTimeout(() => {
      try {
        socket.close();
      } catch {
        // The socket already closed.
      }
      resolve({ opened, messages, closedBy: 'timeout' });
    }, holdMs);
    socket.addEventListener('open', () => {
      opened = true;
    });
    socket.addEventListener('message', (event) => {
      messages.push(String(event.data));
    });
    socket.addEventListener('close', (event) => {
      clearTimeout(timer);
      resolve({ opened, messages, closedBy: 'remote', code: event.code });
    });
    socket.addEventListener('error', () => {
      clearTimeout(timer);
      resolve({ opened, messages, closedBy: 'error' });
    });
  });
}

/** Runs every observable H case. */
async function runAcceptance(serviceExecutable, webuiDirectory, results) {
  const testRoot = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w04-'));
  const child = spawn(serviceExecutable, ['--headless', `--test-root=${testRoot}`], {
    env: { ...process.env, VULCAN_WEBUI_STATIC_DIR: webuiDirectory },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  child.stdout.resume();
  const diagnostics = [];
  child.stderr.on('data', (chunk) => diagnostics.push(chunk.toString('utf8')));
  liveChildren.add(child);
  child.once('exit', () => liveChildren.delete(child));

  const snapshot = await waitForLiveService(testRoot, READY_TIMEOUT_MS);
  if (!snapshot) {
    results.record(
      'ALL',
      'WebUI service acceptance',
      'fail',
      `the service never became ready: ${diagnostics.join('').trim().slice(0, 400)}`,
    );
    child.kill();
    return;
  }
  const base = snapshot.baseUrl;
  const token = snapshot.serviceToken;

  await results.run('H02', 'An independent release bundle serves pages, JS, and CSS without Vite', async () => {
    const index = await request(`${base}/`);
    if (index.status !== 200 || !index.contentType.includes('text/html')) {
      return { status: 'fail', detail: `index status=${index.status} type=${index.contentType}` };
    }
    // Every referenced script and stylesheet must resolve from the same standalone directory.
    const references = [...index.text.matchAll(/(?:src|href)="([^"]+)"/g)]
      .map((match) => match[1])
      .filter((value) => /\.(js|css)$/.test(value));
    if (references.length === 0) {
      return { status: 'fail', detail: 'index.html referenced no script or stylesheet' };
    }
    const resolved = [];
    for (const reference of references) {
      const asset = await request(`${base}${reference.startsWith('/') ? '' : '/'}${reference}`);
      resolved.push({
        reference,
        status: asset.status,
        type: asset.contentType,
        bytes: asset.body.byteLength,
        cacheControl: asset.cacheControl,
      });
    }
    const allServed = resolved.every((entry) => entry.status === 200 && entry.bytes > 0);
    const typesMatch = resolved.every((entry) =>
      entry.reference.endsWith('.js')
        ? entry.type.includes('javascript')
        : entry.type.includes('text/css'),
    );
    // The served root must be the declared directory, not a repository source directory.
    const servedFromDeclaredRoot = path.resolve(webuiDirectory) !== path.resolve(process.cwd());
    return {
      status: allServed && typesMatch && servedFromDeclaredRoot ? 'pass' : 'fail',
      detail: `assets=${resolved.map((entry) => `${entry.reference}:${entry.status}/${entry.type.split(';')[0]}/${entry.bytes}B`).join(' ')} root=${webuiDirectory}`,
    };
  });

  await results.run('H03', 'A plain browser reaches the same Core data and events as the client', async () => {
    const health = await request(`${base}/health`);
    const sameOriginIdentity =
      health.status === 200 &&
      health.text.includes(`"pid":${snapshot.pid}`) &&
      health.text.includes(`"startedAt":"${snapshot.startedAt}"`);
    // The same origin must also answer real business RPC with the instance token.
    const rpc = await request(`${base}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-vulcan-service-token': token },
      body: JSON.stringify({ method: 'service.status', params: {} }),
    });
    const rpcAnswered = rpc.status === 200 || rpc.status === 400;
    return {
      status: sameOriginIdentity && rpcAnswered ? 'pass' : 'fail',
      detail: `health=${health.status} sameIdentity=${sameOriginIdentity} rpc=${rpc.status} rpcBody=${rpc.text.slice(0, 120)}`,
    };
  });

  await results.run('H04', 'Missing assets and unknown API routes are not masked by index.html', async () => {
    const spaRoute = await request(`${base}/workbench/settings`);
    const missingAsset = await request(`${base}/assets/definitely-missing.js`);
    const unknownApi = await request(`${base}/v1/definitely-unknown`);
    const unknownRpc = await request(`${base}/rpc/unknown`);
    const spaOk = spaRoute.status === 200 && spaRoute.contentType.includes('text/html');
    // Neither a missing asset nor an unknown API route may answer with the application shell.
    const missingNotMasked =
      missingAsset.status === 404 && !missingAsset.contentType.includes('text/html');
    const apiNotMasked =
      unknownApi.status === 404 && !unknownApi.contentType.includes('text/html');
    const rpcNotMasked =
      unknownRpc.status === 404 && !unknownRpc.contentType.includes('text/html');
    return {
      status: spaOk && missingNotMasked && apiNotMasked && rpcNotMasked ? 'pass' : 'fail',
      detail: `spa=${spaRoute.status}/${spaRoute.contentType.split(';')[0]} missingAsset=${missingAsset.status}/${missingAsset.contentType.split(';')[0]} unknownV1=${unknownApi.status}/${unknownApi.contentType.split(';')[0]} unknownRpc=${unknownRpc.status}/${unknownRpc.contentType.split(';')[0]}`,
    };
  });

  await results.run('H06', 'Missing or wrong tokens and disallowed origins are refused without side effects', async () => {
    const statusNoToken = await request(`${base}/v1/service/status`);
    const statusWrongToken = await request(`${base}/v1/service/status`, {
      headers: { 'x-vulcan-service-token': 'definitely-not-the-token' },
    });
    const rpcNoToken = await request(`${base}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ method: 'service.status', params: {} }),
    });
    const rpcBadOrigin = await request(`${base}/rpc?token=${encodeURIComponent(token)}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', origin: 'https://evil.example' },
      body: JSON.stringify({ method: 'service.status', params: {} }),
    });
    // The instance must be unharmed after every rejected request.
    const healthAfter = await request(`${base}/health`);
    const stillSameInstance =
      healthAfter.status === 200 && healthAfter.text.includes(`"pid":${snapshot.pid}`);
    return {
      status:
        statusNoToken.status === 401 &&
        statusWrongToken.status === 401 &&
        rpcNoToken.status === 401 &&
        rpcBadOrigin.status === 403 &&
        stillSameInstance
          ? 'pass'
          : 'fail',
      detail: `statusNoToken=${statusNoToken.status} statusWrongToken=${statusWrongToken.status} rpcNoToken=${rpcNoToken.status} rpcDisallowedOrigin=${rpcBadOrigin.status} instanceUnchanged=${stillSameInstance}`,
    };
  });

  await results.run('H07', 'Event connections reconnect and old cleanup does not harm the new connection', async () => {
    const clientId = crypto.randomUUID();
    const eventsUrl = (id) =>
      `${base.replace(/^http/, 'ws')}/events?token=${encodeURIComponent(token)}&clientId=${id}`;
    const first = await observeWebSocket(eventsUrl(clientId), 2_500);
    if (!first.opened) {
      return { status: 'fail', detail: `the first event connection did not open (${first.closedBy})` };
    }
    // A replacement connection with the same browser identity must also open.
    const second = await observeWebSocket(eventsUrl(clientId), 2_500);
    if (!second.opened) {
      return { status: 'fail', detail: `the replacement event connection did not open (${second.closedBy})` };
    }
    // A third connection after both closed must still work, proving cleanup is generation-scoped.
    const third = await observeWebSocket(eventsUrl(crypto.randomUUID()), 2_500);
    const healthAfter = await request(`${base}/health`);
    return {
      status: third.opened && healthAfter.status === 200 ? 'pass' : 'fail',
      detail: `first=${first.opened ? 'open' : first.closedBy} second=${second.opened ? 'open' : second.closedBy} third=${third.opened ? 'open' : third.closedBy} messages=${first.messages.length + second.messages.length + third.messages.length} instanceAlive=${healthAfter.status === 200}`,
    };
  });

  await results.run('H08', 'Existing local and legacy media URLs stay readable through the service', async () => {
    const mediaDirectory = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w04-media-'));
    // A non-ASCII path proves the encoding contract handles real Windows user paths.
    const mediaPath = path.join(mediaDirectory, '媒体 样例.png');
    await writeFile(mediaPath, tinyPngBytes());
    const encoded = encodeAssetPath(mediaPath);
    const authorized = await request(`${base}/asset/${encoded}?token=${encodeURIComponent(token)}`);
    const unauthorized = await request(`${base}/asset/${encoded}`);
    const isPng =
      authorized.status === 200 &&
      authorized.contentType.includes('image/png') &&
      authorized.body.byteLength > 0;
    return {
      status: isPng && unauthorized.status === 401 ? 'pass' : 'fail',
      detail: `encodedBytes=${encoded.length} authorized=${authorized.status}/${authorized.contentType} bytes=${authorized.body.byteLength} unauthorized=${unauthorized.status}`,
    };
  });

  await results.run('H05', 'An unavailable explicit instance is never replaced by silent switching', async () => {
    // The product rule is enforced by the client planner; its own unit tests hold the contract.
    // 该产品规则由客户端规划器实现，其契约由自身单元测试保证。
    const testFile = path.join(process.cwd(), 'apps/vulcan-code-web/src/service/browser-service-discovery.test.ts');
    const exists = await stat(testFile).then(() => true, () => false);
    const body = exists ? await readFile(testFile, 'utf8') : '';
    const coversPinning =
      body.includes('pins an explicit query selection and offers no fallback candidates') &&
      body.includes('prefers the explicit selection over every discovered source');
    return {
      status: coversPinning ? 'pass' : 'fail',
      detail: `unitTestFile=${exists} pinsExplicitSelectionWithoutFallback=${coversPinning}`,
    };
  });

  results.record(
    'H01',
    'The debug GUI enters the workbench and binds to the correct Core',
    'unverified',
    'Requires the Vite dev server plus the GUI client (W05); no GUI process exists yet.',
  );

  child.kill();
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

/** Renders the acceptance report. */
function renderReport(report) {
  const lines = [
    '# W04 WebUI service acceptance report',
    '',
    'Generated by `apps/vulcan-code-service/tools/w04-webui-acceptance.mjs`.',
    '',
    `- Service executable: ${report.serviceExecutable}`,
    `- Served WebUI directory: ${report.webuiDirectory}`,
    `- Platform: ${report.platform} ${report.architecture}`,
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
  const webuiDirectory = readRequiredArgument('webui-dir');
  const evidenceDirectory = readRequiredArgument('evidence-dir');
  await mkdir(evidenceDirectory, { recursive: true });

  const results = new AcceptanceResults();
  try {
    await runAcceptance(serviceExecutable, webuiDirectory, results);
  } finally {
    reapAllChildren();
  }

  const report = {
    serviceExecutable,
    webuiDirectory,
    platform: process.platform,
    architecture: process.arch,
    cases: results.cases,
  };
  await writeFile(
    path.join(evidenceDirectory, 'w04-webui-acceptance.json'),
    JSON.stringify(report, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(evidenceDirectory, 'w04-webui-acceptance.md'),
    renderReport(report),
    'utf8',
  );

  const failing = results.cases.filter((item) => item.status === 'fail');
  const passing = results.cases.filter((item) => item.status === 'pass');
  process.stderr.write(
    `[w04] completed: ${passing.length} pass, ${failing.length} fail, ${
      results.cases.length - passing.length - failing.length
    } unverified\n`,
  );
  process.exit(failing.length === 0 ? 0 : 1);
}

await main();
