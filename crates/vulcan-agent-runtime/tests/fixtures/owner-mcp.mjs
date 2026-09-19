// Read newline-delimited JSON-RPC messages from the host-owned stdio connection.
// 从宿主持有的标准输入输出连接读取逐行 JSON-RPC 消息。
import { createInterface } from 'node:readline';
import { writeFileSync } from 'node:fs';

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
    return {
      protocolVersion: request.params.protocolVersion,
      capabilities: { tools: {} },
      serverInfo: { name: 'vulcan-owner-fixture', version: '1.0.0' },
    };
  }
  if (request.method === 'tools/list') {
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
