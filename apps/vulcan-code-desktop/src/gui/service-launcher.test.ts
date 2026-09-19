// Exercise service readiness against a real isolated HTTP discovery endpoint.
// 使用真实隔离 HTTP 发现端点验证服务就绪行为。
import { strict as assert } from 'node:assert';
import { createServer } from 'node:http';
import { mkdtemp, readFile, writeFile, unlink, rmdir } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { ensureServiceInstance, readLiveServiceInstance, resolveWebUiUrl, type ServiceLaunchDescription } from './service-launcher.js';

// Run readiness through the public launcher with supplied sequential health states.
// 通过公开启动器，使用给定的连续健康状态执行就绪等待。
// statuses controls server replies; body checks the final result while cleanup always releases resources.
// statuses 控制服务应答；body 检查最终结果，清理始终释放资源。
async function withHealth(statuses: string[], body: (description: ServiceLaunchDescription, states: string[]) => Promise<void>): Promise<void> {
  // Every test owns its discovery file and ephemeral port.
  // 每项测试拥有独立发现文件和临时端口。
  const directory = await mkdtemp(path.join(os.tmpdir(), 'vulcan-launcher-test-'));
  const discoveryPath = path.join(directory, 'discovery.json');
  const states: string[] = [];
  let baseUrl = '';
  const identity = { pid: process.pid, serviceToken: 'test-only', startedAt: '1', host: '127.0.0.1', port: 0 };
  const server = createServer((_request, response) => {
    const status = statuses[Math.min(states.length, statuses.length - 1)]!;
    states.push(status);
    response.setHeader('Content-Type', 'application/json');
    response.end(JSON.stringify({ ...identity, baseUrl, status }));
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  identity.port = address.port;
  baseUrl = `http://127.0.0.1:${address.port}`;
  await writeFile(discoveryPath, JSON.stringify({ ...identity, baseUrl, status: 'starting' }));
  const description: ServiceLaunchDescription = {
    protocolVersion: 1, serviceName: 'test', serviceVersion: 'test', profile: 'debug',
    executablePath: 'must-not-spawn', dataRoot: directory, discoveryPath,
    lockPath: path.join(directory, 'lock'), defaultWebUiMode: 'vite', viteUrl: null, modes: [],
  };
  try { await body(description, states); }
  finally {
    await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    await unlink(discoveryPath);
    await rmdir(directory);
  }
}

test('waits for business readiness without spawning a second live owner', async () => {
  await withHealth(['starting', 'starting', 'ready'], async (description, states) => {
    const stages: string[] = [];
    const instance = await ensureServiceInstance(description, [], progress => stages.push(progress.stage));
    assert.equal(instance.status, 'ready');
    assert.equal(states.length, 3);
    assert(!stages.includes('starting-service'));
  });
});

for (const status of ['failed', 'stopping']) {
  test(`refuses an existing ${status} owner`, async () => {
    await withHealth([status], async description => {
      await assert.rejects(ensureServiceInstance(description, [], () => {}), new RegExp(status));
    });
  });
}

// Rejects a rewritten discovery origin before the GUI can load it as a privileged workbench.
// 在 GUI 将改写的发现来源作为特权工作台加载前拒绝它。
test('rejects a rewritten discovery address before probing health', async () => {
  await withHealth(['ready'], async (description, states) => {
    const original = JSON.parse(await readFile(description.discoveryPath, 'utf8')) as { baseUrl: string };
    for (const baseUrl of [`${original.baseUrl}/rewritten`, 'https://example.invalid']) {
      await writeFile(description.discoveryPath, JSON.stringify({ ...original, baseUrl }));
      assert.equal(await readLiveServiceInstance(description.discoveryPath), null);
    }
    assert.equal(states.length, 0);
  });
});

// An explicit workbench override must remain inside the selected Core's trusted pages.
// 显式工作台覆盖必须保持在所选 Core 的可信页面内。
test('accepts only the selected Core or its declared Vite page as an explicit WebUI', async () => {
  await withHealth(['ready'], async description => {
    const instance = await readLiveServiceInstance(description.discoveryPath);
    assert(instance);
    assert.equal(resolveWebUiUrl(description, instance, instance.baseUrl), instance.baseUrl);
    description.viteUrl = 'http://127.0.0.1:17601';
    assert.equal(resolveWebUiUrl(description, instance, description.viteUrl), description.viteUrl);
    assert.throws(() => resolveWebUiUrl(description, instance, 'https://example.invalid'),
      /must match the selected Core/);
    assert.throws(() => resolveWebUiUrl(description, instance, `${instance.baseUrl}/rewritten`),
      /must match the selected Core/);
  });
});
