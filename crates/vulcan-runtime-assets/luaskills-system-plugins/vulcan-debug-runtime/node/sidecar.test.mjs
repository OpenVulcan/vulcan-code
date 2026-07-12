import assert from "node:assert/strict";
import crypto from "node:crypto";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import readline from "node:readline";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

// SIDECAR_PATH resolves the production sidecar beside this test file.
// SIDECAR_PATH 解析当前测试文件旁的生产 sidecar。
const SIDECAR_PATH = path.join(path.dirname(fileURLToPath(import.meta.url)), "sidecar.mjs");

// WS_GUID is the RFC 6455 handshake constant.
// WS_GUID 是 RFC 6455 握手常量。
const WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

// Start one production sidecar and expose a serialized command helper.
// 启动一个生产 sidecar 并暴露串行命令辅助方法。
function startSidecar() {
  const child = spawn(process.execPath, [SIDECAR_PATH, "--stdio"], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  const output = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
  let requestSequence = 1;

  // Send one JSON-line command and require the matching response envelope.
  // 发送一条 JSON 行命令并要求匹配的响应信封。
  async function command(payload) {
    const requestId = String(requestSequence++);
    const response = new Promise((resolve, reject) => {
      const onExit = (code) => reject(new Error(`sidecar exited before responding: ${code}`));
      child.once("exit", onExit);
      output.once("line", (line) => {
        child.off("exit", onExit);
        resolve(JSON.parse(line.replace(/\u001e$/, "")));
      });
    });
    child.stdin.write(`${JSON.stringify({ request_id: requestId, ...payload })}\n`);
    const decoded = await response;
    assert.equal(decoded.request_id, requestId);
    return decoded;
  }

  // Stop the sidecar after the protocol transport has been closed.
  // 在协议传输关闭后停止 sidecar。
  async function stop() {
    child.stdin.end();
    await new Promise((resolve) => child.once("exit", resolve));
    output.close();
  }

  return { child, command, stop };
}

// Encode one unmasked server-to-client WebSocket text frame.
// 编码一个未掩码的服务端到客户端 WebSocket 文本帧。
function encodeServerTextFrame(text) {
  const body = Buffer.from(text, "utf8");
  if (body.length < 126) {
    return Buffer.concat([Buffer.from([0x81, body.length]), body]);
  }
  const header = Buffer.alloc(4);
  header[0] = 0x81;
  header[1] = 126;
  header.writeUInt16BE(body.length, 2);
  return Buffer.concat([header, body]);
}

// Decode every complete masked client WebSocket frame from one retained buffer.
// 从一个保留缓冲区解码全部完整的掩码客户端 WebSocket 帧。
function decodeClientFrames(buffer) {
  const messages = [];
  let offset = 0;
  while (buffer.length - offset >= 2) {
    const opcode = buffer[offset] & 0x0f;
    let length = buffer[offset + 1] & 0x7f;
    let headerLength = 2;
    if (length === 126) {
      if (buffer.length - offset < 4) break;
      length = buffer.readUInt16BE(offset + 2);
      headerLength = 4;
    } else if (length === 127) {
      if (buffer.length - offset < 10) break;
      const longLength = buffer.readBigUInt64BE(offset + 2);
      assert.ok(longLength <= BigInt(Number.MAX_SAFE_INTEGER));
      length = Number(longLength);
      headerLength = 10;
    }
    const masked = (buffer[offset + 1] & 0x80) !== 0;
    assert.equal(masked, true, "client frames must be masked");
    const frameLength = headerLength + 4 + length;
    if (buffer.length - offset < frameLength) break;
    const mask = buffer.subarray(offset + headerLength, offset + headerLength + 4);
    const body = Buffer.from(
      buffer.subarray(offset + headerLength + 4, offset + frameLength),
    );
    for (let index = 0; index < body.length; index += 1) {
      body[index] ^= mask[index % 4];
    }
    if (opcode === 0x1) messages.push(body.toString("utf8"));
    offset += frameLength;
  }
  return { messages, remaining: buffer.subarray(offset) };
}

// Start one minimal loopback WebSocket CDP peer.
// 启动一个最小回环 WebSocket CDP 对端。
async function startCdpPeer() {
  const server = http.createServer();
  server.on("upgrade", (request, socket) => {
    socket.on("error", (error) => {
      if (error.code !== "ECONNRESET") throw error;
    });
    const key = request.headers["sec-websocket-key"];
    const accept = crypto.createHash("sha1").update(`${key}${WS_GUID}`).digest("base64");
    socket.write(
      `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
    );
    let retained = Buffer.alloc(0);
    socket.on("data", (chunk) => {
      retained = Buffer.concat([retained, chunk]);
      const decoded = decodeClientFrames(retained);
      retained = decoded.remaining;
      for (const text of decoded.messages) {
        const requestMessage = JSON.parse(text);
        socket.write(
          encodeServerTextFrame(
            JSON.stringify({ method: "Runtime.consoleAPICalled", params: { type: "log" } }),
          ),
        );
        socket.write(
          encodeServerTextFrame(
            JSON.stringify({ id: requestMessage.id, result: { echoed: requestMessage.method } }),
          ),
        );
      }
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address === "object");
  return {
    endpoint: `ws://127.0.0.1:${address.port}/devtools/page/test`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

// Encode one DAP Content-Length message frame.
// 编码一个 DAP Content-Length 消息帧。
function encodeDapFrame(message) {
  const body = JSON.stringify(message);
  return Buffer.from(`Content-Length: ${Buffer.byteLength(body, "utf8")}\r\n\r\n${body}`, "utf8");
}

// Start one minimal loopback DAP TCP peer.
// 启动一个最小回环 DAP TCP 对端。
async function startDapPeer() {
  let resolveReverseResponse;
  const reverseResponse = new Promise((resolve) => {
    resolveReverseResponse = resolve;
  });
  const server = net.createServer((socket) => {
    let retained = Buffer.alloc(0);
    socket.on("data", (chunk) => {
      retained = Buffer.concat([retained, chunk]);
      const headerEnd = retained.indexOf("\r\n\r\n");
      if (headerEnd < 0) return;
      const header = retained.subarray(0, headerEnd).toString("ascii");
      const contentLength = Number.parseInt(header.match(/Content-Length:\s*(\d+)/i)[1], 10);
      const bodyStart = headerEnd + 4;
      if (retained.length < bodyStart + contentLength) return;
      const requestMessage = JSON.parse(
        retained.subarray(bodyStart, bodyStart + contentLength).toString("utf8"),
      );
      retained = retained.subarray(bodyStart + contentLength);
      if (requestMessage.type === "response") {
        resolveReverseResponse(requestMessage);
        return;
      }
      socket.write(
        encodeDapFrame({
          seq: 100,
          type: "event",
          event: "initialized",
          body: {},
        }),
      );
      socket.write(
        encodeDapFrame({
          seq: 101,
          type: "response",
          request_seq: requestMessage.seq,
          success: true,
          command: requestMessage.command,
          body: { echoed: requestMessage.command },
        }),
      );
      socket.write(
        encodeDapFrame({
          seq: 102,
          type: "request",
          command: "runInTerminal",
          arguments: { cwd: "C:/workspace" },
        }),
      );
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address === "object");
  return {
    port: address.port,
    reverseResponse,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

// Start one DAP peer that emits an unterminated oversized header after connection.
// 启动一个在连接后发送未终止超大头部的 DAP 对端。
async function startOversizedDapHeaderPeer() {
  const server = net.createServer((socket) => {
    setTimeout(() => socket.write(Buffer.alloc(16 * 1024 + 1, "A")), 25);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address === "object");
  return {
    port: address.port,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

// Verify CDP correlation, retained events, status, and deterministic closure.
// 验证 CDP 关联、事件保留、状态与确定性关闭。
test("CDP sidecar lifecycle", async () => {
  const peer = await startCdpPeer();
  const sidecar = startSidecar();
  const opened = await sidecar.command({
    op: "open",
    protocol: "cdp",
    target: { websocket_url: peer.endpoint },
  });
  assert.equal(opened.ok, true);
  assert.equal(opened.result.connected, true);

  const oversizedPage = await sidecar.command({ op: "events", max_events: 33, timeout_ms: 0 });
  assert.equal(oversizedPage.ok, false);
  assert.match(oversizedPage.error, /max_events/);

  const requested = await sidecar.command({
    op: "request",
    method: "Runtime.enable",
    params: {},
    timeout_ms: 2000,
  });
  assert.deepEqual(requested.result, { echoed: "Runtime.enable" });
  const events = await sidecar.command({ op: "events", max_events: 8, timeout_ms: 0 });
  assert.equal(events.result.events[0].method, "Runtime.consoleAPICalled");

  const closed = await sidecar.command({ op: "close" });
  assert.equal(closed.ok, true);
  assert.equal(closed.result.closed, true);
  await sidecar.stop();
  await peer.close();
});

// Verify DAP framing, request correlation, retained events, and deterministic closure.
// 验证 DAP 分帧、请求关联、事件保留与确定性关闭。
test("DAP sidecar lifecycle", async () => {
  const peer = await startDapPeer();
  const sidecar = startSidecar();
  const opened = await sidecar.command({
    op: "open",
    protocol: "dap",
    target: { host: "127.0.0.1", port: peer.port },
  });
  assert.equal(opened.ok, true);
  assert.equal(opened.result.connected, true);

  const requested = await sidecar.command({
    op: "request",
    command: "initialize",
    arguments: {},
    timeout_ms: 2000,
  });
  assert.deepEqual(requested.result, { echoed: "initialize" });
  const events = await sidecar.command({ op: "events", max_events: 8, timeout_ms: 0 });
  assert.equal(events.result.events[0].event, "initialized");
  assert.equal(events.result.events[1].kind, "request");
  assert.equal(events.result.events[1].command, "runInTerminal");

  const invalidReverseResponse = await sidecar.command({
    op: "request",
    request_seq: events.result.events[1].request_seq + 1,
    command: events.result.events[1].command,
    success: true,
    body: {},
  });
  assert.equal(invalidReverseResponse.ok, false);
  assert.match(invalidReverseResponse.error, /not pending/);

  const reverseResponse = await sidecar.command({
    op: "request",
    request_seq: events.result.events[1].request_seq,
    command: events.result.events[1].command,
    success: true,
    body: { processId: 42 },
  });
  assert.equal(reverseResponse.result.sent, true);
  assert.equal((await peer.reverseResponse).body.processId, 42);

  const closed = await sidecar.command({ op: "close" });
  assert.equal(closed.ok, true);
  assert.equal(closed.result.closed, true);
  await sidecar.stop();
  await peer.close();
});

// Verify a malformed peer cannot grow the retained DAP header without bound.
// 验证异常对端不能让保留的 DAP 头部无界增长。
test("DAP oversized header is rejected", async () => {
  const peer = await startOversizedDapHeaderPeer();
  const sidecar = startSidecar();
  const opened = await sidecar.command({
    op: "open",
    protocol: "dap",
    target: { host: "127.0.0.1", port: peer.port },
  });
  assert.equal(opened.ok, true);

  const events = await sidecar.command({ op: "events", max_events: 8, timeout_ms: 2000 });
  assert.equal(events.ok, true);
  assert.equal(events.result.events[0].kind, "transport_error");
  assert.match(events.result.events[0].error, /maximum size/);
  const status = await sidecar.command({ op: "status" });
  assert.equal(status.result.connected, false);
  assert.equal(status.result.closed, true);

  await sidecar.stop();
  await peer.close();
});
