import type { ProjectTerminalInputEventRequest } from "@/service/terminal-service";

// QueuedProjectTerminalInput stores one unacknowledged sequenced xterm input event.
// QueuedProjectTerminalInput 保存一个未确认且带序号的 xterm 输入事件。
export interface QueuedProjectTerminalInput {
  // Monotonic browser input sequence.
  // 单调递增的浏览器输入序号。
  seq: number;
  // Exact service request representation.
  // 精确的服务请求表示。
  event: ProjectTerminalInputEventRequest;
  // Encoded byte length used for bounded batching.
  // 用于有界批处理的编码字节长度。
  byteLength: number;
}

// ProjectTerminalInputBuffer retains unacknowledged input independently from one visible React mount.
// ProjectTerminalInputBuffer 独立于单次可见 React 挂载保留未确认输入。
export interface ProjectTerminalInputBuffer {
  // Exact unacknowledged input events in browser sequence order.
  // 按浏览器序号排列的精确未确认输入事件。
  pending: QueuedProjectTerminalInput[];
  // Total bytes retained by pending events.
  // 待处理事件保留的总字节数。
  pendingBytes: number;
  // Next browser sequence allocated for this exact terminal.
  // 为当前精确终端分配的下一个浏览器序号。
  nextClientSeq: number;
  // Whether one service input request remains in flight across a surface unmount.
  // 一个服务输入请求是否在界面卸载后仍处于进行中。
  sending: boolean;
  // Settlement boundary awaited before an authoritative reconnect query.
  // 权威重连查询前等待的结算边界。
  batchSettled: Promise<void> | null;
}

// projectTerminalInputBuffers owns stable terminal buffers under their exact project identity.
// projectTerminalInputBuffers 在精确项目标识下持有稳定的终端缓冲区。
const projectTerminalInputBuffers = new Map<string, Map<string, ProjectTerminalInputBuffer>>();

// getOrCreateProjectTerminalInputBuffer returns the stable delivery state for one exact project terminal.
// getOrCreateProjectTerminalInputBuffer 返回一个精确项目终端的稳定交付状态。
// `acknowledgedSeq` initializes only a newly observed terminal; existing unacknowledged state is preserved.
// `acknowledgedSeq` 仅初始化首次观察到的终端；既有未确认状态会被保留。
//
// Parameters / 参数:
// - `projectId`: Exact project identity. / 精确项目标识。
// - `terminalId`: Exact terminal identity. / 精确终端标识。
// - `acknowledgedSeq`: Highest service-acknowledged browser sequence. / 服务已确认的最高浏览器序号。
//
// Returns / 返回值:
// Returns the stable project-terminal delivery buffer.
// 返回稳定的项目终端交付缓冲区。
export function getOrCreateProjectTerminalInputBuffer(
  projectId: string,
  terminalId: string,
  acknowledgedSeq: number,
): ProjectTerminalInputBuffer {
  let terminalBuffers = projectTerminalInputBuffers.get(projectId);
  if (!terminalBuffers) {
    terminalBuffers = new Map<string, ProjectTerminalInputBuffer>();
    projectTerminalInputBuffers.set(projectId, terminalBuffers);
  }
  const existing = terminalBuffers.get(terminalId);
  if (existing) return existing;
  const created: ProjectTerminalInputBuffer = {
    pending: [],
    pendingBytes: 0,
    nextClientSeq: acknowledgedSeq + 1,
    sending: false,
    batchSettled: null,
  };
  terminalBuffers.set(terminalId, created);
  return created;
}

// clearProjectTerminalInputBuffer releases delivery state only after the exact terminal is closed.
// clearProjectTerminalInputBuffer 仅在精确终端关闭后释放其交付状态。
//
// Parameters / 参数:
// - `projectId`: Exact owning project identity. / 精确归属项目标识。
// - `terminalId`: Exact closed terminal identity. / 精确已关闭终端标识。
//
// Returns / 返回值:
// Returns nothing after removing the matching buffer and any empty project bucket.
// 移除匹配缓冲区及空项目分组后无返回值。
export function clearProjectTerminalInputBuffer(projectId: string, terminalId: string): void {
  const terminalBuffers = projectTerminalInputBuffers.get(projectId);
  if (!terminalBuffers) return;
  terminalBuffers.delete(terminalId);
  if (terminalBuffers.size === 0) projectTerminalInputBuffers.delete(projectId);
}

// clearProjectTerminalInputBuffers releases every browser delivery buffer after project removal commits.
// clearProjectTerminalInputBuffers 在项目移除提交后释放其全部浏览器交付缓冲区。
//
// Parameters / 参数:
// - `projectId`: Exact removed project identity. / 精确已移除项目标识。
//
// Returns / 返回值:
// Returns nothing after removing the complete project-owned buffer bucket.
// 移除完整项目级缓冲分组后无返回值。
export function clearProjectTerminalInputBuffers(projectId: string): void {
  projectTerminalInputBuffers.delete(projectId);
}
