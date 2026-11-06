import type { CommandSessionSummary } from "./runtime-context";

// TerminalDisplayChunk carries one ordered terminal output fragment.
// TerminalDisplayChunk 承载一个有序终端输出片段。
export interface TerminalDisplayChunk {
  // Monotonic runtime output sequence.
  // 单调递增的运行时输出序号。
  seq: number;
  // Stable stdout or stderr label.
  // 稳定的 stdout 或 stderr 标签。
  stream: string;
  // Decoded terminal text including ANSI control sequences.
  // 包含 ANSI 控制序列的已解码终端文本。
  text: string;
}

// CommandSessionDisplayAttachment initializes one visible read-only AI terminal.
// CommandSessionDisplayAttachment 初始化一个可见的只读 AI 终端。
export interface CommandSessionDisplayAttachment {
  // Opaque visible-display subscription identity.
  // 不透明可见显示订阅标识。
  subscriptionId: string;
  // Current command-session summary.
  // 当前命令会话摘要。
  terminal: CommandSessionSummary;
  // Bounded retained output tail.
  // 有界保留输出尾部。
  chunks: TerminalDisplayChunk[];
}

// CommandSessionDisplayChangedEvent pushes output only to an attached visible AI terminal.
// CommandSessionDisplayChangedEvent 仅向已挂接的可见 AI 终端推送输出。
export interface CommandSessionDisplayChangedEvent extends CommandSessionDisplayAttachment {
  // Browser/WebView lifetime selected by the event socket before transport delivery.
  // 事件套接字在传输交付前选择的浏览器或 WebView 生命周期。
  displayClientId: string;
}

// ShellCommandOutputAttachment returns the recent output of a running shell_command call when its card opens.
// ShellCommandOutputAttachment 在卡片展开时返回运行中 shell_command 调用的最近输出。
export interface ShellCommandOutputAttachment {
  // Subscription for later changes; null when the command is not running in this Core.
  // 后续变化的订阅；命令不在本 Core 中运行时为 null。
  subscriptionId: string | null;
  // Whether the command is still running.
  // 命令是否仍在运行。
  running: boolean;
  // Most recent output lines, oldest first.
  // 最近的输出行，按从旧到新排列。
  lines: string[];
  // Number of most recent lines shown at most.
  // 最多显示的最近行数。
  lineLimit: number;
}

// ShellCommandOutputChangedEvent replaces the recent lines shown by one open command card.
// ShellCommandOutputChangedEvent 替换一张已展开命令卡显示的最近输出行。
export interface ShellCommandOutputChangedEvent extends ShellCommandOutputAttachment {
  // Browser/WebView lifetime selected by the event socket before transport delivery.
  // 事件套接字在传输交付前选择的浏览器或 WebView 生命周期。
  displayClientId: string;
  // Subscription receiving this event.
  // 接收该事件的订阅。
  subscriptionId: string;
}

// ProjectTerminalShellOption describes one detected shell launcher.
// ProjectTerminalShellOption 描述一个已检测 Shell 启动器。
export interface ProjectTerminalShellOption {
  // Stable shell family id.
  // 稳定 Shell 族标识。
  id: string;
  // User-facing shell label.
  // 用户可见 Shell 标签。
  label: string;
  // Whether current auto/default policy selects this shell.
  // 当前自动或默认策略是否选择该 Shell。
  default: boolean;
}

// ProjectTerminalSummary describes one terminal owned by a project.
// ProjectTerminalSummary 描述一个由项目持有的终端。
export interface ProjectTerminalSummary {
  // Stable terminal identity.
  // 稳定终端标识。
  id: string;
  // Stable owning project identity.
  // 稳定归属项目标识。
  projectId: string;
  // User-facing terminal name.
  // 用户可见终端名称。
  name: string;
  // Selected shell family id.
  // 已选择 Shell 族标识。
  shellId: string;
  // Exact working directory.
  // 精确工作目录。
  cwd: string;
  // Stable terminal lifecycle status.
  // 稳定终端生命周期状态。
  status: "running" | "completed" | "failed" | "stopped";
  // Optional child exit code.
  // 可选子进程退出码。
  exitCode?: number | null;
  // Optional terminal failure reason.
  // 可选终端故障原因。
  failureReason?: string | null;
  // Highest runtime output sequence.
  // 运行时最高输出序号。
  lastSeq: number;
  // Highest output sequence evicted from the bounded display checkpoint.
  // 已从有界显示检查点淘汰的最高输出序号。
  droppedBeforeSeq: number;
  // Monotonic revision for user-visible terminal metadata.
  // 用户可见终端元数据的单调修订号。
  stateRevision: number;
  // Highest contiguous browser input sequence acknowledged by the host pipe.
  // 宿主管道已确认的最高连续浏览器输入序号。
  lastAcceptedInputSeq: number;
  // Current terminal rows.
  // 当前终端行数。
  rows: number;
  // Current terminal columns.
  // 当前终端列数。
  cols: number;
}

// ProjectTerminalCatalog contains shells and terminals for one explicit project.
// ProjectTerminalCatalog 包含一个显式项目的 Shell 与终端。
export interface ProjectTerminalCatalog {
  // Exact project identity.
  // 精确项目标识。
  projectId: string;
  // Detected shell choices.
  // 已检测 Shell 选择项。
  shells: ProjectTerminalShellOption[];
  // Project-owned terminal tabs.
  // 项目级终端标签集合。
  terminals: ProjectTerminalSummary[];
  // Monotonic revision for the project's running terminal count.
  // 当前项目运行中终端数量的单调修订号。
  runningCountRevision: number;
}

// ProjectTerminalRunningCountChangedEvent carries lifecycle metadata without terminal content.
// ProjectTerminalRunningCountChangedEvent 承载不包含终端内容的生命周期元数据。
export interface ProjectTerminalRunningCountChangedEvent {
  // Stable project identity owning the counted terminals.
  // 被统计终端所属的稳定项目标识。
  projectId: string;
  // Number of terminals whose lifecycle status is running.
  // 生命周期状态为运行中的终端数量。
  runningCount: number;
  // Monotonic revision used to reject stale metadata queries.
  // 用于拒绝过期元数据查询的单调修订号。
  revision: number;
}

// ProjectTerminalDisplayAttachment initializes one visible interactive project terminal.
// ProjectTerminalDisplayAttachment 初始化一个可见的交互式项目终端。
export interface ProjectTerminalDisplayAttachment {
  // Opaque visible-display subscription identity.
  // 不透明可见显示订阅标识。
  subscriptionId: string;
  // Current project terminal summary.
  // 当前项目终端摘要。
  terminal: ProjectTerminalSummary;
  // Bounded retained output tail.
  // 有界保留输出尾部。
  chunks: TerminalDisplayChunk[];
}

// ProjectTerminalDisplayChangedEvent pushes output only to an attached visible project terminal.
// ProjectTerminalDisplayChangedEvent 仅向已挂接的可见项目终端推送输出。
export interface ProjectTerminalDisplayChangedEvent extends ProjectTerminalDisplayAttachment {
  // Browser/WebView lifetime selected by the event socket before transport delivery.
  // 事件套接字在传输交付前选择的浏览器或 WebView 生命周期。
  displayClientId: string;
}
