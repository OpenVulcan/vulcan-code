import assert from 'node:assert/strict';
import net from 'node:net';
import { test } from 'node:test';
import WebSocket from 'ws';

import { PlaywrightProxy } from './playwright-proxy.js';

/** Creates a fake BrowserServer pipe and optionally observes frames without replying. */
/** 创建假的 BrowserServer 管道，并可选择只观察协议帧而不回复。 */
async function listenPipe(onFrame?: (body: Buffer) => void): Promise<{ path: string; server: net.Server }> {
  const path = `\\\\.\\pipe\\vulcan-playwright-proxy-test-${process.pid}-${Date.now()}`;
  const server = net.createServer(socket => {
    let pending = Buffer.alloc(0);
    socket.on('data', chunk => {
      const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      pending = pending.length === 0 ? bytes : Buffer.concat([pending, bytes]);
      let end = pending.indexOf(0);
      while (end !== -1) {
        const body = pending.subarray(0, end);
        pending = pending.subarray(end + 1);
        if (onFrame) onFrame(body);
        else {
          socket.write(body);
          socket.write('\0');
        }
        end = pending.indexOf(0);
      }
    });
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(path, () => {
      server.off('error', reject);
      resolve();
    });
  });
  return { path, server };
}

/** Waits for one WebSocket connection to become usable. */
/** 等待 WebSocket 连接进入可用状态。 */
async function waitForOpen(socket: WebSocket): Promise<void> {
  if (socket.readyState === WebSocket.OPEN) return;
  await new Promise<void>((resolve, reject) => {
    socket.once('open', resolve);
    socket.once('error', reject);
  });
}

/** Reads one JSON message from a WebSocket client. */
/** 从 WebSocket 客户端读取一条 JSON 消息。 */
async function readJson(socket: WebSocket): Promise<Record<string, unknown>> {
  return new Promise<Record<string, unknown>>((resolve, reject) => {
    socket.once('message', data => {
      try { resolve(JSON.parse(data.toString('utf8')) as Record<string, unknown>); }
      catch (error) { reject(error); }
    });
    socket.once('error', reject);
  });
}

/** Closes the fake named pipe after all test clients have stopped. */
/** 在所有测试客户端停止后关闭假的命名管道。 */
async function closePipe(server: net.Server): Promise<void> {
  await new Promise<void>(resolve => server.close(() => resolve()));
}

test('Playwright relay hides HTTP discovery and forwards one named-pipe session', async () => {
  const pipe = await listenPipe();
  let bindCount = 0;
  let unbindCount = 0;
  const browser = {
    async bind(): Promise<{ endpoint: string }> {
      bindCount += 1;
      return { endpoint: pipe.path };
    },
    async unbind(): Promise<void> {
      unbindCount += 1;
    },
  };
  const proxy = new PlaywrightProxy(browser as never);
  let client: WebSocket | null = null;
  try {
    const grant = await proxy.open('core-test');
    const httpResponse = await fetch(grant.endpoint.replace(/^ws:/, 'http:'));
    assert.equal(httpResponse.status, 404);
    assert.equal(httpResponse.headers.get('cache-control'), 'no-store');

    client = new WebSocket(grant.endpoint);
    await waitForOpen(client);
    const response = readJson(client);
    client.send(JSON.stringify({ id: 7, method: 'Browser.getVersion' }));
    assert.deepEqual(await response, { id: 7, method: 'Browser.getVersion' });
    assert.deepEqual(await proxy.close('core-test'), {
      revoked: true,
      status: 'revoked',
      inFlightRequestIds: [],
    });
    await proxy.shutdown();
    assert.equal(bindCount, 1);
    assert.equal(unbindCount, 1);
  } finally {
    client?.terminate();
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});

test('Playwright relay waits for unbind and rejects reuse of a consumed grant', async () => {
  const pipe = await listenPipe();
  let unbindCount = 0;
  const browser = {
    async bind(): Promise<{ endpoint: string }> { return { endpoint: pipe.path }; },
    async unbind(): Promise<void> { unbindCount += 1; },
  };
  const proxy = new PlaywrightProxy(browser as never);
  let client: WebSocket | null = null;
  try {
    const first = await proxy.open('first-owner');
    client = new WebSocket(first.endpoint);
    await waitForOpen(client);
    const secondUpgrade = new WebSocket(first.endpoint);
    const secondError = new Promise<void>(resolve => {
      secondUpgrade.once('unexpected-response', (_request, response) => {
        assert.equal(response.statusCode, 403);
        resolve();
      });
      secondUpgrade.once('error', () => resolve());
    });
    await secondError;
    client.terminate();
    assert.equal((await proxy.close('wrong-owner')).revoked, false);
    assert.equal((await proxy.close('first-owner')).revoked, true);
    const second = await proxy.open('second-owner');
    assert.notEqual(second.endpoint, first.endpoint);
    assert.equal(unbindCount, 1);
    assert.equal((await proxy.close('second-owner')).revoked, true);
  } finally {
    client?.terminate();
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});

// Revocation must close an attached client and make its old URL unusable after a new owner opens.
// 撤销必须关闭已连接客户端，并在新所有者开启后让旧地址继续失效。
test('Playwright relay rejects an old socket and URL after owner replacement', async () => {
  const pipe = await listenPipe();
  const browser = {
    async bind(): Promise<{ endpoint: string }> { return { endpoint: pipe.path }; },
    async unbind(): Promise<void> {},
  };
  const proxy = new PlaywrightProxy(browser as never);
  let oldClient: WebSocket | null = null;
  let currentClient: WebSocket | null = null;
  try {
    const oldGrant = await proxy.open('old-owner');
    oldClient = new WebSocket(oldGrant.endpoint);
    await waitForOpen(oldClient);
    const oldClosed = new Promise<void>(resolve => oldClient?.once('close', () => resolve()));
    assert.equal((await proxy.close('old-owner')).status, 'revoked');
    await oldClosed;
    assert.equal(oldClient.readyState, WebSocket.CLOSED);

    const currentGrant = await proxy.open('current-owner');
    assert.notEqual(currentGrant.endpoint, oldGrant.endpoint);
    await assert.rejects(waitForOpen(new WebSocket(oldGrant.endpoint)));
    currentClient = new WebSocket(currentGrant.endpoint);
    await waitForOpen(currentClient);
    const response = readJson(currentClient);
    currentClient.send(JSON.stringify({ id: 23, method: 'Browser.getVersion' }));
    assert.deepEqual(await response, { id: 23, method: 'Browser.getVersion' });
    assert.equal((await proxy.close('current-owner')).status, 'revoked');
  } finally {
    oldClient?.terminate();
    currentClient?.terminate();
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});

test('Playwright relay keeps an unanswered forwarded request after the client disconnects', async () => {
  // forwarded resolves only when the fake BrowserServer received a complete protocol frame.
  // forwarded 只在假的 BrowserServer 收到完整协议帧后才完成。
  let acceptFrame: ((body: Buffer) => void) | undefined;
  const forwarded = new Promise<Buffer>(resolve => { acceptFrame = resolve; });
  const pipe = await listenPipe(body => acceptFrame?.(body));
  const browser = {
    async bind(): Promise<{ endpoint: string }> { return { endpoint: pipe.path }; },
    async unbind(): Promise<void> {},
  };
  const proxy = new PlaywrightProxy(browser as never);
  let client: WebSocket | null = null;
  try {
    const grant = await proxy.open('in-flight-owner');
    client = new WebSocket(grant.endpoint);
    await waitForOpen(client);
    client.send(JSON.stringify({ id: 17, method: 'Browser.getVersion' }));
    assert.deepEqual(JSON.parse((await forwarded).toString('utf8')), {
      id: 17, method: 'Browser.getVersion',
    });
    const closed = new Promise<void>(resolve => client?.once('close', () => resolve()));
    client.terminate();
    await closed;
    assert.deepEqual(await proxy.close('in-flight-owner'), {
      revoked: true,
      status: 'inFlight',
      inFlightRequestIds: [17],
    });
    await assert.rejects(
      proxy.open('next-owner'),
      /previous Playwright request remains unconfirmed/,
    );
  } finally {
    client?.terminate();
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});

test('Playwright relay rejects a second grant while the first pipe binding is pending', async () => {
  // bindingStarted and releaseBinding hold the first grant before it publishes a gateway.
  // bindingStarted 与 releaseBinding 将第一个授权停在网关发布之前。
  let reportBindingStarted: (() => void) | undefined;
  const bindingStarted = new Promise<void>(resolve => { reportBindingStarted = resolve; });
  let releaseBinding: (() => void) | undefined;
  const bindingGate = new Promise<void>(resolve => { releaseBinding = resolve; });
  const pipe = await listenPipe();
  const browser = {
    async bind(): Promise<{ endpoint: string }> {
      reportBindingStarted?.();
      await bindingGate;
      return { endpoint: pipe.path };
    },
    async unbind(): Promise<void> {},
  };
  const proxy = new PlaywrightProxy(browser as never);
  try {
    const first = proxy.open('first-owner');
    await bindingStarted;
    await assert.rejects(proxy.open('second-owner'), /already active/);
    releaseBinding?.();
    const grant = await first;
    assert.match(grant.endpoint, /^ws:\/\/127\.0\.0\.1:/);
    assert.equal((await proxy.close('first-owner')).status, 'revoked');
  } finally {
    releaseBinding?.();
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});

// Shutdown must wait for a pending BrowserServer binding and prevent a late grant.
// 关闭必须等待尚未完成的 BrowserServer 绑定，并阻止迟到的授权。
test('Playwright relay shutdown prevents a pending bind from publishing a grant', async () => {
  let reportBindingStarted: (() => void) | undefined;
  const bindingStarted = new Promise<void>(resolve => { reportBindingStarted = resolve; });
  let releaseBinding: (() => void) | undefined;
  const bindingGate = new Promise<void>(resolve => { releaseBinding = resolve; });
  const pipe = await listenPipe();
  let unbindCount = 0;
  const browser = {
    async bind(): Promise<{ endpoint: string }> {
      reportBindingStarted?.();
      await bindingGate;
      return { endpoint: pipe.path };
    },
    async unbind(): Promise<void> { unbindCount += 1; },
  };
  const proxy = new PlaywrightProxy(browser as never);
  try {
    const opening = proxy.open('late-owner');
    await bindingStarted;
    const closing = proxy.shutdown();
    releaseBinding?.();
    await assert.rejects(opening, /shutting down/);
    await closing;
    assert.equal(unbindCount, 1);
    await assert.rejects(proxy.open('later-owner'), /shutting down/);
  } finally {
    releaseBinding?.();
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});

// A failed private-pipe unbind must not be reported to Core as a confirmed revocation.
// 私有管道解绑失败不得向 Core 回报已确认撤销。
test('Playwright relay reports an unbind failure as unknown and blocks a new grant', async () => {
  const pipe = await listenPipe();
  const privateEndpoint = `${pipe.path}-private-capability`;
  const browser = {
    async bind(): Promise<{ endpoint: string }> { return { endpoint: pipe.path }; },
    async unbind(): Promise<void> { throw new Error(privateEndpoint); },
  };
  const proxy = new PlaywrightProxy(browser as never);
  const diagnostics: string[] = [];
  (proxy as unknown as { diagnostic(message: string): void }).diagnostic = message => {
    diagnostics.push(message);
  };
  try {
    await proxy.open('unbind-owner');
    const result = await proxy.close('unbind-owner');
    assert.equal(result.revoked, true);
    assert.equal(result.status, 'unknown');
    assert.ok(!diagnostics.join('\n').includes(privateEndpoint));
    await assert.rejects(proxy.open('next-owner'), /unbind failed/);
  } finally {
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});

test('Playwright relay does not await replies for one-way wait and abort metadata', async () => {
  // forwarded resolves after both protocol messages reach the fake BrowserServer without replies.
  // forwarded 在两条协议消息均到达假的 BrowserServer 且无需回复后完成。
  let reportForwarded: (() => void) | undefined;
  const forwarded = new Promise<void>(resolve => { reportForwarded = resolve; });
  const seen: string[] = [];
  const pipe = await listenPipe(body => {
    seen.push(JSON.parse(body.toString('utf8')).method);
    if (seen.length === 2) reportForwarded?.();
  });
  const browser = {
    async bind(): Promise<{ endpoint: string }> { return { endpoint: pipe.path }; },
    async unbind(): Promise<void> {},
  };
  const proxy = new PlaywrightProxy(browser as never);
  let client: WebSocket | null = null;
  try {
    const grant = await proxy.open('one-way-owner');
    client = new WebSocket(grant.endpoint);
    await waitForOpen(client);
    client.send(JSON.stringify({ id: 21, method: '__waitInfo__', params: { phase: 'before' } }));
    client.send(JSON.stringify({ id: 22, method: '__abort__', params: { id: 19 } }));
    await forwarded;
    assert.deepEqual(seen, ['__waitInfo__', '__abort__']);
    assert.deepEqual(await proxy.close('one-way-owner'), {
      revoked: true,
      status: 'revoked',
      inFlightRequestIds: [],
    });
  } finally {
    client?.terminate();
    await proxy.shutdown();
    await closePipe(pipe.server);
  }
});
