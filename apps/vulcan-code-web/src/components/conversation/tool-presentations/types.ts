import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type { TerminalVisualState } from "@/components/conversation/conversation-terminal-model";
import type { FileTypeIconModel } from "@/components/conversation/conversation-file-type-icon";
import type { TranslationKey } from "@/i18n";
import type { DiffFile } from "@/diff/types";
import type { CommandSessionSummary } from "@/types";

// ToolPresentationDisplayState normalizes tool execution state for compact presentation cards.
// ToolPresentationDisplayState 为紧凑工具展示卡片规范化工具执行状态。
export type ToolPresentationDisplayState = "running" | "completed" | "failed" | "stopped" | "unknown";

// CommandPluginContribution describes one generic plugin badge and its human-readable detail.
// CommandPluginContribution 描述一个通用插件徽标及其可读详情。
export interface CommandPluginContribution {
  // Optional compact badge shown beside the command.
  // 显示在命令旁的可选紧凑徽标。
  badge?: string | null;
  // Stable contribution label.
  // 稳定的贡献标签。
  label: string;
  // Human-readable contribution value.
  // 人类可读的贡献值。
  value: string;
}

// ToolPresentationResolveOptions carries display-only context used while projecting tool results.
// ToolPresentationResolveOptions 承载投影工具结果时使用的纯展示上下文。
export interface ToolPresentationResolveOptions {
  // Workspace root used to shorten project-local file paths.
  // 用于缩短项目内文件路径的工作区根目录。
  workspacePath?: string | null;
  // Live command-session summaries keyed by the workbench snapshot.
  // 工作台快照提供的实时命令会话摘要。
  commandSessions?: CommandSessionSummary[];
  // Optional prebuilt lookup table for live command-session summaries.
  // 可选的实时命令会话摘要预构建查找表。
  commandSessionById?: Map<string, CommandSessionSummary>;
}

// CommandExecutionPresentation describes one one-shot command-line execution display.
// CommandExecutionPresentation 描述一次性命令行执行展示。
export interface CommandExecutionPresentation {
  // Discriminator for future presentation adapters.
  // 供未来展示适配器使用的判别字段。
  kind: "command_execution";
  // Stable id used for expansion state across ordinary remounts.
  // 用于跨普通重新挂载保持展开状态的稳定标识。
  id: string;
  // Command text shown in both collapsed and expanded states.
  // 折叠与展开状态都会展示的命令文本。
  command: string;
  // Effective command text actually executed after host-side rewrites.
  // 宿主侧改写后实际执行的有效命令文本。
  effectiveCommand?: string | null;
  // Ordered generic contributions returned by every completed plugin hook.
  // 每个已完成插件 Hook 返回的有序通用贡献集合。
  pluginContributions?: CommandPluginContribution[];
  // Optional live command-session id for session-backed terminal rows.
  // 会话型终端行使用的可选实时命令会话标识。
  commandSessionId?: string | null;
  // Session-backed command presentation kind.
  // 会话型命令展示类型。
  commandSessionKind?: "terminal" | null;
  // Fixed terminal-session action parsed from the original tool arguments.
  // 从原始工具参数解析出的固定终端会话动作。
  commandSessionAction?: string | null;
  // Optional input text sent by one terminal-session action.
  // 终端会话动作发送的可选输入文本。
  commandSessionInput?: string | null;
  // Optional owner conversation id for command-session controls.
  // 命令会话控制使用的可选归属会话标识。
  ownerSessionId?: string | null;
  // Effective working directory when known.
  // 已知时的实际工作目录。
  cwd?: string | null;
  // Effective shell label when known.
  // 已知时的实际 shell 标签。
  shellLabel?: string | null;
  // Best known stdout text, usually hydrated from hidden tool detail.
  // 当前可获得的 stdout 文本，通常从隐藏工具详情中补水。
  stdout: string;
  // Best known stderr text, usually hydrated from hidden tool detail.
  // 当前可获得的 stderr 文本，通常从隐藏工具详情中补水。
  stderr: string;
  // Process exit code when the host reported one.
  // 宿主已报告时的进程退出码。
  exitCode: number | null;
  // Current compact display state.
  // 当前紧凑展示状态。
  state: ToolPresentationDisplayState;
  // Normalized terminal visual state when live command-session data exists.
  // 存在实时命令会话数据时的归一化终端视觉状态。
  terminalState?: TerminalVisualState;
  // Optional command-session failure reason.
  // 可选命令会话失败原因。
  failureReason?: string | null;
  // Whether the session id was historical and missing from the live runtime.
  // 会话标识是否来自历史记录且实时运行时已找不到。
  lost?: boolean;
  // Detail reference used to hydrate full command output lazily.
  // 用于懒加载完整命令输出的详情引用。
  detailRef?: ConversationDetailRef;
}

// FileChangePresentation describes one file change-set or native vulcan-file display.
// FileChangePresentation 描述单个文件变更集或原生 vulcan-file 展示。
export interface FileChangePresentation {
  // Discriminator for file change presentation cards.
  // 文件变更展示卡片的判别字段。
  kind: "file_change";
  // Source adapter that produced this display model.
  // 生成该展示模型的来源适配器。
  source: "native_vulcan_file" | "host_change_set";
  // Stable id used for the top-level expansion state.
  // 用于顶层展开状态的稳定标识。
  id: string;
  // Current compact display state.
  // 当前紧凑展示状态。
  state: ToolPresentationDisplayState;
  // Change-set apply mode when reported by the host.
  // 宿主报告的变更集应用模式。
  mode: "preview" | "applied" | "unknown";
  // File operation inferred for the row summary.
  // 行摘要推断出的文件操作。
  operation: FileChangeOperation;
  // Files shown by this presentation.
  // 当前展示包含的文件列表。
  files: FileChangeFilePresentation[];
  // Total inserted line count across files when known.
  // 已知时所有文件累计插入行数。
  totalAdded: number | null;
  // Total removed line count across files when known.
  // 已知时所有文件累计删除行数。
  totalRemoved: number | null;
  // Error details shown for failed native vulcan-file calls.
  // 原生 vulcan-file 调用失败时展示的错误详情。
  errorDetail: string | null;
  // Whether the card should be interactive in its current state.
  // 当前状态下卡片是否允许交互。
  interactive: boolean;
}

// FileChangeOperation names the normalized operation shown to users.
// FileChangeOperation 命名展示给用户的规范化文件操作。
export type FileChangeOperation = "create" | "modify" | "delete" | "mixed";

// FileChangeFilePresentation describes one file row inside a file change presentation.
// FileChangeFilePresentation 描述文件变更展示中的单个文件行。
export interface FileChangeFilePresentation {
  // Stable id used for per-file expansion in multi-file rows.
  // 多文件行中用于单文件展开的稳定标识。
  id: string;
  // Normalized operation for this file.
  // 当前文件的规范化操作。
  operation: FileChangeOperation;
  // Original path reported by the tool or host.
  // 工具或宿主报告的原始路径。
  path: string;
  // Display path after workspace-relative normalization.
  // 工作区相对归一后的展示路径。
  displayPath: string;
  // Whether the display path is project-local or external.
  // 展示路径属于项目内还是项目外。
  pathKind: "project" | "external" | "unknown";
  // Icon model resolved from the file path.
  // 从文件路径解析出的图标模型。
  icon: FileTypeIconModel;
  // Added line count when known.
  // 已知时的新增行数。
  added: number | null;
  // Removed line count when known.
  // 已知时的删除行数。
  removed: number | null;
  // Render rows for expandable content.
  // 可展开内容的渲染行。
  rows: FileChangeDiffRow[];
  // Review-renderer diff file used by the expandable body.
  // 可展开主体复用审核渲染器时使用的 diff 文件。
  diffFile: DiffFile | null;
  // Whether this file has meaningful expandable rows.
  // 当前文件是否有有意义的可展开行。
  expandable: boolean;
}

// MarkdownToolPresentationIcon names the visual icon slot for Markdown-output tools.
// MarkdownToolPresentationIcon 命名 Markdown 输出工具使用的视觉图标槽位。
export type MarkdownToolPresentationIcon = "folder_tree" | "file_text" | "ast_tree" | "ast_detail" | "search";

// MarkdownToolPresentation describes raw Markdown/text output produced by exact tool ids.
// MarkdownToolPresentation 描述由明确工具标识产生的原始 Markdown/文本输出。
export interface MarkdownToolPresentation {
  // Discriminator for Markdown-output presentation cards.
  // Markdown 输出展示卡片的判别字段。
  kind: "markdown_tool_output";
  // Stable id used for expansion state across ordinary remounts.
  // 用于跨普通重新挂载保持展开状态的稳定标识。
  id: string;
  // Exact tool id that selected this presentation adapter.
  // 选择该展示适配器的精确工具标识。
  toolId: string;
  // Current compact display state.
  // 当前紧凑展示状态。
  state: ToolPresentationDisplayState;
  // Hidden detail reference used to hydrate the full Markdown output.
  // 用于补全完整 Markdown 输出的隐藏详情引用。
  detailRef?: ConversationDetailRef;
  // User-visible card title translation key.
  // 用户可见卡片标题的翻译键。
  titleKey: TranslationKey;
  // Icon slot used by the shared card renderer.
  // 共享卡片渲染器使用的图标槽位。
  icon: MarkdownToolPresentationIcon;
  // Markdown output shown in the expanded card.
  // 展开卡片中展示的 Markdown 输出。
  markdown: string;
}

// LuaExecutionPresentation describes one isolated RunLua call or persistent Lua lifecycle action.
// LuaExecutionPresentation 描述一次隔离 RunLua 调用或持久 Lua 生命周期动作。
export interface LuaExecutionPresentation {
  // Discriminator for the dedicated Lua execution card.
  // 专用 Lua 执行卡片的判别字段。
  kind: "lua_execution";
  // Stable id used for expansion state across ordinary remounts.
  // 用于跨普通重新挂载保持展开状态的稳定标识。
  id: string;
  // Exact public tool id that produced the execution.
  // 生成本次执行的精确公开工具标识。
  toolId: "runlua" | "runlua_session";
  // Current compact display state.
  // 当前紧凑展示状态。
  state: ToolPresentationDisplayState;
  // Execution mode selected by the exact public tool contract.
  // 由精确公开工具契约选定的执行模式。
  mode: "one_shot" | "persistent";
  // Exact lifecycle action, or invalid when malformed session arguments omit a valid action.
  // 精确生命周期动作；畸形会话参数缺少有效动作时为 invalid。
  action: "eval" | "list" | "start" | "status" | "close" | "invalid";
  // Short user-facing task or session label.
  // 面向用户的简短任务或会话标签。
  label: string | null;
  // Lua source obtained from the fixed invocation arguments or bounded host preview.
  // 从固定调用参数或有界宿主预览取得的 Lua 源码。
  code: string | null;
  // Exact structured arguments obtained from the fixed invocation contract for table rendering.
  // 从固定调用契约取得的精确结构化参数，用于表格渲染。
  arguments: Record<string, unknown> | null;
  // Opaque persistent session identifier when the action addresses one session.
  // 动作指向单个会话时使用的不透明持久会话标识。
  sessionId: string | null;
  // Idle lease lifetime returned by a persistent session action.
  // 持久会话动作返回的闲置租约生命周期。
  ttlSec: number | null;
  // Complete host-side action latency when reported.
  // 宿主报告的完整动作耗时。
  elapsedMs: number | null;
  // Best available model-facing output, hydrated lazily when necessary.
  // 当前可获得的面向模型输出，必要时按需补全。
  output: string;
  // Hidden detail reference used to hydrate full execution output.
  // 用于补全完整执行输出的隐藏详情引用。
  detailRef?: ConversationDetailRef;
}

// FileChangeDiffRow describes one rendered line inside a file change panel.
// FileChangeDiffRow 描述文件变更面板中的单条渲染行。
export interface FileChangeDiffRow {
  // Row kind controls coloring and layout.
  // 行类型控制颜色与布局。
  kind: "context" | "insert" | "delete" | "omit" | "binary" | "error";
  // Optional line number shown in the gutter.
  // 行号槽中展示的可选行号。
  lineNumber: number | null;
  // Text content shown for the row.
  // 当前行展示的文本内容。
  content: string;
}

// AskQuestionPresentation describes one user-question tool display.
// AskQuestionPresentation 描述一次向用户提问工具展示。
export interface AskQuestionPresentation {
  // Discriminator for ask/question presentation cards.
  // ask/question 展示卡片的判别字段。
  kind: "ask_question";
  // Stable id used for expansion state across ordinary remounts.
  // 用于跨普通重新挂载保持展开状态的稳定标识。
  id: string;
  // Hidden detail reference carrying completed structured answers.
  // 承载完成态结构化答案的隐藏详情引用。
  detailRef?: ConversationDetailRef;
  // Current compact display state.
  // 当前紧凑展示状态。
  state: ToolPresentationDisplayState;
  // Compact title shown beside the status label.
  // 状态文案旁展示的紧凑标题。
  title: string;
  // Text shown when the user rejected, cancelled, or left an answer empty.
  // 用户拒绝、取消或答案为空时展示的文本。
  ignoredAnswerText: string;
  // Ordered questions and answers shown in the expanded panel.
  // 展开面板内按顺序展示的问题与回答。
  questions: AskQuestionPresentationItem[];
}

// AskQuestionPresentationItem describes one question-answer pair.
// AskQuestionPresentationItem 描述单个问题与回答配对。
export interface AskQuestionPresentationItem {
  // Stable per-question id.
  // 单个问题的稳定标识。
  id: string;
  // Short question header from the original ask payload.
  // 原始 ask 载荷中的短问题标题。
  header: string;
  // Full question text shown to the user.
  // 展示给用户的完整问题文本。
  question: string;
  // Ordered answer labels supplied by the user.
  // 用户提交的有序回答标签。
  answers: string[];
  // Whether this question has at least one concrete answer.
  // 当前问题是否至少包含一个具体回答。
  answered: boolean;
}

// SubagentOrchestrationPresentation describes one inline subagent orchestration display.
// SubagentOrchestrationPresentation 描述一条内联子代理调度展示。
export interface SubagentOrchestrationPresentation {
  // Discriminator for subagent orchestration cards.
  // 子代理调度卡片的判别字段。
  kind: "subagent_orchestration";
  // Stable id used for expansion state across ordinary remounts.
  // 用于跨普通重新挂载保持展开状态的稳定标识。
  id: string;
  // Current compact display state.
  // 当前紧凑展示状态。
  state: ToolPresentationDisplayState;
  // Hidden detail reference used to hydrate full status details.
  // 用于补全完整状态详情的隐藏详情引用。
  detailRef?: ConversationDetailRef;
  // Normalized orchestration operation.
  // 规范化调度操作类型。
  operation: "create" | "message" | "wait" | "agent_orchestration_info" | "list_agents" | "close" | "interrupt" | "feedback";
  // Compact title shown in the row.
  // 紧凑行中展示的标题。
  title: string;
  // Optional direct target task name.
  // 可选的直接目标任务名。
  target?: string | null;
  // Original prompt or message text for this one call.
  // 本次调用的原始提示词或消息文本。
  prompt?: string | null;
  // Effective prompt used by a resumed message task.
  // 消息续跑任务实际执行的提示词。
  effectivePrompt?: string | null;
  // Final result text for this one call when available.
  // 本次调用可用时的最终返回文本。
  result?: string | null;
  // Optional queue flag for message tools.
  // 消息工具的可选排队标记。
  queued?: boolean | null;
  // Delivery mode used by send_message.
  // send_message 使用的投递模式。
  deliveryMode?: string | null;
  // Optional note returned by the runtime.
  // 运行时返回的可选说明。
  note?: string | null;
  // Optional runtime debug summary for subagent source diagnostics.
  // 可选的子代理来源诊断运行时调试摘要。
  debugSummary?: string | null;
  // Whether a wait_agent call ended by timeout.
  // wait_agent 调用是否因超时结束。
  timedOut?: boolean | null;
  // Whether a wait_agent call ended because the parent turn was interrupted.
  // wait_agent 调用是否因为父级回合被打断而结束。
  interrupted?: boolean | null;
  // Whether a wait_agent call received a terminal child-agent event.
  // wait_agent 调用是否已收到子智能体终态事件。
  terminalEventReceived?: boolean;
  // Listed child-agent status rows.
  // 已列出的子代理状态行。
  agents: SubagentOrchestrationAgent[];
  // Whether the listed child-agent rows came from a known runtime payload.
  // 已列出的子代理状态行是否来自明确的运行时载荷。
  agentsKnown: boolean;
  // Stopped subagent task ids from close or interrupt calls.
  // 关闭或停止调用返回的已停止任务会话标识。
  stoppedTaskIds: string[];
  // Stopped task count reported by close or interrupt calls.
  // 关闭或停止调用返回的已停止任务数量。
  stoppedCount: number;
  // Closed child-agent public ids from close calls.
  // 关闭调用返回的已关闭子智能体公开标识。
  closedAgents: string[];
  // Closed child-agent count reported by close calls.
  // 关闭调用返回的已关闭子智能体数量。
  closedCount: number;
  // Cleared queued inter-agent message count reported by close calls.
  // 关闭调用返回的已清理代理间排队消息数量。
  clearedMessageCount?: number | null;
}

// SubagentOrchestrationAgent describes one listed child-agent status row.
// SubagentOrchestrationAgent 描述一条已列出的子代理状态行。
export interface SubagentOrchestrationAgent {
  // Stable row id.
  // 稳定行标识。
  id: string;
  // Human-visible agent name.
  // 面向人的代理名称。
  name: string;
  // Runtime status text.
  // 运行时状态文本。
  status: string;
  // Optional terminal status summary.
  // 可选终态状态摘要。
  result?: string | null;
  // Optional usage summary collected from child-agent usage segments.
  // 从子智能体用量分段收集到的可选消耗摘要。
  usageSummary?: SubagentUsageSummary | null;
  // Optional runtime debug summary for this listed child agent.
  // 该已列出子代理的可选运行时调试摘要。
  debugSummary?: string | null;
}

// SubagentUsageSummary contains compact usage accounting for one child-agent result.
// SubagentUsageSummary 包含单个子智能体结果的紧凑消耗统计。
export interface SubagentUsageSummary {
  // Provider call count.
  // Provider 调用次数。
  providerCallCount: number;
  // Tool call count.
  // 工具调用次数。
  toolCallCount: number;
  // Total token count.
  // 总 token 数。
  totalTokens: number;
  // Cached input token count.
  // 缓存命中的输入 token 数。
  cachedInputTokens: number;
}

// ProviderToolSearchPresentationResult describes one unified provider search row.
// ProviderToolSearchPresentationResult 描述一条统一供应商搜索行。
export interface ProviderToolSearchPresentationResult {
  // One-based result rank.
  // 从一开始的结果排名。
  rank: number;
  // Result title.
  // 结果标题。
  title: string;
  // Validated result URL.
  // 已校验的结果地址。
  url: string;
  // Normalized source hostname.
  // 归一化来源主机名。
  sourceDomain: string;
  // Result excerpt.
  // 结果摘要。
  snippet: string;
}

// ProviderToolSearchPresentation describes one normalized provider web-search card.
// ProviderToolSearchPresentation 描述一张归一化供应商网页搜索卡片。
export interface ProviderToolSearchPresentation {
  // Presentation discriminator.
  // 展示判别字段。
  kind: "provider_tool_search";
  // Stable expansion identifier.
  // 稳定展开标识。
  id: string;
  // Current tool state.
  // 当前工具状态。
  state: ToolPresentationDisplayState;
  // Exact submitted query.
  // 精确提交的查询。
  query: string;
  // Ordered normalized results.
  // 有序归一化结果。
  results: ProviderToolSearchPresentationResult[];
}

// ProviderToolTextPresentation describes one normalized provider text card.
// ProviderToolTextPresentation 描述一张归一化供应商文本卡片。
export interface ProviderToolTextPresentation {
  // Presentation discriminator.
  // 展示判别字段。
  kind: "provider_tool_text";
  // Stable expansion identifier.
  // 稳定展开标识。
  id: string;
  // Current tool state.
  // 当前工具状态。
  state: ToolPresentationDisplayState;
  // Provider-generated Markdown or plain text.
  // 供应商生成的 Markdown 或纯文本。
  text: string;
}

// ProviderToolExtractPresentationResult describes one unified extracted page.
// ProviderToolExtractPresentationResult 描述一个统一提取页面。
export interface ProviderToolExtractPresentationResult {
  // Extracted HTTPS page URL.
  // 已提取 HTTPS 页面地址。
  url: string;
  // Provider-returned Markdown or text.
  // 供应商返回的 Markdown 或文本。
  content: string;
  // Extracted image URLs.
  // 已提取图片地址。
  images: string[];
  // Optional page favicon.
  // 可选页面站点图标。
  favicon: string | null;
}

// ProviderToolExtractPresentationFailure describes one provider-reported page failure.
// ProviderToolExtractPresentationFailure 描述一个供应商报告的页面失败。
export interface ProviderToolExtractPresentationFailure {
  // Failed HTTPS page URL.
  // 失败 HTTPS 页面地址。
  url: string;
  // Controlled provider error.
  // 受控供应商错误。
  error: string;
}

// ProviderToolExtractPresentation describes one normalized provider extraction card.
// ProviderToolExtractPresentation 描述一张归一化供应商网页提取卡片。
export interface ProviderToolExtractPresentation {
  // Presentation discriminator.
  // 展示判别字段。
  kind: "provider_tool_extract";
  // Stable expansion identifier.
  // 稳定展开标识。
  id: string;
  // Current tool state.
  // 当前工具状态。
  state: ToolPresentationDisplayState;
  // Successful extraction results.
  // 成功提取结果。
  results: ProviderToolExtractPresentationResult[];
  // Provider-reported per-page failures.
  // 供应商报告的逐页面失败。
  failures: ProviderToolExtractPresentationFailure[];
}

// ProviderToolStructuredPresentation describes one normalized embedding, rerank, or composite result.
// ProviderToolStructuredPresentation 描述一个归一化向量、重排或复合结果。
export interface ProviderToolStructuredPresentation {
  // Presentation discriminator.
  // 展示判别字段。
  kind: "provider_tool_structured";
  // Stable expansion identifier.
  // 稳定展开标识。
  id: string;
  // Current tool state.
  // 当前工具状态。
  state: ToolPresentationDisplayState;
  // Exact closed unified output kind.
  // 精确的封闭统一输出类型。
  outputKind: "embedding" | "rerank" | "composite";
  // Number of top-level semantic items represented by the payload.
  // 载荷表示的顶层语义项数量。
  itemCount: number;
  // Strict normalized payload for expanded inspection.
  // 用于展开检查的严格归一化载荷。
  payload: Record<string, unknown>;
}

// ToolPresentation is the union of all recognized custom tool displays.
// ToolPresentation 是所有已识别自定义工具展示的联合类型。
export type ToolPresentation = CommandExecutionPresentation | FileChangePresentation | MarkdownToolPresentation | LuaExecutionPresentation | AskQuestionPresentation | SubagentOrchestrationPresentation | ProviderToolSearchPresentation | ProviderToolExtractPresentation | ProviderToolTextPresentation | ProviderToolStructuredPresentation;
