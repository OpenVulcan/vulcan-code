// Verify first-use built-in browser readiness through an isolated test-support Core.
// 通过隔离的测试支持版 Core 验证内置浏览器首次使用就绪状态。
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFile, writeFile, mkdtemp, rm, rename, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// All state belongs to this disposable Core; the user's application is never contacted.
// 全部状态属于本可丢弃 Core；绝不连接用户的应用。
const distribution = path.resolve(process.argv[2]);
// Refuse user distributions because this fixture temporarily changes its own bootstrap configuration.
// 拒绝用户发行物，因为夹具会临时修改自身引导配置。
const targetRoot = await realpath(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../target'));
assert.ok((await realpath(distribution)).startsWith(targetRoot + path.sep));
const layout = JSON.parse(await readFile(path.join(distribution, 'runtime-layout.json'), 'utf8'));
const root = await mkdtemp(path.join(tmpdir(), 'vulcan-built-in-core-'));
// Browser verification excludes unrelated automatic CodeKit skill downloads in a fresh Core.
// 浏览器验收排除全新 Core 中无关的 CodeKit 技能自动下载。
const bootstrapPath = path.join(distribution, 'config/luaskills_root_skills.yaml');
const bootstrapOriginal = await readFile(bootstrapPath);
await writeFile(bootstrapPath, 'root_skills: []\n');
const service = spawn(path.join(distribution, layout.serviceExecutable), ['--headless', `--test-root=${root}`], {
  windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, VULCAN_RUNTIME_LAYOUT: path.join(distribution, 'runtime-layout.json'), VULCAN_CODE_OUTPUT_ROOT: distribution,
    HTTP_PROXY: 'http://127.0.0.1:1', HTTPS_PROXY: 'http://127.0.0.1:1', npm_config_offline: 'true' },
});
let log = '';
service.stdout.on('data', chunk => { log += chunk; });
service.stderr.on('data', chunk => { log += chunk; });
let discovery;
// Bound every service operation and the entire run, including native initialization.
// 限定每项服务操作及整轮测试时长，包含原生初始化。
const timeout = setTimeout(() => service.kill(), 120_000);
async function rpc(method, params = {}) {
  const response = await fetch(`${discovery.baseUrl}/rpc`, {
    method: 'POST', headers: { 'content-type': 'application/json', 'x-vulcan-service-token': discovery.serviceToken },
    body: JSON.stringify({ method, params }), signal: AbortSignal.timeout(30_000),
  });
  const body = await response.json();
  assert.equal(body.ok, true, `${method}: ${JSON.stringify(body)}`);
  return body.result;
}

try {
  for (let attempt = 0; attempt < 1000; attempt++) {
    if (service.exitCode !== null) throw new Error(`Core exited: ${log}`);
    try {
      discovery = JSON.parse(await readFile(path.join(root, 'user-vulcan/code/webui-service.json'), 'utf8'));
      const health = await (await fetch(`${discovery.baseUrl}/health`, { signal: AbortSignal.timeout(1000) })).json();
      if (health.status === 'ready' && health.pid === service.pid) break;
    } catch (error) { if (error.code !== 'ENOENT' && !(error instanceof TypeError)) throw error; }
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.ok(discovery, log);
  const status = await rpc('browser.runtime.status');
  assert.equal(status.ready, true);
  assert.equal(status.canInstall, false);
  assert.equal(await rpc('service_plugin_environment_ready', { pluginId: 'vulcan-browser' }), true);
  try {
    await rpc('service_set_vulcan_plugin_enabled', { pluginId: 'vulcan-browser', enabled: true });
  } catch (error) {
    console.error(JSON.stringify(await rpc('startup.snapshot')).slice(0, 12000));
    throw error;
  }
  const ownerSessionId = 'bundled-browser-test';
  const session = await rpc('browser.session.create', { ownerSessionId, profileMode: 'ephemeral' });
  const lease = await rpc('browser.lease.acquire', { ownerSessionId, sessionId: session.sessionId, holder: 'bundled-browser-test' });
  const pages = await rpc('browser.pages.list', { ownerSessionId, sessionId: session.sessionId });
  const capture = await rpc('browser.page.capture', { ownerSessionId, sessionId: session.sessionId, pageId: pages[0].pageId });
  assert.equal(capture.mimeType, 'image/jpeg');
  await rpc('browser.session.close', { ownerSessionId, sessionId: session.sessionId, leaseSecret: lease.leaseSecret });
  // Simulate a missing shipped file only inside this explicitly supplied test distribution.
  // 仅在显式指定的测试发行物中模拟随包文件缺失。
  const entry = path.join(distribution, layout.browserHostEntry);
  await rename(entry, `${entry}.test-missing`);
  try {
    const missing = await rpc('browser.runtime.status');
    assert.equal(missing.ready, false);
    assert.equal(missing.canInstall, false);
    assert.match(missing.diagnostic, /Repair the application installation/);
  } finally { await rename(`${entry}.test-missing`, entry); }
  assert.equal((await rpc('browser.runtime.status')).ready, true);
  console.log(JSON.stringify({ result: 'passed', firstUseReady: true, pluginEnabledOffline: true, coreCapture: capture.mimeType, noRuntimeInstaller: true, missingFileRecovery: true }));
} catch (error) {
  console.error(log.slice(-12000));
  throw error;
} finally {
  clearTimeout(timeout);
  if (service.exitCode === null) { service.kill(); await once(service, 'exit'); }
  await writeFile(bootstrapPath, bootstrapOriginal);
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
}
