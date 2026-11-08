import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile, readFile, readdir } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { githubReleaseDownloadParts, obtainLockedInput } from './locked-download.mjs';

// Verify offline cache hits, absence and corruption without any reachable network source.
// 在没有可访问网络来源的情况下验证离线缓存命中、缺失与损坏。
test('locked input accepts exact offline bytes and rejects missing or corrupted bytes', async () => {
  const cache = await mkdtemp(path.join(os.tmpdir(), 'vulcan-locked-input-'));
  const bytes = Buffer.from('locked fixture');
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  const asset = { url: 'https://invalid.invalid/fixture', sha256 };
  try {
    await assert.rejects(obtainLockedInput(asset, cache, true), /absent/);
    await writeFile(path.join(cache, sha256), bytes);
    assert.deepEqual(await obtainLockedInput(asset, cache, true), bytes);
    await writeFile(path.join(cache, sha256), 'corrupt');
    await assert.rejects(obtainLockedInput(asset, cache, true), /checksum mismatch/);
    await assert.rejects(obtainLockedInput(asset, cache, false), /checksum mismatch/);
    await assert.rejects(obtainLockedInput({ ...asset, sha256: '../escape' }, cache, true), /Invalid locked/);
  } finally {
    await rm(cache, { recursive: true, force: true });
  }
});

// A failed network response or digest must never publish an entry; a successful entry serves later offline reads.
// 网络响应或摘要失败绝不能发布缓存项；成功的缓存项用于后续离线读取。
test('download rejects HTTP failures and corrupt content before publishing cache', async (context) => {
  // cache isolates all mutation and fetch is scoped to this test.
  // cache 隔离所有变更，fetch 模拟仅限本测试。
  const cache = await mkdtemp(path.join(os.tmpdir(), 'vulcan-download-'));
  const bytes = Buffer.from('verified download');
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  const asset = { url: 'https://example.invalid/locked', sha256 };
  const fetchMock = context.mock.method(globalThis, 'fetch', async () => new Response('unavailable', { status: 503 }));
  try {
    await assert.rejects(obtainLockedInput(asset, cache, false, [0, 0]), /503/);
    assert.equal(fetchMock.mock.callCount(), 3);
    assert.deepEqual(await readdir(cache), []);
    fetchMock.mock.resetCalls();
    fetchMock.mock.mockImplementation(async () => new Response('missing', { status: 404 }));
    await assert.rejects(obtainLockedInput(asset, cache, false, [0, 0]), /404/);
    assert.equal(fetchMock.mock.callCount(), 1);
    fetchMock.mock.mockImplementation(async () => new Response('corrupt'));
    await assert.rejects(obtainLockedInput(asset, cache), /checksum mismatch/);
    assert.deepEqual(await readdir(cache), []);
    // A transient failure followed by the right bytes succeeds on the repeated request.
    // 暂时性失败之后返回正确字节时，重试的请求成功。
    let calls = 0;
    fetchMock.mock.mockImplementation(async () => (calls++ === 0 ? new Response('gateway', { status: 504 }) : new Response(bytes)));
    assert.deepEqual(await obtainLockedInput(asset, cache, false, [0]), bytes);
    assert.equal(calls, 2);
    fetchMock.mock.mockImplementation(async () => { throw new Error('Network must not be used'); });
    assert.deepEqual(await obtainLockedInput(asset, cache, true), bytes);
    assert.deepEqual(await readdir(cache), [sha256]);
  } finally {
    await rm(cache, { recursive: true, force: true });
  }
});

// A github.com release download is taken from the asset's API endpoint after one release lookup per tag.
// github.com 发布下载在每个标签一次发布查询后，改从资产的 API 端点获取。
test('github release downloads use the API asset endpoint', async (context) => {
  const cache = await mkdtemp(path.join(os.tmpdir(), 'vulcan-github-download-'));
  const bytes = Buffer.from('release asset');
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  const requests = [];
  context.mock.method(globalThis, 'fetch', async (url, options) => {
    requests.push({ url, accept: options.headers.accept });
    if (url === 'https://api.github.com/repos/owner/repo/releases/tags/v1') {
      return Response.json({ assets: [{ name: 'tool a.zip', url: 'https://api.github.com/repos/owner/repo/releases/assets/7' }] });
    }
    if (url === 'https://api.github.com/repos/owner/repo/releases/assets/7') return new Response(bytes);
    return new Response('gateway', { status: 504 });
  });
  try {
    assert.equal(githubReleaseDownloadParts('https://nodejs.org/dist/v1/node.zip'), null);
    const asset = { url: 'https://github.com/owner/repo/releases/download/v1/tool%20a.zip', sha256 };
    assert.deepEqual(await obtainLockedInput(asset, cache, false, []), bytes);
    assert.deepEqual(requests.map(request => request.accept), ['application/vnd.github+json', 'application/octet-stream']);
    await assert.rejects(
      obtainLockedInput({ url: 'https://github.com/owner/repo/releases/download/v1/missing.zip', sha256: '0'.repeat(64) }, cache, false, []),
      /has no asset missing\.zip/,
    );
  } finally {
    await rm(cache, { recursive: true, force: true });
  }
});

// Every supported tool archive and license must have a fixed digest and tag-specific source.
// 每个受支持工具归档及许可证必须具有固定摘要和指定标签的来源。
test('build input lock records immutable identities for all supported tool platforms', async () => {
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  assert.equal(lock.schema_version, 1);
  for (const tool of Object.values(lock.tools)) {
    for (const asset of tool.assets) {
      assert.match(asset.digest, /^sha256:[a-f0-9]{64}$/);
      assert.ok(asset.browser_download_url.includes(`/releases/download/${tool.tag_name}/`));
    }
    for (const license of tool.licenses) {
      assert.match(license.sha256, /^[a-f0-9]{64}$/);
      assert.ok(license.url.includes(`/${tool.tag_name}/`));
    }
  }
  for (const platform of ['win64', 'linux-x86_64', 'linux-aarch_64', 'osx-universal_binary']) {
    const asset = lock.protoc.assets.find(asset => asset.name === `protoc-${lock.protoc.version}-${platform}.zip`);
    assert.ok(asset, `Missing locked protoc platform ${platform}`);
    assert.match(asset.sha256, /^[a-f0-9]{64}$/);
  }
});
