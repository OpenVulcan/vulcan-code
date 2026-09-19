// Exercises real packaged Chromium-host cleanup after startup and authentication failures.
// 验证真实打包 Chromium 宿主在启动及鉴权失败后的清理行为。
import assert from 'node:assert/strict';
import { execFile as execFileCallback, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { promisify } from 'node:util';

// execFile runs read-only Windows process inspection without opening a visible console.
// execFile 在不显示控制台窗口的情况下执行只读 Windows 进程检查。
const execFile = promisify(execFileCallback);
// DISCOVERY_FILENAME is the host control endpoint document published after Chromium starts.
// DISCOVERY_FILENAME 是 Chromium 启动后由宿主发布的控制端点文档。
const DISCOVERY_FILENAME = 'browser-host-control.json';
// HOST_EXIT_TIMEOUT_MS bounds every disposable host process used by this acceptance test.
// HOST_EXIT_TIMEOUT_MS 限制本验收中每个一次性宿主进程的最长存活时间。
const HOST_EXIT_TIMEOUT_MS = 20_000;
// AUTH_REJECTION_TIMEOUT_MS bounds the invalid-owner socket before process cleanup begins.
// AUTH_REJECTION_TIMEOUT_MS 限制错误所有者套接字等待时间，之后进入进程清理。
const AUTH_REJECTION_TIMEOUT_MS = 5_000;

/** Returns one required command-line argument value.
 * 返回一个必需的命令行参数值。
 * @param {string} name Argument name without leading dashes. / 不带前导横线的参数名。
 * @returns {string} Nonempty argument value. / 非空参数值。
 */
function requiredArgument(name) {
  // value is the exact text following the named argument prefix.
  // value 是具名参数前缀后的精确文本。
  const value = process.argv.find(argument => argument.startsWith(`--${name}=`))?.slice(name.length + 3);
  if (!value) throw new Error(`Missing --${name}=<value>.`);
  return value;
}

/** Waits until one file can be read and returns its UTF-8 body.
 * 等待一个文件可读并返回其 UTF-8 正文。
 * @param {string} filePath Absolute file path. / 文件绝对路径。
 * @param {number} timeoutMs Maximum wait in milliseconds. / 最长等待毫秒数。
 * @returns {Promise<string>} File body. / 文件正文。
 */
async function waitForFile(filePath, timeoutMs) {
  // deadline prevents a failed host from leaving the test waiting indefinitely.
  // deadline 防止宿主失败后测试无限等待。
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      return await readFile(filePath, 'utf8');
    } catch {
      await new Promise(resolve => setTimeout(resolve, 50));
    }
  }
  throw new Error(`Timed out waiting for ${filePath}.`);
}

/** Encodes one JSON value using the host's four-byte big-endian frame protocol.
 * 使用宿主的四字节大端长度帧协议编码一个 JSON 值。
 * @param {unknown} value JSON-compatible value. / 可转换为 JSON 的值。
 * @returns {Buffer} Encoded control frame. / 已编码控制帧。
 */
function encodeFrame(value) {
  // body is the one UTF-8 JSON document carried by this frame.
  // body 是该帧承载的一份 UTF-8 JSON 文档。
  const body = Buffer.from(JSON.stringify(value), 'utf8');
  // frame reserves four bytes before the JSON body for its unsigned length.
  // frame 在 JSON 正文前预留四字节无符号长度。
  const frame = Buffer.allocUnsafe(body.length + 4);
  frame.writeUInt32BE(body.length, 0);
  body.copy(frame, 4);
  return frame;
}

/** Waits for one child process to exit within a fixed deadline.
 * 在固定期限内等待一个子进程退出。
 * @param {import('node:child_process').ChildProcess} child Spawned process. / 已启动进程。
 * @param {number} timeoutMs Maximum wait in milliseconds. / 最长等待毫秒数。
 * @returns {Promise<{code: number|null, signal: NodeJS.Signals|null}>} Exit result. / 退出结果。
 */
function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve({ code: child.exitCode, signal: child.signalCode });
  }
  return new Promise((resolve, reject) => {
    // timeout fails the acceptance instead of hiding a stranded browser behind an endless wait.
    // timeout 让验收明确失败，而不是用无限等待掩盖残留浏览器。
    const timeout = setTimeout(() => reject(new Error(`Host ${child.pid ?? 'unknown'} did not exit.`)), timeoutMs);
    child.once('error', error => {
      clearTimeout(timeout);
      reject(error);
    });
    child.once('exit', (code, signal) => {
      clearTimeout(timeout);
      resolve({ code, signal });
    });
  });
}

/** Terminates one disposable Windows process tree after a failed acceptance run.
 * 在验收失败后终止一个一次性 Windows 进程树。
 * @param {number|undefined} pid Root process identifier. / 根进程标识。
 * @returns {Promise<void>} Completion after best-effort cleanup. / 尽力清理后的完成信号。
 */
async function terminateProcessTree(pid) {
  if (!pid || process.platform !== 'win32') return;
  try {
    await execFile('taskkill.exe', ['/PID', String(pid), '/T', '/F'], { windowsHide: true });
  } catch {
    // A process that already exited needs no further cleanup.
    // 已退出的进程无需继续清理。
  }
}

/** Terminates any failed-test process that still references one generated profile.
 * 终止失败测试后仍引用某个生成画像的进程。
 * @param {string} profilePath Absolute profile path. / 画像绝对路径。
 * @returns {Promise<void>} Completion after every matching process was handled. / 处理全部匹配进程后的完成信号。
 */
async function terminateProfileProcesses(profilePath) {
  // processIds contains only processes tied to this test's unique generated profile path.
  // processIds 只包含绑定到本测试唯一生成画像路径的进程。
  const processIds = await profileProcessIds(profilePath);
  await Promise.all(processIds.map(processId => terminateProcessTree(processId)));
}

/** Returns Windows processes whose command line still references one isolated profile.
 * 返回命令行仍引用某个隔离画像的 Windows 进程。
 * @param {string} profilePath Absolute profile path. / 画像绝对路径。
 * @returns {Promise<number[]>} Matching process identifiers. / 匹配的进程标识列表。
 */
async function profileProcessIds(profilePath) {
  assert.equal(process.platform, 'win32', 'This real-process acceptance currently targets Windows.');
  // command is static so the generated profile path never enters PowerShell source text.
  // command 为静态文本，因此生成的画像路径绝不会进入 PowerShell 源码。
  const command = 'Get-CimInstance Win32_Process | Select-Object ProcessId,CommandLine | ConvertTo-Json -Compress -Depth 2';
  // stdout contains a process snapshot used only inside this test and is never persisted.
  // stdout 包含仅在本测试内使用且不会持久化的进程快照。
  const { stdout } = await execFile('powershell.exe', ['-NoProfile', '-Command', command], {
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
  });
  // parsed normalizes PowerShell's one-object and array JSON shapes.
  // parsed 统一 PowerShell 单对象与数组两种 JSON 形态。
  const parsed = JSON.parse(stdout || '[]');
  // processes is the normalized process snapshot.
  // processes 是统一后的进程快照。
  const processes = Array.isArray(parsed) ? parsed : [parsed];
  // needle matches the exact generated profile path case-insensitively.
  // needle 以不区分大小写方式匹配精确生成的画像路径。
  const needle = profilePath.toLowerCase();
  return processes
    .filter(entry => typeof entry.CommandLine === 'string' && entry.CommandLine.toLowerCase().includes(needle))
    .map(entry => Number(entry.ProcessId))
    .filter(Number.isInteger);
}

/** Confirms no host or Chromium process retains one isolated profile.
 * 确认没有宿主或 Chromium 进程继续占用某个隔离画像。
 * @param {string} profilePath Absolute profile path. / 画像绝对路径。
 * @param {number} timeoutMs Maximum cleanup wait. / 最长清理等待时间。
 * @returns {Promise<void>} Completion when no process remains. / 无残留进程时完成。
 */
async function waitForProfileRelease(profilePath, timeoutMs) {
  // deadline allows short Windows process-table propagation without accepting a lasting orphan.
  // deadline 允许 Windows 进程表短暂传播延迟，但不接受长期孤儿进程。
  const deadline = Date.now() + timeoutMs;
  // remaining retains the latest matching process identifiers for a useful failure message.
  // remaining 保留最近一次匹配的进程标识，供失败信息使用。
  let remaining = [];
  while (Date.now() < deadline) {
    remaining = await profileProcessIds(profilePath);
    if (remaining.length === 0) return;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`Profile remained in use by process IDs: ${remaining.join(', ')}.`);
}

/** Resolves and validates the exact packaged runtime paths used by the host.
 * 解析并校验宿主使用的精确打包运行时路径。
 * @param {string} runtimeRoot Distribution root. / 发行目录根。
 * @returns {Promise<{desktopExecutable:string,browserHostEntry:string,chromiumExecutable:string}>} Runtime paths. / 运行时路径。
 */
async function loadRuntime(runtimeRoot) {
  // layout is the package's authoritative relative-path manifest.
  // layout 是打包目录的权威相对路径清单。
  const layout = JSON.parse(await readFile(path.join(runtimeRoot, 'runtime-layout.json'), 'utf8'));
  return {
    desktopExecutable: path.resolve(runtimeRoot, layout.desktopExecutable),
    browserHostEntry: path.resolve(runtimeRoot, layout.browserHostEntry),
    chromiumExecutable: path.resolve(runtimeRoot, layout.chromiumExecutable),
  };
}

/** Launches one real packaged Chromium host in an isolated directory.
 * 在隔离目录内启动一个真实打包 Chromium 宿主。
 * @param {{desktopExecutable:string,browserHostEntry:string,chromiumExecutable:string}} runtime Runtime paths. / 运行时路径。
 * @param {string} root Isolated scenario root. / 隔离场景根。
 * @param {string} initialUrl Initial browser URL. / 浏览器初始地址。
 * @returns {{child:import('node:child_process').ChildProcess,profile:string,control:string,stderr:()=>string}} Host handle. / 宿主句柄。
 */
function launchHost(runtime, root, initialUrl) {
  // profile is the only Chromium user-data directory for this scenario.
  // profile 是该场景唯一的 Chromium 用户数据目录。
  const profile = path.join(root, 'profile');
  // control is the isolated directory where the host publishes its endpoint.
  // control 是宿主发布端点的隔离目录。
  const control = path.join(root, 'control');
  // sessionId gives the disposable host a unique Core-style identity.
  // sessionId 为一次性宿主提供唯一的 Core 风格身份。
  const sessionId = randomUUID();
  // child is the packaged Electron runtime operating strictly in Node host mode.
  // child 是严格以 Node 宿主模式运行的打包 Electron 运行时。
  const child = spawn(runtime.desktopExecutable, [
    runtime.browserHostEntry,
    `--session-id=${sessionId}`,
    '--host-generation=1',
    `--user-data-dir=${profile}`,
    `--control-dir=${control}`,
    `--control-secret=${randomUUID()}`,
    `--chromium-executable=${runtime.chromiumExecutable}`,
    `--initial-url=${initialUrl}`,
  ], {
    env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' },
    stdio: ['ignore', 'ignore', 'pipe'],
    windowsHide: true,
  });
  // diagnosticText retains bounded host diagnostics for assertions without exposing arguments.
  // diagnosticText 保留有界宿主诊断供断言使用，且不暴露启动参数。
  let diagnosticText = '';
  child.stderr?.on('data', chunk => {
    diagnosticText = `${diagnosticText}${chunk.toString('utf8')}`.slice(-64 * 1024);
  });
  return { child, profile, control, stderr: () => diagnosticText };
}

/** Verifies a navigation failure after Chromium launch leaves no browser process.
 * 验证 Chromium 启动后的导航失败不会遗留浏览器进程。
 * @param {ReturnType<typeof loadRuntime> extends Promise<infer T> ? T : never} runtime Runtime paths. / 运行时路径。
 * @returns {Promise<object>} Scenario evidence. / 场景证据。
 */
async function verifyStartupFailureCleanup(runtime) {
  // root contains all disposable files for this scenario.
  // root 包含该场景的全部一次性文件。
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-host-startup-fault-'));
  // requestCount proves a real Chromium renderer reached the deliberately broken page.
  // requestCount 证明真实 Chromium 渲染器访问了故意损坏的页面。
  let requestCount = 0;
  // server accepts navigation and then resets it without sending a response.
  // server 接受导航后立即重置连接且不发送响应。
  const server = http.createServer(request => {
    requestCount += 1;
    request.socket.destroy();
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  // address provides the exact ephemeral port selected by Windows.
  // address 提供 Windows 选择的精确临时端口。
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  // host is the real packaged browser host under test.
  // host 是本场景接受测试的真实打包浏览器宿主。
  const host = launchHost(runtime, root, `http://127.0.0.1:${address.port}/broken`);
  try {
    // exit is expected to report startup failure after the renderer request is reset.
    // exit 预期在渲染器请求被重置后报告启动失败。
    const exit = await waitForExit(host.child, HOST_EXIT_TIMEOUT_MS);
    assert.equal(exit.code, 1);
    assert.ok(requestCount >= 1, 'Real Chromium never reached the broken startup page.');
    assert.match(host.stderr(), /startup failed:/);
    await waitForProfileRelease(host.profile, 5_000);
    return { exitCode: exit.code, rendererRequests: requestCount, orphanProcessIds: [] };
  } finally {
    await new Promise(resolve => server.close(resolve));
    if (host.child.exitCode === null && host.child.signalCode === null) {
      await terminateProcessTree(host.child.pid);
    }
    await terminateProfileProcesses(host.profile);
    await rm(root, { recursive: true, force: true });
  }
}

/** Verifies an invalid Core hello closes the authenticated channel and Chromium tree.
 * 验证无效 Core hello 会关闭鉴权通道及 Chromium 进程树。
 * @param {ReturnType<typeof loadRuntime> extends Promise<infer T> ? T : never} runtime Runtime paths. / 运行时路径。
 * @returns {Promise<object>} Scenario evidence. / 场景证据。
 */
async function verifyAuthenticationFailureCleanup(runtime) {
  // root contains all disposable files for this scenario.
  // root 包含该场景的全部一次性文件。
  const root = await mkdtemp(path.join(os.tmpdir(), 'vulcan-host-auth-fault-'));
  // host is a fully started real browser waiting for its owning Core handshake.
  // host 是已完整启动并等待所属 Core 握手的真实浏览器。
  const host = launchHost(runtime, root, 'about:blank');
  try {
    // discovery is the endpoint published only after the Chromium context exists.
    // discovery 是仅在 Chromium 上下文存在后才发布的端点。
    const discovery = JSON.parse(await waitForFile(path.join(host.control, DISCOVERY_FILENAME), 15_000));
    assert.equal(discovery.pid, host.child.pid);
    // socket deliberately proves the wrong Core identity with an invalid secret.
    // socket 故意使用无效密钥证明错误的 Core 身份。
    await new Promise((resolve, reject) => {
      const socket = net.createConnection(discovery.controlPort, discovery.controlHost);
      let settled = false;
      // rejectionTimeout makes an incorrectly accepted secret fail deterministically.
      // rejectionTimeout 使被错误接受的密钥以确定方式失败。
      const rejectionTimeout = setTimeout(() => {
        if (settled) return;
        settled = true;
        socket.destroy();
        reject(new Error('Browser host did not reject the invalid Core hello.'));
      }, AUTH_REJECTION_TIMEOUT_MS);
      // connected distinguishes an expected authentication reset from a failure to reach the host.
      // connected 区分预期的鉴权重置与无法连接宿主的故障。
      let connected = false;
      socket.once('connect', () => {
        connected = true;
        socket.write(encodeFrame({
          kind: 'coreHello',
          protocolVersion: discovery.protocolVersion,
          connectionSecret: 'invalid-secret',
          sessionId: discovery.sessionId,
          hostGeneration: discovery.hostGeneration,
        }));
      });
      socket.once('close', () => {
        if (!settled) {
          settled = true;
          clearTimeout(rejectionTimeout);
          resolve();
        }
      });
      socket.once('error', error => {
        if (!settled) {
          settled = true;
          clearTimeout(rejectionTimeout);
          if (connected) resolve();
          else reject(error);
        }
      });
    });
    // exit is graceful because rejecting an unauthenticated socket is an expected security path.
    // exit 为正常退出，因为拒绝未鉴权套接字属于预期安全路径。
    const exit = await waitForExit(host.child, HOST_EXIT_TIMEOUT_MS);
    assert.equal(exit.code, 0);
    await waitForProfileRelease(host.profile, 5_000);
    return { exitCode: exit.code, discoveryPublished: true, invalidHelloRejected: true, orphanProcessIds: [] };
  } finally {
    if (host.child.exitCode === null && host.child.signalCode === null) {
      await terminateProcessTree(host.child.pid);
    }
    await terminateProfileProcesses(host.profile);
    await rm(root, { recursive: true, force: true });
  }
}

// runtimeRoot selects the exact Debug distribution under functional acceptance.
// runtimeRoot 选择本轮功能验收使用的精确 Debug 发行目录。
const runtimeRoot = path.resolve(requiredArgument('runtime-root'));
// runtime contains only paths resolved from that distribution's authoritative manifest.
// runtime 仅包含从该发行目录权威清单解析出的路径。
const runtime = await loadRuntime(runtimeRoot);
// startupFailure captures the real partial-start cleanup result.
// startupFailure 记录真实部分启动失败的清理结果。
const startupFailure = await verifyStartupFailureCleanup(runtime);
// authenticationFailure captures the real invalid-owner cleanup result.
// authenticationFailure 记录真实错误所有者鉴权失败的清理结果。
const authenticationFailure = await verifyAuthenticationFailureCleanup(runtime);
process.stdout.write(`${JSON.stringify({ result: 'passed', runtimeRoot, startupFailure, authenticationFailure })}\n`);
