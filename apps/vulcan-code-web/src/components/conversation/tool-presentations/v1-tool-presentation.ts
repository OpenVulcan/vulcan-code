import type { ToolCallDisplayItem, ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { TranslationKey } from "@/i18n";
import type {
  ExternalResourceRef,
  PersistentChangeSet,
  PersistentChangeSetFile,
  PersistentLifecycleContent,
  PersistentTextFormat,
  ToolJsonValue,
  ToolPayload,
} from "@/service/conversation-event-window-store";
import { modelExtensionToolTitleKey } from "./model-extension-tool-presentation";
import { resolveV1ToolDomainPresentation, v1ModelResult, type V1ToolDomainPresentation } from "./v1-tool-domain-presentation";

/** Stable visual state copied from the authoritative tool-call event.
 * 从权威工具调用事件复制的稳定视觉状态。 */
export type V1ToolPresentationState = ToolCallEvent["status"];

/** Icon identity understood by the V1 card renderer; null intentionally reserves no slot.
 * V1 卡片渲染器识别的图标身份；null 表示明确不保留图标槽。 */
export type V1ToolPresentationIcon = "file" | "folder-search" | "markdown" | "shell" | "tool" | null;

/** Shared immutable fields carried by every V1 tool presentation.
 * 每个 V1 工具展示模型携带的共享不可变字段。 */
export interface V1ToolPresentationBase {
  id: string;
  seq: number;
  apiCallId: number;
  toolCallId: number;
  toolName: string;
  state: V1ToolPresentationState;
  title: string;
  // Optional localized title used by one exact recognized tool contract.
  // 精确识别的工具契约使用的可选本地化标题。
  titleKey?: TranslationKey;
  icon: V1ToolPresentationIcon;
  externalRefs: readonly ExternalResourceRef[];
  // Readable result derived from the exact event envelope, independent of the card shell.
  // 从精确事件信封派生的可读结果，与卡片外壳独立。
  domain: V1ToolDomainPresentation;
}

/** One line rendered by a persisted ChangeSet file body.
 * 持久化 ChangeSet 文件正文渲染的一行。 */
export interface V1ChangeSetRow {
  kind: "context" | "added" | "deleted" | "omitted";
  lineNumber: number | null;
  content: string;
  logicalLineCount: number;
}

/** One file view derived exclusively from a persisted ChangeSet file record.
 * 完全从持久化 ChangeSet 文件记录派生的单文件视图。 */
export interface V1ChangeSetFilePresentation {
  resourceId: string | null;
  change: PersistentChangeSetFile["change"];
  // Canonical path used for stable identity and list keys.
  // 用于稳定身份与列表键的 canonical 路径。
  path: string;
  // Exact persisted rename source path when the operation is a rename.
  // 操作为重命名时持久化的精确源路径。
  oldPath: string | null;
  // Exact persisted rename destination path when the operation is a rename.
  // 操作为重命名时持久化的精确目标路径。
  newPath: string | null;
  // Stable user-visible path, including both sides of a rename when available.
  // 稳定的用户可见路径；重命名两端可用时同时展示。
  displayPath: string;
  additions: number;
  deletions: number;
  rows: readonly V1ChangeSetRow[];
  beforeFormat: string | null;
  afterFormat: string | null;
}

/** File mutation presentation whose counts and rows share one persisted source.
 * 统计与正文共享同一持久化来源的文件变更展示。 */
export interface V1FileChangePresentation extends V1ToolPresentationBase {
  kind: "file_change";
  mode: PersistentChangeSet["mode"] | null;
  summary: string | null;
  requestTarget: string | null;
  additions: number;
  deletions: number;
  files: readonly V1ChangeSetFilePresentation[];
  diagnostics: readonly { level: string; message: string }[];
  outputMarkdown: string;
}

/** File-access presentation for exact read, list, and stat contracts.
 * 精确 read、list、stat 契约使用的文件访问展示。 */
export interface V1FileAccessPresentation extends V1ToolPresentationBase {
  kind: "file_access";
  action: "read" | "list" | "stat";
  targets: readonly string[];
  markdown: string;
}

/** Markdown-output presentation selected only by an exact tool id.
 * 仅按精确工具标识选择的 Markdown 输出展示。 */
export interface V1MarkdownToolPresentation extends V1ToolPresentationBase {
  kind: "markdown";
  markdown: string;
  targets: readonly string[];
}

/** Shell presentation retaining command, output, and event status together.
 * 将命令、输出与事件状态保存在一起的 shell 展示。 */
export interface V1ShellToolPresentation extends V1ToolPresentationBase {
  kind: "shell";
  command: string;
  outputMarkdown: string;
}

/** Native semantic presentation selected from the frozen native-tool allowlist.
 * 从冻结原生工具允许列表中选择的原生语义展示。 */
export interface V1NativeToolPresentation extends V1ToolPresentationBase {
  kind: "native";
  family: "todo" | "goal" | "knowledge" | "skill" | "memory" | "runtime_config" | "retrieval" | "plan" | "ask" | "documentation" | "debug";
  inputMarkdown: string;
  outputMarkdown: string;
}

/** Generic self-contained fallback for an unknown tool.
 * 未知工具使用的自包含通用兜底展示。 */
export interface V1GenericToolPresentation extends V1ToolPresentationBase {
  kind: "generic";
  inputMarkdown: string;
  outputMarkdown: string;
}

/** Complete V1 tool-card presentation union.
 * 完整的 V1 工具卡展示判别联合。 */
export type V1ToolPresentation =
  | V1FileChangePresentation
  | V1FileAccessPresentation
  | V1MarkdownToolPresentation
  | V1ShellToolPresentation
  | V1NativeToolPresentation
  | V1GenericToolPresentation;

/** Exact Markdown tool contract retained from the reviewed presentation branch.
 * 从已审核展示分支保留的精确 Markdown 工具契约。 */
interface V1MarkdownToolConfig {
  title: string;
  icon: V1ToolPresentationIcon;
  targetKind: "none" | "file-read" | "markdown-menu";
}

/** Exact native semantic card contract retained from the reviewed presentation branch.
 * 从已审核展示分支保留的精确原生语义卡契约。 */
interface V1NativeToolConfig {
  family: V1NativeToolPresentation["family"];
  title: string;
}

// MARKDOWN_TOOL_CONFIGS is keyed only by the concrete public tool id.
// MARKDOWN_TOOL_CONFIGS 仅以具体公开工具标识为键。
const MARKDOWN_TOOL_CONFIGS = new Map<string, V1MarkdownToolConfig>([
  ["vulcan_codekit_ast_tree", { title: "查询代码结构", icon: "markdown", targetKind: "none" }],
  ["vulcan_codekit_ast_detail", { title: "读取代码结构", icon: "markdown", targetKind: "none" }],
  ["vulcan_codekit_rg", { title: "搜索代码", icon: "markdown", targetKind: "none" }],
  ["vulcan_codekit_node_source", { title: "读取节点源码", icon: "markdown", targetKind: "none" }],
  ["vulcan_codekit_markdown_menu", { title: "查询文档目录", icon: null, targetKind: "markdown-menu" }],
]);

// NATIVE_TOOL_CONFIGS is the exact reviewed allowlist and never matches display text.
// NATIVE_TOOL_CONFIGS 是精确审核后的允许列表，绝不匹配展示文本。
const NATIVE_TOOL_CONFIGS = new Map<string, V1NativeToolConfig>([
  ["todolist", { family: "todo", title: "任务列表" }],
  ["get_goal", { family: "goal", title: "读取目标" }],
  ["create_goal", { family: "goal", title: "创建目标" }],
  ["update_goal", { family: "goal", title: "更新目标" }],
  ["knowledge_search", { family: "knowledge", title: "搜索知识库" }],
  ["knowledge_browse", { family: "knowledge", title: "浏览知识库" }],
  ["knowledge_read", { family: "knowledge", title: "读取知识库" }],
  ["skill", { family: "skill", title: "调用技能" }],
  ["vmm_memory_search", { family: "memory", title: "搜索记忆" }],
  ["vmm_turn_details", { family: "memory", title: "读取回合记忆" }],
  ["vmm_memory_write", { family: "memory", title: "写入记忆" }],
  ["vmm_memory_delete", { family: "memory", title: "删除记忆" }],
  ["runtime_config", { family: "runtime_config", title: "运行时配置" }],
  ["codebase_retrieval", { family: "retrieval", title: "检索代码库" }],
  ["submit_plan", { family: "plan", title: "提交计划" }],
  ["ask", { family: "ask", title: "向用户提问" }],
  ["luaskill", { family: "skill", title: "发现技能" }],
  ["call_luaskill", { family: "skill", title: "执行技能" }],
  ["context7_resolve_library_id", { family: "documentation", title: "查找文档库" }],
  ["context7_query_docs", { family: "documentation", title: "查询文档" }],
  ["debug_session_open", { family: "debug", title: "打开调试会话" }],
  ["debug_session_request", { family: "debug", title: "发送调试请求" }],
  ["debug_session_events", { family: "debug", title: "读取调试事件" }],
  ["debug_session_status", { family: "debug", title: "查询调试状态" }],
  ["debug_session_close", { family: "debug", title: "关闭调试会话" }],
]);

// SHELL_TOOL_IDS is the complete exact shell-card allowlist for this stage.
// SHELL_TOOL_IDS 是本阶段完整且精确的 shell 卡片允许列表。
const SHELL_TOOL_IDS = new Set(["shell_command", "shell_session", "terminal_session"]);

/** Resolves one self-contained V1 tool card without record lookup or filesystem access.
 * 在不查询记录且不访问文件系统的情况下解析一张自包含 V1 工具卡。 */
export function resolveV1ToolPresentation(item: ToolCallDisplayItem): V1ToolPresentation {
  // toolCall is the sole authoritative presentation input.
  // toolCall 是唯一权威展示输入。
  const toolCall = item.tool_call;
  // base preserves physical API grouping and event order without secondary ids.
  // base 保留物理 API 分组与事件顺序，不引入二次标识。
  const base = v1ToolPresentationBase(item);

  if (toolCall.changeset) {
    return resolveV1FileChangePresentation(base, toolCall.changeset, toolCall.output);
  }
  if (toolCall.tool_name === "file") {
    // action is the exact discriminator declared by the native file tool schema.
    // action 是原生 file 工具 Schema 声明的精确判别字段。
    const action = payloadString(toolCall.input, "action");
    if (action === "read" || action === "list" || action === "stat") {
      return resolveV1FileAccessPresentation(base, action, toolCall.input, toolCall.output);
    }
    return resolveV1PendingFileChangePresentation(base, toolCall.input, toolCall.output);
  }
  if (toolCall.tool_name === "file_edit" || toolCall.tool_name === "apply_patch") {
    return resolveV1PendingFileChangePresentation(base, toolCall.input, toolCall.output);
  }
  if (toolCall.tool_name === "vulcan_file_list") {
    return resolveV1FileAccessPresentation(base, "list", toolCall.input, toolCall.output);
  }
  if (toolCall.tool_name === "vulcan_file_read") {
    return resolveV1FileAccessPresentation(base, "read", toolCall.input, toolCall.output);
  }
  // markdownConfig exists only for one exact configured public tool id.
  // markdownConfig 只会为一个精确配置的公开工具标识存在。
  const markdownConfig = MARKDOWN_TOOL_CONFIGS.get(toolCall.tool_name);
  if (markdownConfig) {
    return {
      ...base,
      kind: "markdown",
      title: markdownConfig.title,
      icon: markdownConfig.icon,
      markdown: payloadMarkdown(toolCall.output, "markdown"),
      targets: markdownTargets(toolCall.input, markdownConfig.targetKind),
    };
  }
  if (SHELL_TOOL_IDS.has(toolCall.tool_name)) {
    return {
      ...base,
      kind: "shell",
      title: "执行命令",
      icon: "shell",
      command: payloadString(toolCall.input, "command") ?? payloadString(toolCall.input, "chars") ?? "",
      outputMarkdown: payloadMarkdown(toolCall.output, "text"),
    };
  }
  // nativeConfig exists only for one exact reviewed native semantic id.
  // nativeConfig 只会为一个精确审核的原生语义标识存在。
  const nativeConfig = NATIVE_TOOL_CONFIGS.get(toolCall.tool_name);
  if (nativeConfig) {
    return {
      ...base,
      kind: "native",
      title: nativeConfig.title,
      icon: "tool",
      family: nativeConfig.family,
      inputMarkdown: payloadMarkdown(toolCall.input, null),
      outputMarkdown: payloadMarkdown(toolCall.output, "markdown"),
    };
  }
  // modelExtensionTitleKey exists only for the closed AI model-extension tool set.
  // modelExtensionTitleKey 仅对封闭的 AI 模型扩展工具集合存在。
  const modelExtensionTitleKey = modelExtensionToolTitleKey(toolCall.tool_name);
  if (modelExtensionTitleKey) {
    return {
      ...base,
      kind: "generic",
      title: toolCall.tool_name,
      titleKey: modelExtensionTitleKey,
      icon: "tool",
      inputMarkdown: payloadMarkdown(toolCall.input, null),
      outputMarkdown: payloadMarkdown(toolCall.output, null),
    };
  }
  return {
    ...base,
    kind: "generic",
    title: toolCall.tool_name,
    icon: "tool",
    inputMarkdown: payloadMarkdown(toolCall.input, null),
    outputMarkdown: payloadMarkdown(toolCall.output, "markdown"),
  };
}

/** Builds shared presentation identity directly from the V1 event coordinates.
 * 直接从 V1 事件坐标构建共享展示身份。 */
function v1ToolPresentationBase(item: ToolCallDisplayItem): V1ToolPresentationBase {
  return {
    id: `v1-tool:${item.seq}`,
    seq: item.seq,
    apiCallId: item.tool_call.api_call_id,
    toolCallId: item.tool_call.tool_call_id,
    toolName: item.tool_call.tool_name,
    state: item.tool_call.status,
    title: item.tool_call.tool_name,
    icon: "tool",
    externalRefs: item.tool_call.external_refs ?? [],
    domain: resolveV1ToolDomainPresentation(item.tool_call),
  };
}

/** Builds an applied or preview file-change card from the canonical ChangeSet.
 * 从 canonical ChangeSet 构建已应用或预览文件变更卡。 */
function resolveV1FileChangePresentation(
  base: V1ToolPresentationBase,
  changeset: PersistentChangeSet,
  output: ToolPayload | undefined,
): V1FileChangePresentation {
  // files are mapped once so header totals and rendered rows share one model.
  // files 只映射一次，使头部统计与渲染行共享同一模型。
  const files = changeset.files.map(buildV1ChangeSetFilePresentation);
  return {
    ...base,
    kind: "file_change",
    title: files.length > 0 && files.every(file => file.change === files[0].change)
      ? ({ create: "创建", modify: "编辑", delete: "删除", rename: "改名" } as const)[files[0].change] : "变更",
    icon: "file",
    mode: changeset.mode,
    summary: changeset.summary ?? null,
    requestTarget: null,
    additions: files.reduce((total, file) => total + file.additions, 0),
    deletions: files.reduce((total, file) => total + file.deletions, 0),
    files,
    diagnostics: changeset.diagnostics ?? [],
    outputMarkdown: payloadMarkdown(output, "markdown"),
  };
}

/** Builds a running or failed mutation card before a ChangeSet exists.
 * 在 ChangeSet 尚不存在时构建运行中或失败的变更卡。 */
function resolveV1PendingFileChangePresentation(
  base: V1ToolPresentationBase,
  input: ToolPayload,
  output: ToolPayload | undefined,
): V1FileChangePresentation {
  return {
    ...base,
    kind: "file_change",
    title: base.state === "running" ? "正在编辑" : "文件编辑",
    icon: "file",
    mode: null,
    summary: null,
    requestTarget: payloadString(input, "path"),
    additions: 0,
    deletions: 0,
    files: [],
    diagnostics: [],
    outputMarkdown: payloadMarkdown(output, "markdown"),
  };
}

/** Builds one exact file-access card from the native file contract.
 * 从原生文件工具契约构建一张精确文件访问卡。 */
function resolveV1FileAccessPresentation(
  base: V1ToolPresentationBase,
  action: V1FileAccessPresentation["action"],
  input: ToolPayload,
  output: ToolPayload | undefined,
): V1FileAccessPresentation {
  // title is fixed by the explicit action rather than inferred from result text.
  // title 由显式 action 固定，而不是从结果文本推断。
  const title = action === "list" ? "查询文件列表" : action === "read" ? "读取" : "查询文件信息";
  return {
    ...base,
    kind: "file_access",
    title,
    // Directory queries carry their folder-search affordance like the branch renderer.
    // 目录查询与分支渲染器一致使用文件夹搜索图标。
    icon: action === "list" ? "folder-search" : "file",
    action,
    targets: fileAccessTargets(base.toolName, input),
    markdown: payloadMarkdown(output, action === "list" ? "markdown" : "text"),
  };
}

/** Derives one file presentation from exact persisted lifecycle and hunk data.
 * 从精确持久化生命周期与 hunk 数据派生单文件展示。 */
export function buildV1ChangeSetFilePresentation(
  file: PersistentChangeSetFile,
): V1ChangeSetFilePresentation {
  // rows preserve producer order and carry all changed-line accounting.
  // rows 保留产出方顺序并携带全部变更行统计。
  const rows = changeSetFileRows(file);
  // path remains the canonical identity while displayPath carries rename semantics.
  // path 保持 canonical 身份，displayPath 则携带重命名语义。
  const path = changeSetFilePath(file);
  // oldPath and newPath are exact persisted facts and never inferred from the filesystem.
  // oldPath 与 newPath 是精确持久化事实，绝不从文件系统推断。
  const oldPath = file.old_path ?? null;
  const newPath = file.new_path ?? null;
  return {
    resourceId: file.resource_id ?? null,
    change: file.change,
    path,
    oldPath,
    newPath,
    displayPath: file.change === "rename" && oldPath && newPath
      ? `${oldPath} → ${newPath}`
      : path,
    additions: rows.reduce((total, row) => total + (row.kind === "added" || (row.kind === "omitted" && file.change === "create") ? row.logicalLineCount : 0), 0),
    deletions: rows.reduce((total, row) => total + (row.kind === "deleted" || (row.kind === "omitted" && file.change === "delete") ? row.logicalLineCount : 0), 0),
    rows,
    beforeFormat: persistentTextFormatLabel(file.before_text_format),
    afterFormat: persistentTextFormatLabel(file.after_text_format),
  };
}

/** Selects the authoritative row source for each persisted file-change kind.
 * 为每种持久化文件变更类型选择权威行来源。 */
function changeSetFileRows(file: PersistentChangeSetFile): V1ChangeSetRow[] {
  if (file.change === "modify") {
    return hunkRows(file);
  }
  if (file.change === "rename") {
    // Rename hunks own content-change accounting; lifecycle text is only the pure-rename body fallback.
    // rename hunks 拥有内容变更统计；生命周期正文只作为纯重命名的正文来源。
    return file.hunks.length > 0
      ? hunkRows(file)
      : file.lifecycle_content
        ? lifecycleRows(file.change, file.lifecycle_content)
        : [];
  }
  return file.lifecycle_content
    ? lifecycleRows(file.change, file.lifecycle_content)
    : hunkRows(file);
}

/** Converts exact modify hunks to display rows without sorting or merging them.
 * 在不排序或合并的情况下将精确 modify hunks 转为展示行。 */
function hunkRows(file: PersistentChangeSetFile): V1ChangeSetRow[] {
  return file.hunks.flatMap((hunk) => [
    ...contextRows(hunk.before),
    ...hunk.deleted.map((line) => ({
      kind: "deleted" as const,
      lineNumber: line.line,
      content: line.content,
      logicalLineCount: 1,
    })),
    ...hunk.inserted.map((line) => ({
      kind: "added" as const,
      lineNumber: line.line,
      content: line.content,
      logicalLineCount: 1,
    })),
    ...contextRows(hunk.after),
  ]);
}

/** Converts stable hunk context text to non-counted display rows.
 * 将稳定 hunk 上下文文本转换为不计入变更统计的展示行。 */
function contextRows(text: string): V1ChangeSetRow[] {
  return logicalLines(text).map((content) => ({
    kind: "context",
    lineNumber: null,
    content,
    logicalLineCount: 0,
  }));
}

/** Builds lifecycle rows directly from full, truncated, or omitted producer semantics.
 * 直接按照产出方的 full、truncated 或 omitted 语义构建生命周期行。 */
function lifecycleRows(
  change: PersistentChangeSetFile["change"],
  lifecycle: PersistentLifecycleContent,
): V1ChangeSetRow[] {
  // rowKind follows the only lifecycle operations that own +/- totals.
  // rowKind 遵循唯一拥有加减统计的生命周期操作。
  const rowKind = change === "delete" ? "deleted" : change === "create" ? "added" : "context";
  if (lifecycle.mode === "omitted") {
    return [{
      kind: "omitted",
      lineNumber: null,
      content: "正文由产出方明确省略",
      logicalLineCount: lifecycle.total_line_count ?? 0,
    }];
  }
  if (lifecycle.mode === "full") {
    // retainedRows are the exact inline body supplied by the producer.
    // retainedRows 是产出方提供的精确内联正文。
    return numberedRows(logicalLines(lifecycle.content), rowKind, 1);
  }
  // headLines and tailLines remain separate so tail line numbers use the complete total.
  // headLines 与 tailLines 保持分离，使尾部行号使用完整总行数。
  const headLines = logicalLines(lifecycle.head);
  const tailLines = logicalLines(lifecycle.tail);
  const retainedCount = headLines.length + tailLines.length;
  const completeCount = lifecycle.total_line_count;
  const omittedCount = completeCount - retainedCount;
  return [
    ...numberedRows(headLines, rowKind, 1),
    ...(omittedCount > 0 ? [{
      kind: "omitted" as const,
      lineNumber: null,
      content: `省略 ${omittedCount} 行`,
      logicalLineCount: omittedCount,
    }] : []),
    ...numberedRows(tailLines, rowKind, completeCount - tailLines.length + 1),
  ];
}

/** Assigns producer-order line numbers to one retained lifecycle text fragment.
 * 为一段保留的生命周期正文按产出方顺序分配行号。 */
function numberedRows(
  lines: readonly string[],
  kind: V1ChangeSetRow["kind"],
  startLine: number,
): V1ChangeSetRow[] {
  return lines.map((content, index) => ({
    kind,
    lineNumber: startLine + index,
    content,
    logicalLineCount: kind === "context" ? 0 : 1,
  }));
}

/** Splits persisted text into logical lines without inventing a trailing empty line.
 * 将持久化文本拆成逻辑行，且不虚构末尾空行。 */
function logicalLines(text: string): string[] {
  if (!text) {
    return [];
  }
  // lines preserves empty interior lines while removing only a final terminator artifact.
  // lines 保留内部空行，仅移除末尾换行产生的分割伪项。
  const lines = text.split(/\r\n|\n|\r/);
  if (lines.at(-1) === "") {
    lines.pop();
  }
  return lines;
}

/** Selects the declared path for each change kind without filesystem probing.
 * 在不探测文件系统的情况下为每种变更类型选择声明路径。 */
function changeSetFilePath(file: PersistentChangeSetFile): string {
  if (file.change === "rename") {
    return file.new_path ?? file.old_path ?? file.path ?? file.resource_id ?? "unknown";
  }
  return file.path ?? file.new_path ?? file.old_path ?? file.resource_id ?? "unknown";
}

/** Formats one persisted text-format snapshot into stable inline metadata.
 * 将一份持久化文本格式快照格式化为稳定行内元数据。 */
export function persistentTextFormatLabel(format: PersistentTextFormat | undefined): string | null {
  if (!format) {
    return null;
  }
  // parts contains only values explicitly persisted by the producer.
  // parts 仅包含产出方明确持久化的值。
  const parts = [format.encoding, format.line_ending].filter((value): value is string => Boolean(value));
  if (format.bom === true) {
    parts.push("BOM");
  }
  if (format.mixed_line_endings === true) {
    parts.push("mixed");
  }
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** Extracts exact file-access targets from declared tool contracts.
 * 从已声明工具契约中提取精确文件访问目标。 */
function fileAccessTargets(toolName: string, input: ToolPayload): string[] {
  if (toolName === "vulcan_file_read") {
    // singleFile and files are the two explicit variants of the file-read schema.
    // singleFile 与 files 是 file-read Schema 的两个显式变体。
    const singleFile = payloadString(input, "file");
    if (singleFile) {
      return [singleFile];
    }
    return payloadArray(input, "files")
      .map(toolObject)
      .map((file) => file ? payloadString(file, "file") : null)
      .filter((path): path is string => path !== null);
  }
  // path is the exact native file/list target.
  // path 是原生 file/list 的精确目标。
  const path = payloadString(input, "path");
  return path ? [path] : [];
}

/** Extracts Markdown summary targets from one configured exact contract.
 * 从一个已配置的精确契约中提取 Markdown 摘要目标。 */
function markdownTargets(input: ToolPayload, kind: V1MarkdownToolConfig["targetKind"]): string[] {
  if (kind === "none") {
    return [];
  }
  if (kind === "file-read") {
    return fileAccessTargets("vulcan_file_read", input);
  }
  // path is LF-separated by the exact Markdown-menu contract.
  // path 按精确 Markdown-menu 契约使用 LF 分隔。
  const path = payloadString(input, "path");
  return path ? path.split(/\r?\n/).map((value) => value.trim()).filter(Boolean) : [];
}

/** Converts an event payload to Markdown without reading any external state.
 * 在不读取任何外部状态的情况下将事件载荷转换为 Markdown。 */
function payloadMarkdown(
  payload: ToolPayload | undefined,
  exactTextField: "markdown" | "text" | null,
): string {
  if (!payload) {
    return "";
  }
  // Runtime results use the canonical envelope; direct event payloads retain their declared text field.
  // 运行时结果使用规范信封；直接事件载荷保留其声明的文本字段。
  if (Object.hasOwn(payload, "model_output")) {
    const result = v1ModelResult(payload);
    return typeof result === "string" ? result : result === undefined ? "" : `\`\`\`json\n${JSON.stringify(result, null, 2)}\n\`\`\``;
  }
  if (exactTextField) {
    // text is accepted only from the field declared by the selected tool-card contract.
    // text 只接受所选工具卡契约声明的字段。
    const text = payloadString(payload, exactTextField);
    if (text !== null) {
      return text;
    }
  }
  return `\`\`\`json\n${JSON.stringify(payload, null, 2)}\n\`\`\``;
}

/** Reads one exact string field from a typed tool payload.
 * 从强类型工具载荷中读取一个精确字符串字段。 */
function payloadString(payload: ToolPayload, key: string): string | null {
  // value is read from the one explicitly named contract field.
  // value 从唯一明确命名的契约字段读取。
  const value = payload[key];
  return typeof value === "string" ? value : null;
}

/** Reads one exact array field from a typed tool payload.
 * 从强类型工具载荷中读取一个精确数组字段。 */
function payloadArray(payload: ToolPayload, key: string): ToolJsonValue[] {
  // value is accepted only when the manifest-validated field is an array.
  // value 仅在 Manifest 已校验字段为数组时接受。
  const value = payload[key];
  return Array.isArray(value) ? value : [];
}

/** Narrows one JSON value to its object form.
 * 将一个 JSON 值收窄为对象形态。 */
function toolObject(value: ToolJsonValue): ToolPayload | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value
    : null;
}
