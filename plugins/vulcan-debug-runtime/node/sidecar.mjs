import net from "node:net";
import readline from "node:readline";

// MAX_EVENTS bounds retained protocol notifications for one debug session.
// MAX_EVENTS 限制单个调试会话保留的协议通知数量。
const MAX_EVENTS = 1024;

// MAX_EVENT_PAGE bounds one event-poll response independently of queue capacity.
// MAX_EVENT_PAGE 独立于队列容量限制单次事件轮询响应数量。
const MAX_EVENT_PAGE = 32;

// MAX_EVENT_BYTES bounds one retained decoded protocol event.
// MAX_EVENT_BYTES 限制单条保留协议事件解码后的字节数。
const MAX_EVENT_BYTES = 16 * 1024;

// MAX_RESPONSE_BYTES keeps one framed response below the Lua managed-session read buffer.
// MAX_RESPONSE_BYTES 让单条分帧响应保持在 Lua 受管会话读取缓冲区以内。
const MAX_RESPONSE_BYTES = 768 * 1024;

// MAX_DAP_HEADER_BYTES bounds an incomplete or malicious DAP header.
// MAX_DAP_HEADER_BYTES 限制不完整或恶意 DAP 头部的最大字节数。
const MAX_DAP_HEADER_BYTES = 16 * 1024;

// MAX_DAP_BODY_BYTES bounds one decoded DAP JSON message body.
// MAX_DAP_BODY_BYTES 限制单条 DAP JSON 消息正文的最大字节数。
const MAX_DAP_BODY_BYTES = 16 * 1024 * 1024;

// MAX_DAP_BUFFER_BYTES bounds all bytes retained before frame parsing catches up.
// MAX_DAP_BUFFER_BYTES 限制帧解析追赶前允许保留的全部字节数。
const MAX_DAP_BUFFER_BYTES = MAX_DAP_HEADER_BYTES + MAX_DAP_BODY_BYTES;

// CONNECT_TIMEOUT_MS bounds opening one CDP or DAP transport.
// CONNECT_TIMEOUT_MS 限制打开一个 CDP 或 DAP 传输的时长。
const CONNECT_TIMEOUT_MS = 10000;

// DEFAULT_REQUEST_TIMEOUT_MS is the protocol request wait used when a command omits timeout_ms.
// DEFAULT_REQUEST_TIMEOUT_MS 是命令省略 timeout_ms 时使用的协议请求等待时长。
const DEFAULT_REQUEST_TIMEOUT_MS = 30000;

// LOCAL_HOSTS is the exact endpoint allowlist enforced again inside the sidecar.
// LOCAL_HOSTS 是 sidecar 内再次执行的精确端点白名单。
const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]);

// FRAME_MARKER terminates one JSON response without colliding with JSON-escaped control characters.
// FRAME_MARKER 终止一条 JSON 响应，同时不会与 JSON 转义后的控制字符冲突。
const FRAME_MARKER = "\u001e";

// state owns the only active protocol transport and its bounded event queue.
// state 持有唯一活动协议传输及其有界事件队列。
const state = {
  protocol: null,
  transport: null,
  endpoint: null,
  connected: false,
  closed: false,
  lastError: null,
  nextSequence: 1,
  pending: new Map(),
  dapReverseRequests: new Map(),
  events: [],
  droppedEvents: 0,
  dapBuffer: Buffer.alloc(0),
  eventWaiters: [],
};

// input reads one JSON command per line from the managed session stdin.
// input 从受管会话 stdin 按行读取 JSON 命令。
const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

// Write one structured response without allowing protocol events to interleave with it.
// 写入一条结构化响应，并禁止协议事件与其交错。
function writeResponse(response) {
  let encoded = JSON.stringify(response);
  if (Buffer.byteLength(encoded, "utf8") > MAX_RESPONSE_BYTES) {
    encoded = JSON.stringify({
      request_id: response.request_id ?? null,
      ok: false,
      error: `debug sidecar response exceeds ${MAX_RESPONSE_BYTES} bytes`,
    });
  }
  process.stdout.write(`${encoded}${FRAME_MARKER}\n`);
}

// Convert one unknown failure into stable readable text.
// 把未知失败转换为稳定可读文本。
function errorText(error) {
  return error instanceof Error ? error.message : String(error);
}

// Require one finite non-negative integer from an untrusted command field.
// 从不可信命令字段中读取一个有限非负整数。
function nonNegativeInteger(value, fallback, field) {
  if (value === undefined || value === null) {
    return fallback;
  }
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${field} must be a non-negative integer`);
  }
  return value;
}

// Require one positive integer from an untrusted command field.
// 从不可信命令字段中读取一个正整数。
function positiveInteger(value, fallback, field) {
  const parsed = nonNegativeInteger(value, fallback, field);
  if (parsed <= 0) {
    throw new Error(`${field} must be a positive integer`);
  }
  return parsed;
}

// Resolve and validate one loopback hostname.
// 解析并校验一个回环主机名。
function localHost(value) {
  if (typeof value !== "string" || !LOCAL_HOSTS.has(value.toLowerCase())) {
    throw new Error("debug endpoints must use localhost, 127.0.0.1, or ::1");
  }
  return value;
}

// Publish one bounded protocol event and wake finite event waiters.
// 发布一个有界协议事件并唤醒有限事件等待者。
function enqueueEvent(event) {
  const encodedBytes = Buffer.byteLength(JSON.stringify(event), "utf8");
  if (encodedBytes > MAX_EVENT_BYTES) {
    if (event.protocol === "dap" && event.kind === "request") {
      throw new Error("DAP reverse request event exceeds the maximum size");
    }
    state.droppedEvents += 1;
    event = {
      protocol: event.protocol ?? state.protocol,
      kind: "oversized_event_dropped",
      original_kind: event.kind ?? null,
      encoded_bytes: encodedBytes,
    };
  }
  if (state.events.length >= MAX_EVENTS) {
    const removableIndex = state.events.findIndex(
      (retained) => retained.protocol !== "dap" || retained.kind !== "request",
    );
    if (removableIndex < 0) {
      if (event.protocol === "dap" && event.kind === "request") {
        throw new Error("DAP event queue is full of pending reverse requests");
      }
      state.droppedEvents += 1;
      return;
    }
    state.events.splice(removableIndex, 1);
    state.droppedEvents += 1;
  }
  state.events.push({ sequence: state.nextSequence++, ...event });
  const waiters = state.eventWaiters.splice(0);
  for (const wake of waiters) {
    wake();
  }
}

// Reject all in-flight protocol requests after transport failure or closure.
// 在传输失败或关闭后拒绝全部执行中的协议请求。
function rejectPending(reason) {
  for (const pending of state.pending.values()) {
    clearTimeout(pending.timer);
    pending.reject(new Error(reason));
  }
  state.pending.clear();
}

// Record one transport failure and wake pending consumers.
// 记录单次传输失败并唤醒等待中的消费者。
function failTransport(error) {
  const message = errorText(error);
  const transport = state.transport;
  state.transport = null;
  state.lastError = message;
  state.connected = false;
  state.closed = true;
  rejectPending(message);
  state.dapReverseRequests.clear();
  enqueueEvent({ protocol: state.protocol, kind: "transport_error", error: message });
  if (transport !== null) {
    if (state.protocol === "cdp") {
      transport.close();
    } else {
      transport.destroy();
    }
  }
}

// Wait for the current event queue to become non-empty or reach the deadline.
// 等待当前事件队列变为非空或到达截止时间。
async function waitForEvent(timeoutMs) {
  if (state.events.length > 0 || timeoutMs === 0) {
    return;
  }
  await new Promise((resolve) => {
    let completed = false;
    const finish = () => {
      if (completed) {
        return;
      }
      completed = true;
      clearTimeout(timer);
      const index = state.eventWaiters.indexOf(finish);
      if (index >= 0) {
        state.eventWaiters.splice(index, 1);
      }
      resolve();
    };
    const timer = setTimeout(finish, timeoutMs);
    state.eventWaiters.push(finish);
  });
}

// Open one local Chrome DevTools Protocol WebSocket.
// 打开一个本机 Chrome DevTools Protocol WebSocket。
async function openCdp(target) {
  if (typeof target?.websocket_url !== "string") {
    throw new Error("target.websocket_url is required for CDP");
  }
  const endpoint = new URL(target.websocket_url);
  if (endpoint.protocol !== "ws:" && endpoint.protocol !== "wss:") {
    throw new Error("CDP websocket_url must use ws or wss");
  }
  localHost(endpoint.hostname);

  const socket = new WebSocket(endpoint);
  await new Promise((resolve, reject) => {
    const finish = (error) => {
      socket.removeEventListener("open", onOpen);
      socket.removeEventListener("error", onError);
      clearTimeout(timer);
      if (error === null) {
        resolve();
      } else {
        reject(error);
      }
    };
    const onOpen = () => finish(null);
    const onError = () => finish(new Error("CDP WebSocket connection failed"));
    const timer = setTimeout(() => {
      finish(new Error("CDP connection timed out"));
      socket.close();
    }, CONNECT_TIMEOUT_MS);
    socket.addEventListener("open", onOpen, { once: true });
    socket.addEventListener("error", onError, { once: true });
  });
  socket.addEventListener("message", (event) => {
    try {
      const message = JSON.parse(String(event.data));
      if (Number.isInteger(message.id) && state.pending.has(message.id)) {
        const pending = state.pending.get(message.id);
        state.pending.delete(message.id);
        clearTimeout(pending.timer);
        if (message.error) {
          pending.reject(new Error(JSON.stringify(message.error)));
        } else {
          pending.resolve(message.result ?? null);
        }
        return;
      }
      if (typeof message.method === "string") {
        enqueueEvent({ protocol: "cdp", kind: "event", method: message.method, params: message.params ?? null });
      }
    } catch (error) {
      failTransport(error);
    }
  });
  socket.addEventListener("close", () => {
    state.connected = false;
    rejectPending("CDP WebSocket closed");
    enqueueEvent({ protocol: "cdp", kind: "closed" });
  });
  socket.addEventListener("error", () => failTransport(new Error("CDP WebSocket failed")));

  state.protocol = "cdp";
  state.transport = socket;
  state.endpoint = endpoint.toString();
  state.connected = true;
  state.closed = false;
}

// Parse every complete DAP Content-Length frame currently buffered.
// 解析当前缓冲区内所有完整的 DAP Content-Length 帧。
function parseDapFrames() {
  while (true) {
    const headerEnd = state.dapBuffer.indexOf("\r\n\r\n");
    if (headerEnd < 0) {
      if (state.dapBuffer.length > MAX_DAP_HEADER_BYTES) {
        throw new Error("DAP header exceeds the maximum size");
      }
      return;
    }
    if (headerEnd > MAX_DAP_HEADER_BYTES) {
      throw new Error("DAP header exceeds the maximum size");
    }
    const header = state.dapBuffer.subarray(0, headerEnd).toString("ascii");
    const lengthLine = header.split("\r\n").find((line) => /^Content-Length:/i.test(line));
    if (!lengthLine) {
      throw new Error("DAP frame is missing Content-Length");
    }
    const contentLengthText = lengthLine.split(":", 2)[1].trim();
    if (!/^\d+$/.test(contentLengthText)) {
      throw new Error("DAP Content-Length is invalid");
    }
    const contentLength = Number.parseInt(contentLengthText, 10);
    if (!Number.isSafeInteger(contentLength) || contentLength > MAX_DAP_BODY_BYTES) {
      throw new Error("DAP Content-Length is invalid");
    }
    const bodyStart = headerEnd + 4;
    const frameEnd = bodyStart + contentLength;
    if (state.dapBuffer.length < frameEnd) {
      return;
    }
    const body = state.dapBuffer.subarray(bodyStart, frameEnd).toString("utf8");
    state.dapBuffer = state.dapBuffer.subarray(frameEnd);
    handleDapMessage(JSON.parse(body));
  }
}

// Route one decoded DAP response or event.
// 路由一条解码后的 DAP 响应或事件。
function handleDapMessage(message) {
  if (message.type === "response" && Number.isInteger(message.request_seq) && state.pending.has(message.request_seq)) {
    const pending = state.pending.get(message.request_seq);
    state.pending.delete(message.request_seq);
    clearTimeout(pending.timer);
    if (message.success === false) {
      pending.reject(new Error(message.message || "DAP request failed"));
    } else {
      pending.resolve(message.body ?? null);
    }
    return;
  }
  if (message.type === "event" && typeof message.event === "string") {
    enqueueEvent({ protocol: "dap", kind: "event", event: message.event, body: message.body ?? null });
    return;
  }
  if (message.type === "request" && Number.isInteger(message.seq) && typeof message.command === "string") {
    if (state.dapReverseRequests.has(message.seq)) {
      throw new Error(`duplicate DAP reverse request sequence: ${message.seq}`);
    }
    if (state.dapReverseRequests.size >= MAX_EVENTS) {
      throw new Error("too many pending DAP reverse requests");
    }
    state.dapReverseRequests.set(message.seq, message.command);
    enqueueEvent({
      protocol: "dap",
      kind: "request",
      request_seq: message.seq,
      command: message.command,
      arguments: message.arguments ?? null,
    });
  }
}

// Open one local TCP Debug Adapter Protocol endpoint.
// 打开一个本机 TCP Debug Adapter Protocol 端点。
async function openDap(target) {
  const host = localHost(target?.host);
  const port = positiveInteger(target?.port, 0, "target.port");
  if (port > 65535) {
    throw new Error("target.port must not exceed 65535");
  }
  const socket = net.createConnection({ host, port });
  await new Promise((resolve, reject) => {
    const finish = (error) => {
      socket.off("connect", onConnect);
      socket.off("error", onError);
      clearTimeout(timer);
      if (error === null) {
        resolve();
      } else {
        reject(error);
      }
    };
    const onConnect = () => finish(null);
    const onError = (error) => finish(error);
    const timer = setTimeout(() => {
      finish(new Error("DAP connection timed out"));
      socket.destroy();
    }, CONNECT_TIMEOUT_MS);
    socket.once("connect", onConnect);
    socket.once("error", onError);
  });
  socket.on("data", (chunk) => {
    try {
      if (state.dapBuffer.length + chunk.length > MAX_DAP_BUFFER_BYTES) {
        throw new Error("DAP receive buffer exceeds the maximum size");
      }
      state.dapBuffer = Buffer.concat([state.dapBuffer, chunk]);
      parseDapFrames();
    } catch (error) {
      failTransport(error);
    }
  });
  socket.on("close", () => {
    state.connected = false;
    rejectPending("DAP socket closed");
    enqueueEvent({ protocol: "dap", kind: "closed" });
  });
  socket.on("error", failTransport);

  state.protocol = "dap";
  state.transport = socket;
  state.endpoint = `${host}:${port}`;
  state.connected = true;
  state.closed = false;
  state.dapBuffer = Buffer.alloc(0);
}

// Open the single protocol transport owned by this sidecar process.
// 打开当前 sidecar 进程持有的唯一协议传输。
async function openProtocol(command) {
  if (state.transport !== null && !state.closed) {
    throw new Error("debug protocol is already open");
  }
  if (command.protocol === "cdp") {
    await openCdp(command.target);
  } else if (command.protocol === "dap") {
    await openDap(command.target);
  } else {
    throw new Error("protocol must be cdp or dap");
  }
  return statusSnapshot();
}

// Send one CDP request and resolve its correlated response.
// 发送一条 CDP 请求并解析其关联响应。
function requestCdp(command) {
  if (typeof command.method !== "string" || command.method.trim() === "") {
    throw new Error("method is required for CDP requests");
  }
  const id = state.nextSequence++;
  const timeoutMs = positiveInteger(command.timeout_ms, DEFAULT_REQUEST_TIMEOUT_MS, "timeout_ms");
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      state.pending.delete(id);
      reject(new Error(`CDP request timed out: ${command.method}`));
    }, timeoutMs);
    state.pending.set(id, { resolve, reject, timer });
    try {
      state.transport.send(JSON.stringify({ id, method: command.method, params: command.params ?? {} }));
    } catch (error) {
      clearTimeout(timer);
      state.pending.delete(id);
      reject(error);
    }
  });
}

// Send one DAP request and resolve its correlated response.
// 发送一条 DAP 请求并解析其关联响应。
function requestDap(command) {
  if (typeof command.command !== "string" || command.command.trim() === "") {
    throw new Error("command is required for DAP requests");
  }
  const seq = state.nextSequence++;
  const timeoutMs = positiveInteger(command.timeout_ms, DEFAULT_REQUEST_TIMEOUT_MS, "timeout_ms");
  const message = JSON.stringify({
    seq,
    type: "request",
    command: command.command,
    arguments: command.arguments ?? {},
  });
  const frame = `Content-Length: ${Buffer.byteLength(message, "utf8")}\r\n\r\n${message}`;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      state.pending.delete(seq);
      reject(new Error(`DAP request timed out: ${command.command}`));
    }, timeoutMs);
    state.pending.set(seq, { resolve, reject, timer });
    try {
      state.transport.write(frame, "utf8", (error) => {
        if (error) {
          clearTimeout(timer);
          state.pending.delete(seq);
          reject(error);
        }
      });
    } catch (error) {
      clearTimeout(timer);
      state.pending.delete(seq);
      reject(error);
    }
  });
}

// Send one DAP response for an adapter-to-client reverse request.
// 为一条 adapter 到客户端的反向请求发送 DAP 响应。
function respondDap(command) {
  const requestSeq = positiveInteger(command.request_seq, 0, "request_seq");
  if (typeof command.command !== "string" || command.command.trim() === "") {
    throw new Error("command is required for DAP responses");
  }
  if (typeof command.success !== "boolean") {
    throw new Error("success is required for DAP responses");
  }
  const expectedCommand = state.dapReverseRequests.get(requestSeq);
  if (expectedCommand === undefined) {
    throw new Error(`DAP reverse request ${requestSeq} is not pending`);
  }
  if (expectedCommand !== command.command) {
    throw new Error(`DAP reverse request ${requestSeq} expects command ${expectedCommand}`);
  }
  const seq = state.nextSequence++;
  const response = {
    seq,
    type: "response",
    request_seq: requestSeq,
    success: command.success,
    command: command.command,
    body: command.body ?? {},
  };
  if (typeof command.message === "string" && command.message !== "") {
    response.message = command.message;
  }
  const message = JSON.stringify(response);
  const frame = `Content-Length: ${Buffer.byteLength(message, "utf8")}\r\n\r\n${message}`;
  return new Promise((resolve, reject) => {
    state.transport.write(frame, "utf8", (error) => {
      if (error) {
        reject(error);
      } else {
        state.dapReverseRequests.delete(requestSeq);
        resolve({ sent: true, seq, request_seq: requestSeq });
      }
    });
  });
}

// Send one request through the active protocol transport.
// 通过活动协议传输发送一条请求。
async function requestProtocol(command) {
  if (!state.connected || state.transport === null) {
    throw new Error("debug protocol is not connected");
  }
  if (state.protocol === "cdp") {
    return requestCdp(command);
  }
  if (command.request_seq !== undefined) {
    return respondDap(command);
  }
  return requestDap(command);
}

// Drain a bounded number of retained protocol events after an optional finite wait.
// 在可选有限等待后排空有限数量的已保留协议事件。
async function drainEvents(command) {
  const maxEvents = positiveInteger(command.max_events, MAX_EVENT_PAGE, "max_events");
  if (maxEvents > MAX_EVENT_PAGE) {
    throw new Error(`max_events must not exceed ${MAX_EVENT_PAGE}`);
  }
  const timeoutMs = nonNegativeInteger(command.timeout_ms, 0, "timeout_ms");
  await waitForEvent(timeoutMs);
  const events = state.events.splice(0, maxEvents);
  return {
    events,
    remaining: state.events.length,
    timed_out: events.length === 0 && timeoutMs > 0,
    dropped_events: state.droppedEvents,
  };
}

// Return the current protocol and queue status.
// 返回当前协议与队列状态。
function statusSnapshot() {
  return {
    protocol: state.protocol,
    endpoint: state.endpoint,
    connected: state.connected,
    closed: state.closed,
    pending_requests: state.pending.size,
    pending_reverse_requests: state.dapReverseRequests.size,
    buffered_events: state.events.length,
    dropped_events: state.droppedEvents,
    last_error: state.lastError,
  };
}

// Close the active protocol transport and reject incomplete requests.
// 关闭活动协议传输并拒绝未完成请求。
async function closeProtocol() {
  if (state.transport === null) {
    state.closed = true;
    state.connected = false;
    return statusSnapshot();
  }
  const transport = state.transport;
  state.transport = null;
  state.connected = false;
  state.closed = true;
  rejectPending("debug protocol closed by host");
  state.dapReverseRequests.clear();
  if (state.protocol === "cdp") {
    transport.close();
  } else {
    transport.end();
    setTimeout(() => transport.destroy(), 1000).unref();
  }
  return statusSnapshot();
}

// Execute one validated host operation.
// 执行一项已校验的宿主操作。
async function executeCommand(command) {
  switch (command.op) {
    case "open":
      return openProtocol(command);
    case "request":
      return requestProtocol(command);
    case "events":
      return drainEvents(command);
    case "status":
      return statusSnapshot();
    case "close":
      return closeProtocol();
    default:
      throw new Error("op must be open, request, events, status, or close");
  }
}

// Parse and execute one stdin command, always returning one response line.
// 解析并执行一条 stdin 命令，并始终返回一行响应。
async function handleLine(line) {
  let command;
  try {
    command = JSON.parse(line);
    if (command === null || Array.isArray(command) || typeof command !== "object") {
      throw new Error("command must be a JSON object");
    }
    if (typeof command.request_id !== "string" || command.request_id.trim() === "") {
      throw new Error("request_id is required");
    }
    const result = await executeCommand(command);
    writeResponse({ request_id: command.request_id, ok: true, result });
  } catch (error) {
    writeResponse({
      request_id: typeof command?.request_id === "string" ? command.request_id : null,
      ok: false,
      error: errorText(error),
    });
  }
}

// commandChain serializes stdin operations so one response maps to exactly one Lua read.
// commandChain 串行化 stdin 操作，确保一条响应精确对应一次 Lua 读取。
let commandChain = Promise.resolve();
input.on("line", (line) => {
  commandChain = commandChain.then(() => handleLine(line));
});
input.on("close", () => {
  commandChain.finally(() => closeProtocol()).finally(() => process.exit(0));
});

process.on("SIGTERM", () => {
  closeProtocol().finally(() => process.exit(0));
});
