// Verify that a healthy headless Core handles a real terminal Ctrl-C and exits through shutdown.
// 验证健康无头 Core 能处理真实终端 Ctrl-C，并通过关闭协调器退出。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// repository contains the debug test-support service and isolated release resources.
// repository 包含 debug 测试支持服务与隔离发行资源。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
// assetRoot is a generated package, never the live output/bin used by the user.
// assetRoot 是生成的发行包，不使用用户正在运行的 output/bin。
const assetRoot = path.join(repository, 'target/w10-release-distribution');
// missingChromium simulates an optional browser component absent from an isolated layout.
// missingChromium 在隔离布局中模拟可选浏览器组件缺失。
const missingChromium = process.argv.slice(2).includes('--missing-chromium');
// root isolates all persistent service data from the actual debug profile.
// root 将全部持久化服务数据与真实 debug 配置隔离。
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-w10-headless-'));
// layoutRoot contains only test-owned links and a debug runtime manifest.
// layoutRoot 仅保存测试自有链接与 debug 运行布局清单。
const layoutRoot = await mkdtemp(path.join(repository, 'target/w10-headless-layout-'));
// service is retained for bounded cleanup even when startup or signalling fails.
// service 在启动或信号处理失败时仍保留，以便有界清理。
let service = null;
// interrupted records whether the enclosing terminal delivered the requested Ctrl-C.
// interrupted 记录外层终端是否投递了请求的 Ctrl-C。
let interrupted = false;
process.on('SIGINT', () => { interrupted = true; });

// assertScopedDeletion verifies a generated directory stays within its named parent.
// assertScopedDeletion 验证生成目录始终位于指定父目录之内。
// Parameters: parent is the allowed root and candidate is the directory to remove.
// 参数：parent 是允许的根目录，candidate 是准备删除的目录。
// Returns: the checked absolute candidate path.
// 返回值：经检查的候选绝对路径。
function assertScopedDeletion(parent, candidate) {
  // absoluteParent is the normalized cleanup boundary.
  // absoluteParent 是规范化的清理边界。
  const absoluteParent = path.resolve(parent);
  // absoluteCandidate is the normalized generated directory to delete.
  // absoluteCandidate 是准备删除的规范化生成目录。
  const absoluteCandidate = path.resolve(candidate);
  // relative proves the candidate is a strict descendant.
  // relative 证明候选目录是严格子目录。
  const relative = path.relative(absoluteParent, absoluteCandidate);
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative),
    'Refusing recursive cleanup outside the generated parent.');
  return absoluteCandidate;
}

// serviceRpc calls one authenticated Core method and requires a successful wire result.
// serviceRpc 调用一个已认证的 Core 方法，并要求线协议返回成功。
// Parameters: discovery selects the isolated service, method its exact RPC, and params its declared fields.
// 参数：discovery 选择隔离服务，method 是精确 RPC，params 是声明的字段。
// Returns: the Core result after transport and application checks.
// 返回值：通过传输与应用层检查后的 Core 结果。
async function serviceRpc(discovery, method, params = {}) {
  // response is the authenticated HTTP transport result.
  // response 是已认证的 HTTP 传输结果。
  const response = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method, params }),
  });
  // body retains an explicit Core refusal instead of treating HTTP 200 as success.
  // body 保留 Core 的明确拒绝，不把 HTTP 200 直接当作成功。
  const body = await response.json();
  assert.equal(response.status, 200, `${method}: HTTP ${response.status}`);
  assert.equal(body.ok, true, `${method}: ${JSON.stringify(body.error)}`);
  return body.result;
}

try {
  // layout is copied from the generated package and switched only to the matching debug profile.
  // layout 从生成的发行包读取，仅切换到匹配的 debug 配置。
  const layout = JSON.parse(await readFile(path.join(assetRoot, 'runtime-layout.json'), 'utf8'));
  layout.profile = 'debug';
  if (missingChromium) layout.chromiumExecutable = 'browser/not-installed-chromium.exe';
  await Promise.all(['desktop', 'browser', 'webui'].map(async (name) => {
    await symlink(path.join(assetRoot, name), path.join(layoutRoot, name), 'junction');
  }));
  await writeFile(path.join(layoutRoot, 'runtime-layout.json'), JSON.stringify(layout));
  // diagnostic captures only local service logs if the isolated process fails.
  // diagnostic 仅在隔离进程失败时保留本地服务日志。
  const diagnostic = [];
  service = spawn(path.join(repository, 'target/debug/vulcan-code-service.exe'),
    ['--headless', `--test-root=${root}`], {
      env: { ...process.env, VULCAN_RUNTIME_LAYOUT: path.join(layoutRoot, 'runtime-layout.json'),
        VULCAN_CODE_OUTPUT_ROOT: assetRoot },
      stdio: ['inherit', 'pipe', 'pipe'],
    });
  service.stdout.on('data', (chunk) => diagnostic.push(chunk.toString()));
  service.stderr.on('data', (chunk) => diagnostic.push(chunk.toString()));
  // discovery is published only by this exact test child after its HTTP bridge starts.
  // discovery 仅由此测试子进程在 HTTP 桥启动后发布。
  let discovery;
  // readyDeadline bounds startup even if the service leaves a stale discovery file.
  // readyDeadline 限制启动等待时间，即使服务留下旧发现文件也不会无限等待。
  const readyDeadline = Date.now() + 120_000;
  // ready becomes true only after the exact child reports its ready lifecycle.
  // ready 仅在精确的子进程报告 ready 生命周期后变为真。
  let ready = false;
  while (Date.now() < readyDeadline) {
    assert.equal(service.exitCode, null, `Headless service exited before ready: ${diagnostic.join('').slice(-1200)}`);
    try {
      discovery = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
      // response comes from the authenticated service endpoint, not the discovery file alone.
      // response 来自已认证的服务端点，而不只依赖发现文件。
      const response = await fetch(`${discovery.baseUrl}/v1/service/status`, {
        headers: { 'x-vulcan-service-token': discovery.serviceToken },
      });
      // status binds readiness to this process id.
      // status 将就绪状态绑定到当前进程标识。
      const status = await response.json();
      if (response.status === 200 && status.pid === service.pid && status.lifecycle === 'ready') {
        ready = true;
        break;
      }
    } catch {
      // The service has not published its ready snapshot yet.
      // 服务尚未发布就绪快照。
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.ok(ready && discovery, 'Headless service did not become ready.');
  // readyResponse rechecks the service immediately before the terminal signal.
  // readyResponse 在投递终端信号前再次检查服务。
  const readyResponse = await fetch(`${discovery.baseUrl}/v1/service/status`, {
    headers: { 'x-vulcan-service-token': discovery.serviceToken },
  });
  assert.equal((await readyResponse.json()).pid, service.pid);
  // browserStatus confirms that Core reports the optional Chromium separately from HTTP health.
  // browserStatus 确认 Core 独立于 HTTP 健康状态报告可选 Chromium。
  const browserStatus = await serviceRpc(discovery, 'browser.runtime.status');
  assert.equal(browserStatus.ready, !missingChromium);
  // pluginMcpEnvironmentReady proves package installation and optional Chromium remain separate.
  // pluginMcpEnvironmentReady 证明插件包安装与可选 Chromium 各自独立。
  let pluginMcpEnvironmentReady = null;
  if (missingChromium) {
    assert.equal(await serviceRpc(discovery, 'service_plugin_environment_ready',
      { pluginId: 'vulcan-browser' }), false);
    // installTask belongs to Core and may be observed independently of this terminal.
    // installTask 归 Core 持有，可以独立于本终端观察。
    const installTask = await serviceRpc(discovery, 'service_start_plugin_environment_install',
      { pluginId: 'vulcan-browser' });
    // installDeadline bounds dependency preparation in the isolated user environment.
    // installDeadline 限制隔离用户环境中的依赖准备时间。
    const installDeadline = Date.now() + 120_000;
    let installed = false;
    while (Date.now() < installDeadline) {
      // taskStatus is the current Core-owned installation state.
      // taskStatus 是当前由 Core 持有的安装状态。
      const taskStatus = await serviceRpc(discovery, 'service_plugin_environment_install_status',
        { taskId: installTask.taskId });
      assert.notEqual(taskStatus.state, 'failed', JSON.stringify(taskStatus));
      if (taskStatus.state === 'succeeded') { installed = true; break; }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    assert.ok(installed, 'Playwright MCP dependencies did not install in time.');
    pluginMcpEnvironmentReady = await serviceRpc(discovery, 'service_plugin_environment_ready',
      { pluginId: 'vulcan-browser' });
    assert.equal(pluginMcpEnvironmentReady, true);
    assert.equal((await serviceRpc(discovery, 'browser.runtime.status')).ready, false);
  }
  console.log(`W10_HEADLESS_READY pid=${service.pid}; send Ctrl-C to this terminal`);
  // close waits for the actual service process exit; a script-level signal alone is insufficient.
  // close 等待真实服务进程退出；仅脚本收到信号不算通过。
  const close = await new Promise((resolve, reject) => {
    // timeout is cleared on child exit so a successful harness can finish naturally.
    // timeout 会在子进程退出时清除，使成功的验收脚本能够自然结束。
    const timeout = setTimeout(() => reject(new Error('Headless service ignored Ctrl-C for 30 seconds.')), 30_000);
    service.once('exit', (code, signal) => {
      clearTimeout(timeout);
      resolve({ code, signal });
    });
  });
  assert.equal(close.code, 0, `Headless shutdown exit was ${JSON.stringify(close)}; terminalSignal=${interrupted}`);
  // A new HTTP connection must fail after the service has fully exited.
  // 服务彻底退出后，新 HTTP 连接必须失败。
  await assert.rejects(fetch(`${discovery.baseUrl}/v1/service/status`, {
    headers: { 'x-vulcan-service-token': discovery.serviceToken }, signal: AbortSignal.timeout(1000),
  }));
  console.log(JSON.stringify({ result: 'passed', headlessExitCode: close.code,
    terminalSignalObserved: interrupted, portClosed: true,
    coreBrowserComponentReady: browserStatus.ready, pluginMcpEnvironmentReady }));
} finally {
  if (service && service.exitCode === null && service.signalCode === null) {
    service.kill('SIGKILL');
    await new Promise((resolve) => service.once('exit', resolve));
  }
  await rm(assertScopedDeletion(tmpdir(), root), { recursive: true, force: true });
  await rm(assertScopedDeletion(path.join(repository, 'target'), layoutRoot), { recursive: true, force: true });
}
