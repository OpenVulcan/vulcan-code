export interface CommandSessionSummary {
  // Stable command session identifier.
  // 稳定命令会话标识。
  id: string;
  // Optional user-facing terminal session name.
  // 可选用户可见终端会话名称。
  name?: string | null;
  // Command text.
  // 命令文本。
  command: string;
  // Command status.
  // 命令状态。
  status: string;
  // Recent output preview.
  // 最近输出预览。
  preview: string;
  // Optional owner conversation id when the runtime reports one.
  // 运行时报告时的可选归属会话标识。
  ownerSessionId?: string | null;
  // Effective working directory used by the command session.
  // 命令会话使用的实际工作目录。
  cwd?: string | null;
  // Effective shell label selected by the host.
  // 宿主选择的实际 shell 标签。
  shellLabel?: string | null;
  // Concrete backend label selected by the host.
  // 宿主选择的具体后端标签。
  backend?: string | null;
  // Latest terminal screen frame sequence.
  // 最新终端屏幕帧序号。
  screenFrameSeq?: number | null;
  // Latest terminal screen row count.
  // 最新终端屏幕行数。
  screenRows?: number | null;
  // Latest terminal screen column count.
  // 最新终端屏幕列数。
  screenCols?: number | null;
  // Latest renderable terminal screen text.
  // 最新可渲染终端屏幕内容。
  screenText?: string | null;
  // Optional process exit code captured by the host.
  // 宿主捕获到的可选进程退出码。
  exitCode?: number | null;
  // Optional failure reason synthesized by the host runtime.
  // 宿主运行时综合生成的可选失败原因。
  failureReason?: string | null;
  // Whether the summary represents a historical session missing from live runtime state.
  // 摘要是否代表实时运行时中已找不到的历史会话。
  lost?: boolean;
}

// LogEntrySummary describes one diagnostic log entry.
// LogEntrySummary 描述单条诊断日志。
export interface LogEntrySummary {
  // Stable log entry identifier.
  // 稳定日志条目标识。
  id: string;
  // Log level.
  // 日志级别。
  level: string;
  // Log source.
  // 日志来源。
  source: string;
  // Log message.
  // 日志消息。
  message: string;
}

// ContextInjectionItemSummary describes one visible injected context item.
// ContextInjectionItemSummary 描述单条可见注入上下文条目。
export interface ContextInjectionItemSummary {
  // Compact content preview displayed in the side panel.
  // 侧面板展示的紧凑内容预览。
  content: string;
  // Full detail text displayed as expanded context information.
  // 展开上下文信息时展示的完整详情文本。
  detail: string;
  // Optional turn label that owns this injection.
  // 拥有该注入的可选轮次标签。
  turnLabel?: string | null;
  // TTL or persistence label.
  // TTL 或持久化状态标签。
  ttlLabel: string;
  // Optional source display label.
  // 可选来源显示标签。
  sourceDisplay?: string | null;
  // Optional stable injection dedupe key.
  // 可选稳定注入去重键。
  dedupeKey?: string | null;
  // Optional durable VMM memory id.
  // 可选长期 VMM 记忆标识。
  memoryId?: number | null;
  // Optional source turn id.
  // 可选来源回合标识。
  turnId?: number | null;
  // Optional relevance score label.
  // 可选相关性评分标签。
  scoreLabel?: string | null;
  // Whether full turn detail can be queried.
  // 是否可以查询完整回合详情。
  hasDialogue?: boolean;
  // Optional VMM creation datetime.
  // 可选 VMM 创建时间。
  createdDatetime?: string | null;
  // Optional injection update timestamp.
  // 可选注入更新时间戳。
  updatedAt?: string | null;
}

// ContextInjectionCardSummary describes one context injection source group.
// ContextInjectionCardSummary 描述单个上下文注入来源分组。
export interface ContextInjectionCardSummary {
  // Stable context injection tag.
  // 稳定上下文注入标记。
  tag: string;
  // Human-readable card description.
  // 面向用户的卡片说明。
  description: string;
  // Items emitted by this injection source.
  // 当前注入来源产生的条目。
  items: ContextInjectionItemSummary[];
}

// VmmStaticProfileNodeSummary describes one active static VMM profile node.
// VmmStaticProfileNodeSummary 描述单个 active 静态 VMM 画像节点。
export interface VmmStaticProfileNodeSummary {
  // Durable VMM profile node id.
  // 长期 VMM 画像节点标识。
  id: number;
  // Priority marker such as P1.
  // 优先级标记，例如 P1。
  priority: string;
  // Lifetime marker such as L2.
  // 生命周期标记，例如 L2。
  level: string;
  // Refresh weight marker such as W3.
  // 刷新权重标记，例如 W3。
  refreshWeight: string;
  // Atomic profile content.
  // 原子化画像内容。
  content: string;
  // Source kind key returned by VMM.
  // VMM 返回的来源类型键。
  sourceKind: string;
  // Source row id such as the originating turn id.
  // 来源行标识，例如来源回合标识。
  sourceId: number;
  // Bound user/project/team/space id.
  // 绑定的 user/project/team/space 标识。
  bindId: number;
}

// VmmStaticProfileDateGroupSummary describes one date bucket inside a VMM profile section.
// VmmStaticProfileDateGroupSummary 描述 VMM 画像段落内的单个日期分组。
export interface VmmStaticProfileDateGroupSummary {
  // Human-readable profile date.
  // 人类可读画像日期。
  date: string;
  // Active profile nodes for this date.
  // 当前日期下的 active 画像节点。
  nodes: VmmStaticProfileNodeSummary[];
}

// VmmStaticProfileSectionSummary describes one target-scope static profile section.
// VmmStaticProfileSectionSummary 描述单个目标作用域的静态画像段落。
export interface VmmStaticProfileSectionSummary {
  // Stable target key such as team, space, project, or user.
  // 稳定目标键，例如 team、space、project 或 user。
  target: string;
  // Uppercase target label displayed by the panel.
  // 面板展示的大写目标标签。
  label: string;
  // Date groups under this target.
  // 当前目标下的日期分组。
  dates: VmmStaticProfileDateGroupSummary[];
}

// VmmStaticProfileSummary describes the current active VMM profile snapshot.
// VmmStaticProfileSummary 描述当前 active VMM 画像快照。
export interface VmmStaticProfileSummary {
  // Optional update timestamp captured by the backend.
  // 后端捕获的可选更新时间。
  updatedAt?: string | null;
  // Profile sections grouped by VMM target scope.
  // 按 VMM 目标作用域分组的画像段落。
  sections: VmmStaticProfileSectionSummary[];
}

// ContextInjectionSnapshot describes dynamic and static context injection panels.
// ContextInjectionSnapshot 描述动态与静态上下文注入面板数据。
export interface ContextInjectionSnapshot {
  // Dynamic per-turn injection cards.
  // 按轮次生效的动态注入卡片。
  dynamicCards: ContextInjectionCardSummary[];
  // Structured active VMM profile snapshot for static memory display.
  // 用于静态记忆展示的结构化 active VMM 画像快照。
  staticProfile?: VmmStaticProfileSummary | null;
  // Static persistent injection cards.
  // 持续生效的静态注入卡片。
  staticCards: ContextInjectionCardSummary[];
}

// ContextUsageSnapshot describes the composer context usage orb state.
// ContextUsageSnapshot 描述输入区上下文占用圆盘状态。
export interface ContextUsageSnapshot {
  // Rounded usage percent shown inside the orb.
  // 圆盘中显示的四舍五入占用百分比。
  percent: number;
  // Severity key used by WebUI styling.
  // WebUI 样式使用的严重级别键。
  severity: string;
  // Whether the active runtime still matches the accepted baseline.
  // 当前运行时是否仍匹配已接受的基线。
  baselineMatchesActiveRuntime: boolean;
  // Tooltip title.
  // 提示浮层标题。
  tooltipTitle: string;
  // Tooltip diagnostic rows.
  // 提示浮层诊断行。
  tooltipRows: string[];
}
