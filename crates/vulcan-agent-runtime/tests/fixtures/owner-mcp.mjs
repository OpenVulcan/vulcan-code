// Read newline-delimited JSON-RPC messages from the host-owned stdio connection.
// 从宿主持有的标准输入输出连接读取逐行 JSON-RPC 消息。
import { createInterface } from 'node:readline';
import { appendFileSync, writeFileSync } from 'node:fs';

// An optional marker records every real process start for lifecycle tests.
// 可选标记记录每次真实进程启动，供生命周期测试计数。
const processMarker = process.env.VULCAN_MCP_PROCESS_MARKER;
if (processMarker) appendFileSync(processMarker, `${process.pid}\n`, 'utf8');

// The fixture exposes one tool whose owner argument must be hidden from the model.
// 本夹具暴露一个工具，其所有者参数必须对模型隐藏。
const readPageTool = {
  name: 'read_page',
  description: 'Return the supplied page and trusted owner session.',
  inputSchema: {
    type: 'object',
    properties: {
      url: { type: 'string' },
      owner_session_id: { type: 'string' },
    },
    required: ['url', 'owner_session_id'],
    additionalProperties: false,
  },
};

// Answer a single MCP request while retaining the raw arguments for process-level assertions.
// 回答单个 MCP 请求，并保留原始参数以供进程级断言。
async function handleRequest(request) {
  if (request.method === 'initialize') {
    if (process.env.VULCAN_MCP_STALL_INITIALIZE === '1') {
      // This pause tests cancellation before transport initialization returns to its owner.
      // 此停顿测试传输初始化返回所有者之前的取消行为。
      await new Promise((resolve) => setTimeout(resolve, 30_000));
    }
    return {
      protocolVersion: request.params.protocolVersion,
      capabilities: { tools: {} },
      serverInfo: { name: 'vulcan-owner-fixture', version: '1.0.0' },
    };
  }
  if (request.method === 'tools/list') {
    if (process.env.VULCAN_MCP_DISCOVERY_MARKER) {
      // The marker proves the first tool discovery reached the real child before cancellation.
      // 该标记证明首次工具发现在取消前已到达真实子进程。
      writeFileSync(process.env.VULCAN_MCP_DISCOVERY_MARKER, `${process.pid}\n`, 'utf8');
    }
    if (process.env.VULCAN_MCP_STALL_DISCOVERY === '1') {
      // A bounded stall lets cancellation tests interrupt a real initialized child.
      // 有界停顿允许取消测试中断真实的已初始化子进程。
      await new Promise((resolve) => setTimeout(resolve, 30_000));
    }
    return { tools: [readPageTool] };
  }
  if (request.method === 'tools/call') {
    if (request.params.name !== readPageTool.name) {
      throw new Error(`Unexpected tool: ${request.params.name}`);
    }
    if (request.params.arguments.url === 'https://stall.test') {
      // The marker proves the old child admitted this call before its plugin is drained.
      // 标记文件证明旧子进程已在插件排空前接受本次调用。
      writeFileSync(process.env.VULCAN_W14_STALL_MARKER,
        JSON.stringify(request.params.arguments), 'utf8');
      await new Promise((resolve) => setTimeout(resolve, 30_000));
    }
    return {
      content: [{
        type: 'text',
        text: JSON.stringify({
          url: request.params.arguments.url,
          owner_session_id: request.params.arguments.owner_session_id,
          process_id: process.pid,
        }),
      }],
    };
  }
  return {};
}

// The line reader gives each owner partition a separate, long-lived MCP process.
// 逐行读取器让每个所有者分区拥有独立且常驻的 MCP 进程。
const lineReader = createInterface({ input: process.stdin, crlfDelay: Infinity });

for await (const line of lineReader) {
  // A notification has no request ID and must not receive a JSON-RPC response.
  // 通知没有请求 ID，因此不能返回 JSON-RPC 响应。
  const request = JSON.parse(line);
  if (request.id === undefined) continue;
  try {
    // The response keeps MCP's exact JSON-RPC request ID for concurrent calls.
    // 响应保留 MCP 的精确 JSON-RPC 请求 ID，以支持并发调用。
    const response = { jsonrpc: '2.0', id: request.id, result: await handleRequest(request) };
    process.stdout.write(`${JSON.stringify(response)}\n`);
  } catch (error) {
    // A protocol error remains explicit instead of silently dropping the call.
    // 协议错误显式返回，不静默丢弃调用。
    const response = {
      jsonrpc: '2.0',
      id: request.id,
      error: { code: -32603, message: String(error) },
    };
    process.stdout.write(`${JSON.stringify(response)}\n`);
  }
}
