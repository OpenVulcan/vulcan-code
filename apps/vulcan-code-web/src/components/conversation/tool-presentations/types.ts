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
  // Optional parser-sanitized request preview for a tool that is still receiving arguments.
  // 可选的解析器净化请求预览，用于仍在接收参数的工具。
  requestPreview?: Record<string, unknown> | null;
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

// FileChangePresentation describes one mutation-only file change-set display.
// FileChangePresentation 描述仅用于文件变更的 change-set 展示。
export interface FileChangePresentation {
  // Discriminator for file change presentation cards.
  // 文件变更展示卡片的判别字段。
  kind: "file_change";
  // Source adapter that produced this display model.
  // 生成该展示模型的来源适配器。
  source: "native_vulcan_file" | "native_file_mutation" | "native_file_edit" | "native_apply_patch" | "host_change_set";
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
  // Hidden detail reference used to hydrate native file results lazily after completion.
  // 用于工具完成后按需加载原生 file 返回内容的隐藏详情引用。
  detailRef?: ConversationDetailRef;
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
  // Producer-owned error code used for a precise localized failure badge.
  // 用于精确本地化失败徽标的产出方错误码。
  errorCode?: string | null;
  // Whether the card should be interactive in its current state.
  // 当前状态下卡片是否允许交互。
  interactive: boolean;
  // Whether an authoritative terminal ChangeSet contains effective changes; null before that boundary.
  // 权威终态 ChangeSet 是否包含有效变更；到达该边界前为 null。
  hasEffectiveChanges: boolean | null;
}

// FileChangeOperation names the normalized operation shown to users.
// FileChangeOperation 命名展示给用户的规范化文件操作。
export type FileChangeOperation = "create" | "modify" | "delete" | "rename" | "mixed";

// FileAccessAction names the read-only operation displayed by the native file tool.
// FileAccessAction 命名原生 file 工具展示的只读操作。
export type FileAccessAction = "read" | "list" | "search" | "stat";

// FileAccessPresentationKind gives each read-only native file action an independent card identity.
// FileAccessPresentationKind 为每个只读原生文件动作提供独立卡片身份。
export type FileAccessPresentationKind = "file_read" | "file_list" | "file_search" | "file_stat";

// FileAccessLineRangePresentation keeps one requested source range structured until localized rendering.
// FileAccessLineRangePresentation 将一段请求源码范围保持为结构化数据，直至本地化渲染。
export interface FileAccessLineRangePresentation {
  // Inclusive 1-based first line.
  // 以 1 为基准的包含式起始行。
  startLine: number;
  // Inclusive 1-based last line, or null when the request continues to the available end.
  // 以 1 为基准的包含式结束行；请求持续到可用末尾时为 null。
  endLine: number | null;
}

// FileAccessItemPresentation describes one target and its read-only returned rows.
// FileAccessItemPresentation 描述一个目标及其只读返回行。
export interface FileAccessItemPresentation {
  // Stable target identity used by expanded rendering.
  // 展开渲染使用的稳定目标标识。
  id: string;
  // Original path supplied to or returned by the native tool.
  // 提供给原生工具或由其返回的原始路径。
  path: string;
  // Workspace-normalized display path.
  // 工作区归一化后的展示路径。
  displayPath: string;
  // Whether the path is project-local or external.
  // 路径属于项目内还是项目外。
  pathKind: "project" | "external" | "unknown";
  // File-type icon model used for read and stat targets.
  // 读取与状态目标使用的文件类型图标模型。
  icon: FileTypeIconModel;
  // Canonical native resource identity when available.
  // 可用时的原生 canonical 资源标识。
  resourceId: string | null;
  // Ordered source ranges requested for this target.
  // 当前目标请求的有序源码范围。
  lineRanges: FileAccessLineRangePresentation[];
  // Read-only returned rows.
  // 只读返回行。
  rows: FileChangeDiffRow[];
  // Review-renderer file used only by source reads.
  // 仅源文件读取使用的审核渲染文件。
  diffFile: DiffFile | null;
  // Whether the target owns meaningful expanded content.
  // 目标是否持有有意义的展开内容。
  expandable: boolean;
  // Decoded text metadata reported by read or stat.
  // 读取或状态操作报告的解码文本元数据。
  textFormat: FileTextFormatPresentation | null;
}

// FileAccessPresentation describes one read, list, search, or stat card without mutation semantics.
// FileAccessPresentation 描述一张不含变更语义的读取、列表、搜索或状态卡片。
export interface FileAccessPresentation {
  // Independent card discriminator selected from the exact native action.
  // 根据精确原生动作选择的独立卡片判别字段。
  kind: FileAccessPresentationKind;
  // Exact read-only action from the native file contract.
  // 原生 file 契约中的精确只读动作。
  action: FileAccessAction;
  // Stable top-level expansion identity.
  // 稳定顶层展开标识。
  id: string;
  // Current authoritative lifecycle state.
  // 当前权威生命周期状态。
  state: ToolPresentationDisplayState;
  // Hidden terminal detail reference used to hydrate complete returned rows.
  // 用于补全完整返回行的隐藏终态详情引用。
  detailRef?: ConversationDetailRef;
  // Ordered request targets aligned with native result indexes.
  // 与原生结果索引对齐的有序请求目标。
  items: FileAccessItemPresentation[];
  // Controlled error detail returned by the native runtime.
  // 原生运行时返回的受控错误详情。
  errorDetail: string | null;
  // Whether terminal returned content may be expanded.
  // 终态返回内容是否允许展开。
  interactive: boolean;
}

// FileTextFormatPresentation describes decoded text metadata reported by the native file runtime.
// FileTextFormatPresentation 描述原生文件运行时报告的已解码文本元数据。
export interface FileTextFormatPresentation {
  // Canonical encoding label, or null when the runtime could not establish text encoding.
  // canonical 编码标签；运行时无法确认文本编码时为 null。
  encoding: string | null;
  // Whether the source contains an encoding byte-order mark.
  // 源文件是否包含编码字节顺序标记。
  bom: boolean | null;
  // Dominant or exact line-ending label, or null when no line ending exists.
  // 主要或精确换行标签；文件不存在换行时为 null。
  lineEnding: string | null;
  // Whether more than one line-ending form occurs in the source.
  // 源文件是否存在多种换行形式。
  mixedLineEndings: boolean | null;
  // Whether the source ends with a line ending.
  // 源文件是否以换行结束。
  finalNewline: boolean | null;
}

// FileChangeFilePresentation describes one file row inside a file change presentation.
// FileChangeFilePresentation 描述文件变更展示中的单个文件行。
export interface FileChangeFilePresentation {
  // Stable id used for per-file expansion in multi-file rows.
  // 多文件行中用于单文件展开的稳定标识。
  id: string;
  // Normalized operation for this file.
  // 当前文件的规范化操作。
  operation: FileChangeOperation;
  // Canonical resource identity used to correlate native multi-file results.
  // 用于关联原生多文件结果的 canonical 资源标识。
  resourceId: string | null;
  // Native operation identifiers mapped to this resource, when available.
  // 映射到该资源的原生操作标识集合。
  operationIds: string[];
  // Previous path for a canonical rename row.
  // canonical rename 行的原始路径。
  previousPath: string | null;
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
  // Whether rows are read-only result lines rather than a mutation diff.
  // 当前行是否为只读结果行，而不是文件变更 diff。
  readOnly: boolean;
  // Decoded text metadata reported for native read and stat results.
  // 原生读取与状态结果报告的已解码文本元数据。
  textFormat: FileTextFormatPresentation | null;
}

// MarkdownToolPresentationIcon names the visual icon slot for Markdown-output tools.
// MarkdownToolPresentationIcon 命名 Markdown 输出工具使用的视觉图标槽位。
export type MarkdownToolPresentationIcon = "folder_tree" | "file_text" | "ast_tree" | "ast_detail" | "search";

// MarkdownToolContentKind selects the exact expanded renderer for one recognized tool.
// MarkdownToolContentKind 为单个已识别工具选择精确的展开渲染器。
export type MarkdownToolContentKind = "markdown" | "source";

// MarkdownToolSummaryTarget carries one exact request target into the collapsed summary.
// MarkdownToolSummaryTarget 将一个精确请求目标带入折叠摘要。
export interface MarkdownToolSummaryTarget {
  // Request path supplied to the tool.
  // 提供给工具的请求路径。
  path: string;
  // Structured source ranges supplied for this path.
  // 为当前路径提供的结构化源码范围。
  lineRanges: FileAccessLineRangePresentation[];
  // Exact numbered-output setting for file reads, or null for non-read tools.
  // 文件读取的精确行号输出设置；非读取工具为 null。
  numbered: boolean | null;
}

// MarkdownToolSourceSection stores one parsed file-read result as source-code rows.
// MarkdownToolSourceSection 将一段已解析的文件读取结果保存为源码行。
export interface MarkdownToolSourceSection {
  // Stable section identity derived from invocation, path, and section index.
  // 根据调用、路径与段序号派生的稳定段标识。
  id: string;
  // Exact file path reported by the file-read result header.
  // 文件读取结果头报告的精确文件路径。
  path: string;
  // Context-only source model used by the code renderer.
  // 代码渲染器使用的纯上下文源码模型。
  diffFile: DiffFile;
}

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
  // Exact expanded renderer selected by the concrete tool contract.
  // 由具体工具契约选择的精确展开渲染器。
  contentKind: MarkdownToolContentKind;
  // Markdown output shown in the expanded card.
  // 展开卡片中展示的 Markdown 输出。
  markdown: string;
  // Exact request targets displayed while the card remains collapsed.
  // 卡片保持折叠时展示的精确请求目标。
  summaryTargets: MarkdownToolSummaryTarget[];
  // Parsed source sections used only by file-read cards.
  // 仅由文件读取卡片使用的已解析源码段。
  sourceSections: MarkdownToolSourceSection[];
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

// CollaborationToolArguments keeps the exact native collaboration argument contract behind the detail view.
// CollaborationToolArguments 将原生协作工具的精确参数契约保留在详情视图之后。
export type CollaborationToolArguments =
  | {
      target_session_id: string;
      question: string;
    }
  | {
      question: string;
    }
  | {
      question_id: string;
      answer: string;
      unable_to_answer: boolean;
    }
  | {
      question_id: string;
      after_revision: number | null;
      timeout_ms: number;
    }
  | {
      question_id: string;
      reason: string;
    }
  | {
      limit: number;
      cursor: string | null;
    }
  | {
      session_id: string;
      after_seq: number | null;
      limit: number;
    }
  | {
      session_id: string;
      message: string;
      delivery_mode: "queue" | "interrupt";
    }
  | {
      target_session_id: string;
      message: string;
    }
  | {
      session_id: string;
      after_revision: number | null;
      timeout_ms: number;
    };

// SubagentOrchestrationPresentation describes one inline subagent or agent-collaboration display.
// SubagentOrchestrationPresentation 描述一条内联子代理或代理协作展示。
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
  operation:
    | "create"
    | "message"
    | "wait"
    | "agent_orchestration_info"
    | "list_agents"
    | "close"
    | "interrupt"
    | "feedback"
    | "collaboration_question"
    | "collaboration_answer"
    | "collaboration_steer"
    | "collaboration_question_wait"
    | "collaboration_notification"
    | "collaboration_session_list"
    | "collaboration_session_read"
    | "collaboration_session_wait"
    | "collaboration_cancel";
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
  // Exact native collaboration arguments retained for the technical detail panel.
  // 保留给技术详情面板的原生协作精确参数。
  collaborationArguments?: CollaborationToolArguments | null;
  // Whether a wait_agent call ended by timeout.
  // wait_agent 调用是否因超时结束。
  timedOut?: boolean | null;
  // Whether wait_agent returned immediately because no active child or queued feedback exists.
  // wait_agent 是否因为不存在活动子代理或排队反馈而立即返回。
  noActiveAgents?: boolean;
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

// NativeSemanticToolId names every native tool that uses the shared semantic card.
// NativeSemanticToolId 命名所有使用共享语义卡片的原生工具。
export type NativeSemanticToolId =
  | "todolist"
  | "get_goal"
  | "create_goal"
  | "update_goal"
  | "knowledge_search"
  | "knowledge_browse"
  | "knowledge_read"
  | "skill"
  | "vmm_memory_search"
  | "vmm_turn_details"
  | "vmm_memory_write"
  | "vmm_memory_delete"
  | "runtime_config";

// NativeSemanticToolFamily selects the business-oriented presentation used by one native tool.
// NativeSemanticToolFamily 为原生工具选择面向业务的展示族。
export type NativeSemanticToolFamily = "todo" | "goal" | "knowledge" | "skill" | "memory" | "runtime_config";

// NativeSemanticToolPresentation describes one recognized native tool without exposing its raw id.
// NativeSemanticToolPresentation 描述一项已识别原生工具，普通卡片不暴露原始标识。
export interface NativeSemanticToolPresentation {
  // Presentation discriminator.
  // 展示判别字段。
  kind: "native_semantic";
  // Stable card identity.
  // 稳定卡片标识。
  id: string;
  // Exact normalized native tool id.
  // 精确规范化的原生工具标识。
  toolId: NativeSemanticToolId;
  // Business presentation family.
  // 业务展示族。
  family: NativeSemanticToolFamily;
  // Human-readable title translation key.
  // 面向用户的标题翻译键。
  titleKey: TranslationKey;
  // Current authoritative tool state.
  // 当前权威工具状态。
  state: ToolPresentationDisplayState;
  // Parsed invocation arguments retained for semantic summaries.
  // 为语义摘要保留的已解析调用参数。
  arguments: unknown;
  // Best currently visible result payload.
  // 当前可见的最佳结果载荷。
  result: unknown;
  // Optional hidden detail reference used after execution starts.
  // 实际执行开始后使用的可选隐藏详情引用。
  detailRef?: ConversationDetailRef;
}

// GenericToolMarkdownPresentation describes the inline fallback card for an unknown tool.
// GenericToolMarkdownPresentation 描述未知工具使用的内联 Markdown 兜底卡片。
export interface GenericToolMarkdownPresentation {
  // Presentation discriminator.
  // 展示判别字段。
  kind: "generic_tool_markdown";
  // Stable card identity.
  // 稳定卡片标识。
  id: string;
  // Raw tool name shown only because this tool is unknown.
  // 仅因工具未知而显示的原始工具名称。
  toolName: string;
  // Optional localized title for one exact recognized inline tool contract.
  // 精确识别的内联工具契约使用的可选本地化标题。
  titleKey?: TranslationKey;
  // Current authoritative tool state.
  // 当前权威工具状态。
  state: ToolPresentationDisplayState;
  // Inline Markdown result or request summary.
  // 内联 Markdown 结果或请求摘要。
  markdown: string;
  // Optional exact request payload for a recognized AI model-extension tool.
  // 已识别 AI 模型扩展工具的可选精确请求载荷。
  inputMarkdown?: string;
  // Optional hidden detail reference used after execution starts.
  // 实际执行开始后使用的可选隐藏详情引用。
  detailRef?: ConversationDetailRef;
}

// ToolPresentation is the union of all recognized custom tool displays.
// ToolPresentation 是所有已识别自定义工具展示的联合类型。
export type ToolPresentation = CommandExecutionPresentation | FileChangePresentation | FileAccessPresentation | MarkdownToolPresentation | LuaExecutionPresentation | AskQuestionPresentation | SubagentOrchestrationPresentation | ProviderToolSearchPresentation | ProviderToolExtractPresentation | ProviderToolTextPresentation | ProviderToolStructuredPresentation | NativeSemanticToolPresentation | GenericToolMarkdownPresentation;
