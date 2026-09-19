// W03 service acceptance harness: controlled launcher for the runnable S cases.
// W03 服务验收工具：用于可运行 S 用例的受控启动器。
//
// Every case runs real service processes against one isolated test root created by this script.
// The real user data directory is never touched, and no case fakes a pass: a case that cannot be
// observed is recorded as unverified with the reason.
// 每个用例都针对本脚本创建的隔离测试根运行真实服务进程。绝不触碰真实用户数据目录，
// 也不伪造通过：无法观察到结果的用例会连同原因记录为未验证。

import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';

const HEALTH_TIMEOUT_MS = 2_000;
const READY_TIMEOUT_MS = 30_000;
const EXIT_TIMEOUT_MS = 20_000;

// Every spawned service is tracked so the harness can always reap children before exiting.
// 追踪每一个已启动的服务，使工具在退出前总能回收子进程。
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
    process.stderr.write(`[w03] ${status.toUpperCase()} ${id} ${title} :: ${detail}\n`);
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

/** Returns the service lock file path inside one isolated test root. */
function lockPathFor(testRoot) {
  return path.join(testRoot, 'user-vulcan', 'code', 'service.lock');
}

/** Spawns one service process against an isolated root with fully drained output pipes. */
function spawnService(serviceExecutable, testRoot, args, environment = {}) {
  const child = spawn(serviceExecutable, [...args, `--test-root=${testRoot}`], {
    env: { ...process.env, ...environment },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  // Both pipes must be drained, otherwise a full pipe buffer blocks the service process.
  // 两条管道都必须排空，否则管道缓冲写满会阻塞服务进程。
  child.stdout.resume();
  liveChildren.add(child);
  child.once('exit', () => liveChildren.delete(child));
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

/** Waits for a bounded time. */
function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/** Waits for one live instance, confirmed by /health, ignoring identities that must not be trusted. */
async function waitForLiveService(testRoot, deadlineMs, ignoredIdentities = []) {
  const deadline = Date.now() + deadlineMs;
  let lastObservation = 'no discovery file was readable';
  for (;;) {
    try {
      const snapshot = JSON.parse(await readFile(discoveryPathFor(testRoot), 'utf8'));
      if (snapshot && typeof snapshot.port === 'number' && snapshot.port > 0) {
        // A discovery file is only a claim: the published identity must also answer /health.
        // 发现文件只是一份声明：已发布身份还必须能通过 /health 应答。
        const ignored = ignoredIdentities.some(
          (identity) => identity.pid === snapshot.pid || identity.serviceToken === snapshot.serviceToken,
        );
        if (!ignored) {
          const health = await probeHealth(snapshot.baseUrl);
          if (health.ok && health.body?.pid === snapshot.pid) {
            return { snapshot, health, diagnostics: lastObservation };
          }
          lastObservation = `published pid=${snapshot.pid} did not answer /health (status=${health.status})`;
        } else {
          lastObservation = `ignored stale identity pid=${snapshot.pid}`;
        }
      }
    } catch (error) {
      lastObservation = `discovery read failed: ${String(error?.message ?? error)}`;
    }
    if (Date.now() >= deadline) {
      return null;
    }
    await delay(50);
  }
}

/** Queries /health with a bounded timeout. */
async function probeHealth(baseUrl) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), HEALTH_TIMEOUT_MS);
  try {
    const response = await fetch(`${baseUrl}/health`, { signal: controller.signal });
    return { ok: response.ok, status: response.status, body: await response.json() };
  } catch (error) {
    return { ok: false, status: 0, error: String(error?.message ?? error) };
  } finally {
    clearTimeout(timer);
  }
}

/** Returns whether a path exists using metadata only.
 *
 * Reading the lock file's contents is not a valid existence check: the owner holds an exclusive
 * operating-system lock on it, so opening it for reading fails while the lock is held.
 * 仅通过元数据判断路径是否存在。读取锁文件内容不是有效的存在性检查：所有者对其持有操作系统
 * 排他锁，因此持锁期间以读取方式打开会失败。
 */
async function pathExists(candidate) {
  try {
    await stat(candidate);
    return true;
  } catch {
    return false;
  }
}

/** Waits until the protected status endpoint reports one lifecycle, recording the transitions. */
async function waitForLifecycle(baseUrl, serviceToken, deadlineMs) {
  const deadline = Date.now() + deadlineMs;
  const observed = [];
  for (;;) {
    const probe = await probeServiceStatus(baseUrl, serviceToken);
    const lifecycle = probe.body?.lifecycle;
    if (typeof lifecycle === 'string' && observed.at(-1) !== lifecycle) {
      observed.push(lifecycle);
    }
    if (lifecycle === 'ready' || lifecycle === 'failed') {
      return { lifecycle, observed, probe };
    }
    if (Date.now() >= deadline) {
      return { lifecycle: lifecycle ?? 'unavailable', observed, probe };
    }
    await delay(50);
  }
}

/** Reads the raw discovery snapshot without confirming liveness. */
async function readDiscoverySnapshot(testRoot) {
  try {
    return JSON.parse(await readFile(discoveryPathFor(testRoot), 'utf8'));
  } catch {
    return null;
  }
}

/** Queries the protected detailed status endpoint with the discovery token. */
async function probeServiceStatus(baseUrl, serviceToken) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), HEALTH_TIMEOUT_MS);
  try {
    const response = await fetch(`${baseUrl}/v1/service/status`, {
      signal: controller.signal,
      headers: { 'x-vulcan-service-token': serviceToken },
    });
    if (!response.ok) {
      return { ok: false, status: response.status };
    }
    return { ok: true, status: response.status, body: await response.json() };
  } catch (error) {
    return { ok: false, status: 0, error: String(error?.message ?? error) };
  } finally {
    clearTimeout(timer);
  }
}

/** Waits for one child to exit and reports its code. */
function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null) {
    return Promise.resolve(child.exitCode);
  }
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), timeoutMs);
    child.once('exit', (code) => {
      clearTimeout(timer);
      resolve(code);
    });
  });
}

/** Starts one isolated service and returns its handles plus confirmed identity. */
async function startIsolatedService(serviceExecutable, isolatedRoot, args = ['--headless'], ignoredIdentities = []) {
  const child = spawnService(serviceExecutable, isolatedRoot, args);
  const diagnostics = [];
  child.stderr.on('data', (chunk) => diagnostics.push(chunk.toString('utf8')));
  const live = await waitForLiveService(isolatedRoot, READY_TIMEOUT_MS, ignoredIdentities);
  return {
    child,
    diagnostics,
    snapshot: live?.snapshot ?? null,
    health: live?.health ?? null,
  };
}

/** Runs every runnable S case and returns the collected results. */
async function runAcceptance(serviceExecutable, results, evidenceDirectory) {
  // S01: one launcher against an empty isolated root starts exactly one service.
  const s01Root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w03-s01-'));
  await mkdir(s01Root, { recursive: true });
  let s01 = null;
  await results.run('S01', 'One launcher starts one service and reaches a ready identity', async () => {
    s01 = await startIsolatedService(serviceExecutable, s01Root);
    if (!s01.child || s01.child.exitCode !== null) {
      return { status: 'fail', detail: `service exited early: ${s01.diagnostics.join('').trim()}` };
    }
    if (!s01.snapshot) {
      return { status: 'fail', detail: 'no live discovery identity was published' };
    }
    const health = s01.health ?? (await probeHealth(s01.snapshot.baseUrl));
    // The detailed lifecycle must be readable with the published token and unusable without it.
    // 详细生命周期必须能用已发布令牌读取，且在无令牌时不可用。
    const authorized = await probeServiceStatus(s01.snapshot.baseUrl, s01.snapshot.serviceToken);
    const unauthorized = await probeServiceStatus(s01.snapshot.baseUrl, 'not-the-real-token');
    // Readiness is only proven once the protected lifecycle reaches ready, not when /health binds.
    // 只有当受保护生命周期到达 ready 时才证明就绪，而不是 /health 一开始监听就证明。
    const readiness = await waitForLifecycle(
      s01.snapshot.baseUrl,
      s01.snapshot.serviceToken,
      READY_TIMEOUT_MS,
    );
    return {
      status:
        health.ok && authorized.ok && readiness.lifecycle === 'ready' && !unauthorized.ok
          ? 'pass'
          : 'fail',
      detail: `pid=${s01.snapshot.pid} port=${s01.snapshot.port} health=${health.status} statusWithoutToken=${unauthorized.status} lifecycleSequence=${readiness.observed.join('->')}`,
    };
  });

  // S03: a second launcher during startup must reuse the same instance identity.
  await results.run('S03', 'A second launcher reports the original instance instead of starting new work', async () => {
    if (!s01?.snapshot) {
      return { status: 'fail', detail: 'S01 did not publish an instance to compare against' };
    }
    const second = spawnService(serviceExecutable, s01Root, ['--headless']);
    const secondDiagnostics = [];
    second.stderr.on('data', (chunk) => secondDiagnostics.push(chunk.toString('utf8')));
    const secondExit = await waitForExit(second, EXIT_TIMEOUT_MS);
    if (secondExit === null) {
      second.kill();
      return { status: 'fail', detail: 'the second launcher stayed alive instead of yielding' };
    }
    const health = await probeHealth(s01.snapshot.baseUrl);
    const sameIdentity = health.ok && health.body?.pid === s01.snapshot.pid;
    return {
      status: secondExit === 0 && sameIdentity ? 'pass' : 'fail',
      detail: `secondExitCode=${secondExit} originalPid=${s01.snapshot.pid} observedPid=${health.body?.pid} stderr=${secondDiagnostics.join('').trim()}`,
    };
  });

  // S02: eight concurrent launchers must produce exactly one owner.
  await results.run('S02', 'Eight concurrent launchers initialize exactly one service', async () => {
    const s02Root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w03-s02-'));
    const launchers = Array.from({ length: 8 }, () =>
      spawnService(serviceExecutable, s02Root, ['--headless']),
    );
    const diagnostics = launchers.map(() => []);
    launchers.forEach((child, index) => {
      child.stderr.on('data', (chunk) => diagnostics[index].push(chunk.toString('utf8')));
    });
    const exitCodes = await Promise.all(launchers.map((child) => waitForExit(child, EXIT_TIMEOUT_MS)));
    const owners = launchers.filter((child, index) => exitCodes[index] === null);
    const live = await waitForLiveService(s02Root, 10_000);
    const snapshot = live?.snapshot ?? null;
    // Exactly one process must survive as the owner, and every loser must exit successfully.
    const losersExitedCleanly = launchers.every(
      (child, index) => exitCodes[index] === null || exitCodes[index] === 0,
    );
    for (const child of owners) {
      child.kill();
    }
    return {
      status: owners.length === 1 && losersExitedCleanly && snapshot ? 'pass' : 'fail',
      detail: `survivors=${owners.length} exitCodes=${exitCodes.join(',')} loserStderr=${diagnostics
        .map((lines, index) => (exitCodes[index] === 0 ? lines.join('').trim() : ''))
        .filter(Boolean)
        .slice(0, 1)
        .join('')}`,
    };
  });

  // S04: an occupied explicit port must fail loudly and must not publish a ready instance.
  await results.run('S04', 'An occupied service port fails explicitly without publishing ready', async () => {
    const s04Root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w03-s04-'));
    // The blocker occupies the exact port the service is told to use.
    const { createServer } = await import('node:net');
    const blocker = createServer();
    await new Promise((resolve) => blocker.listen(0, '127.0.0.1', resolve));
    const occupiedPort = blocker.address().port;
    const child = spawnService(serviceExecutable, s04Root, ['--headless'], {
      VULCAN_WEBUI_SERVICE_PORT: String(occupiedPort),
    });
    const diagnostics = [];
    child.stderr.on('data', (chunk) => diagnostics.push(chunk.toString('utf8')));
    const exitCode = await waitForExit(child, 30_000);
    if (exitCode === null) {
      child.kill();
    }
    blocker.close();
    const snapshot = await readDiscoverySnapshot(s04Root);
    const publishedReady = snapshot?.status === 'ready';
    return {
      status: exitCode !== 0 && exitCode !== null && !publishedReady ? 'pass' : 'fail',
      detail: `occupiedPort=${occupiedPort} exitCode=${exitCode} publishedReady=${publishedReady} stderr=${diagnostics.join('').trim().slice(0, 300)}`,
    };
  });

  // S06: a stale discovery file with a foreign identity must never be trusted.
  await results.run('S06', 'A stale discovery file is not mistaken for the live instance', async () => {
    const s06Root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w03-s06-'));
    const staleDirectory = path.join(s06Root, 'user-vulcan', 'code');
    await mkdir(staleDirectory, { recursive: true });
    const stale = {
      status: 'ready',
      baseUrl: 'http://127.0.0.1:1',
      host: '127.0.0.1',
      port: 1,
      pid: 999_999,
      startedAt: '1',
      serviceToken: 'stale-token-value',
    };
    await writeFile(discoveryPathFor(s06Root), JSON.stringify(stale), 'utf8');
    const service = await startIsolatedService(serviceExecutable, s06Root, ['--headless'], [
      { pid: stale.pid, serviceToken: stale.serviceToken },
    ]);
    if (!service.snapshot || service.child.exitCode !== null) {
      service.child?.kill();
      return {
        status: 'fail',
        detail: `the live service was not distinguishable from the stale file: ${service.diagnostics.join('').trim()}`,
      };
    }
    const replaced =
      service.snapshot.pid !== stale.pid && service.snapshot.serviceToken !== stale.serviceToken;
    service.child.kill();
    return {
      status: replaced && service.health?.ok ? 'pass' : 'fail',
      detail: `stalePid=${stale.pid} livePid=${service.snapshot.pid} tokenReplaced=${
        service.snapshot.serviceToken !== stale.serviceToken
      } health=${service.health?.status}`,
    };
  });

  // S11: after the owner dies the lock is released and a new process becomes the sole owner.
  await results.run('S11', 'The lock is released after an abnormal owner exit and a new owner starts', async () => {
    const s11Root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-w03-s11-'));
    const first = await startIsolatedService(serviceExecutable, s11Root);
    if (!first.snapshot) {
      first.child?.kill();
      return { status: 'fail', detail: 'the first owner never published an identity' };
    }
    const firstPid = first.snapshot.pid;
    const firstIdentity = {
      pid: first.snapshot.pid,
      serviceToken: first.snapshot.serviceToken,
    };
    first.child.kill('SIGKILL');
    await waitForExit(first.child, EXIT_TIMEOUT_MS);
    // The stale discovery file from the killed owner must never be treated as the new instance.
    // 已被终止所有者留下的过期发现文件绝不能被当作新实例。
    const second = await startIsolatedService(serviceExecutable, s11Root, ['--headless'], [firstIdentity]);
    if (!second.snapshot) {
      second.child?.kill();
      return {
        status: 'fail',
        detail: `the new owner could not start: ${second.diagnostics.join('').trim()}`,
      };
    }
    const distinctIdentity = second.snapshot.pid !== firstPid;
    const lockStillPresent = await pathExists(lockPathFor(s11Root));
    second.child.kill();
    return {
      status: distinctIdentity && second.health?.ok && lockStillPresent ? 'pass' : 'fail',
      detail: `firstPid=${firstPid} secondPid=${second.snapshot.pid} health=${second.health?.status} lockFileRetained=${lockStillPresent}`,
    };
  });

  // Cases that need surfaces this harness does not own are recorded honestly.
  results.record(
    'S05',
    'An unwritable discovery directory fails startup explicitly',
    'unverified',
    'Requires an access-denied discovery directory; not simulated in this run.',
  );
  results.record(
    'S07',
    'Closing or terminating the GUI leaves Core, tasks, and browsers running',
    'unverified',
    'The Electron GUI does not exist yet (W05); no GUI process could be closed.',
  );
  results.record(
    'S08',
    'Tray quit drains and then exits with no orphaned browser host',
    'unverified',
    'Requires tray menu interaction and a browser host (W03 menu exists, W06 host absent).',
  );
  results.record(
    'S09',
    'An injected durable drain failure keeps the process alive and reopens admission',
    'unverified',
    'Requires a fault-injection seam for the drain coordinator; not implemented yet.',
  );
  results.record(
    'S10',
    'An HTTP bridge that dies after readiness leaves ready and reports the failure',
    'unverified',
    'Requires killing only the bridge thread inside the live process; not externally reachable.',
  );
  results.record(
    'S12',
    'A no-desktop build initializes no graphical loop and exits normally',
    'unverified',
    'Requires a separate --no-default-features binary build; deferred to W10 packaging.',
  );
  void evidenceDirectory;
}

/** Renders the acceptance report. */
function renderReport(report) {
  const lines = [
    '# W03 service acceptance report',
    '',
    'Generated by `apps/vulcan-code-service/tools/w03-service-acceptance.mjs`.',
    '',
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
  const evidenceDirectory = readRequiredArgument('evidence-dir');
  await mkdir(evidenceDirectory, { recursive: true });

  const results = new AcceptanceResults();
  try {
    await runAcceptance(serviceExecutable, results, evidenceDirectory);
  } finally {
    reapAllChildren();
  }

  const report = {
    serviceExecutable,
    platform: process.platform,
    architecture: process.arch,
    nodeVersion: process.version,
    cases: results.cases,
  };
  await writeFile(
    path.join(evidenceDirectory, 'w03-service-acceptance.json'),
    JSON.stringify(report, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(evidenceDirectory, 'w03-service-acceptance.md'),
    renderReport(report),
    'utf8',
  );

  const failing = results.cases.filter((item) => item.status === 'fail');
  const passing = results.cases.filter((item) => item.status === 'pass');
  process.stderr.write(
    `[w03] completed: ${passing.length} pass, ${failing.length} fail, ${
      results.cases.length - passing.length - failing.length
    } unverified\n`,
  );
  // Explicit exit avoids waiting on any lingering handle after every child was reaped.
  // 显式退出，避免在所有子进程回收后仍等待残留句柄。
  process.exit(failing.length === 0 ? 0 : 1);
}

await main();
