// Parse only actual newline-delimited MCP requests from the owning runtime.
// 仅解析所属运行时发送的实际逐行 MCP 请求。
import { createInterface } from 'node:readline';
import { appendFileSync } from 'node:fs';

// Record actual selected inputs before protocol work, under an isolated test directory.
// 在隔离测试目录下，先于协议工作记录实际选定输入。
const selectedInputs = {
  pid: process.pid,
  label: process.env.CANDIDATE_LABEL,
  token: process.env.CANDIDATE_TOKEN,
};
appendFileSync(process.env.CANDIDATE_MARKER, `${JSON.stringify(selectedInputs)}\n`, 'utf8');

// The declared mode is a fixed fixture input, never inferred from host timing.
// 声明模式为固定夹具输入，绝不从宿主计时推断。
const mode = process.env.CANDIDATE_MODE;

// Answer one actual MCP request; request owns its ID and the returned value is the exact result.
// 回答单个实际 MCP 请求；request 持有其标识，返回值为精确结果。
async function answer(request) {
  if (request.method === 'initialize') {
    if (mode === 'fail') throw new Error('Declared candidate initialization failure.');
    if (mode === 'stall') {
      // Hold initialization until host cancellation, with a finite fixture safety deadline.
      // 保持初始化直至宿主取消，并设置有限夹具安全期限。
      await new Promise((resolve) => setTimeout(resolve, 30_000));
    }
    return {
      protocolVersion: request.params.protocolVersion,
      capabilities: { tools: {} },
      serverInfo: { name: selectedInputs.label, version: '1.0.0' },
    };
  }
  if (request.method === 'tools/list') return { tools: [] };
  throw new Error(`Unexpected candidate request: ${request.method}`);
}

// Closing the actual input ends the real process after all current request work completes.
// 在当前请求工作完成后，关闭实际输入会结束真实进程。
const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of input) {
  // Notifications have no response; request IDs remain unchanged for protocol correctness.
  // 通知没有响应；请求标识保持不变以符合协议。
  const request = JSON.parse(line);
  if (request.id === undefined) continue;
  try {
    // Build the response only after the exact request completed.
    // 仅在精确请求完成后构建响应。
    const result = await answer(request);
    process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id: request.id, result })}\n`);
  } catch (error) {
    // A declared failure remains an actual JSON-RPC error rather than a fabricated readiness state.
    // 声明失败保持为实际 JSON-RPC 错误，而非伪造的就绪状态。
    const response = {
      jsonrpc: '2.0', id: request.id,
      error: { code: -32603, message: String(error) },
    };
    process.stdout.write(`${JSON.stringify(response)}\n`);
  }
}
