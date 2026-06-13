import { parseUnifiedDiff } from "./parseDiff";
import type { DiffPanelSnapshot, DiffPanelSource } from "./types";

// DiffParseWorkerRequest asks the worker to parse one unified diff payload.
// DiffParseWorkerRequest 请求 Worker 解析一个 unified diff 载荷。
interface DiffParseWorkerRequest {
  // Request id echoed back to the main thread.
  // 回传给主线程的请求标识。
  id: number;
  // Raw unified diff text returned by the backend.
  // 后端返回的原始 unified diff 文本。
  rawDiff: string;
  // Diff source label used by the review renderer.
  // 审核渲染器使用的 diff 来源标签。
  source: DiffPanelSource;
  // Repository root shown by the review renderer.
  // 审核渲染器展示的仓库根路径。
  repositoryRoot: string;
}

// DiffParseWorkerResponse returns a parsed snapshot or an error.
// DiffParseWorkerResponse 返回解析后的快照或错误。
interface DiffParseWorkerResponse {
  // Request id echoed from the request.
  // 从请求中回传的请求标识。
  id: number;
  // Parsed diff snapshot when parsing succeeds.
  // 解析成功时得到的 diff 快照。
  snapshot?: DiffPanelSnapshot;
  // Optional error message returned to the worker-only caller.
  // 返回给 Worker 单轨调用方的可选错误信息。
  error?: string;
}

self.onmessage = (event: MessageEvent<DiffParseWorkerRequest>) => {
  const request = event.data;
  try {
    const response: DiffParseWorkerResponse = {
      id: request.id,
      snapshot: parseUnifiedDiff(request.rawDiff, request.source, request.repositoryRoot),
    };
    self.postMessage(response);
  } catch (error) {
    const response: DiffParseWorkerResponse = {
      id: request.id,
      error: error instanceof Error ? error.message : String(error),
    };
    self.postMessage(response);
  }
};
