import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import type { FileTypeIconModel } from "@/components/conversation/conversation-file-type-icon";
import { resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { DiffFile, DiffLine } from "@/diff/types";

// ToolPresentationDisplayState normalizes tool execution state for compact presentation cards.
// ToolPresentationDisplayState 为紧凑工具展示卡片规范化工具执行状态。
export type ToolPresentationDisplayState = "running" | "completed" | "failed" | "stopped" | "unknown";

// ToolPresentationResolveOptions carries display-only context used while projecting tool results.
// ToolPresentationResolveOptions 承载投影工具结果时使用的纯展示上下文。
export interface ToolPresentationResolveOptions {
  // Workspace root used to shorten project-local file paths.
  // 用于缩短项目内文件路径的工作区根目录。
  workspacePath?: string | null;
}

// CommandExecutionPresentation describes one one-shot command-line execution display.
// CommandExecutionPresentation 描述一次性命令行执行展示。
export interface CommandExecutionPresentation {
  // Discriminator for future presentation adapters.
  // 供未来展示适配器使用的判别字段。
  kind: "command_execution";
  // Stable id used for expansion state across virtualized remounts.
  // 用于跨虚拟滚动重挂保持展开状态的稳定标识。
  id: string;
  // Command text shown in both collapsed and expanded states.
  // 折叠与展开状态都会展示的命令文本。
  command: string;
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

// ToolPresentation is the union of all recognized custom tool displays.
// ToolPresentation 是所有已识别自定义工具展示的联合类型。
export type ToolPresentation = CommandExecutionPresentation | FileChangePresentation;

// resolveToolPresentation builds a specialized presentation model or returns null for default rendering.
// resolveToolPresentation 构建专用展示模型；无法识别时返回 null 以回退默认渲染。
export function resolveToolPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
  options: ToolPresentationResolveOptions = {},
): ToolPresentation | null {
  return resolveFileChangePresentation(blocks, displayState, options)
    ?? resolveCommandExecutionPresentation(blocks, displayState, detailRef);
}

// hydrateCommandExecutionPresentation merges one hidden tool detail payload into an existing command presentation.
// hydrateCommandExecutionPresentation 将单条隐藏工具详情载荷合并进已有命令展示模型。
export function hydrateCommandExecutionPresentation(
  presentation: CommandExecutionPresentation,
  payload: ToolDetailPayload | null,
): CommandExecutionPresentation {
  if (!payload) {
    return presentation;
  }
  const display = shellCommandDisplayFromMetadata(payload.metadata) ?? parseShellCommandDisplayFromText(payload.body);
  if (!display) {
    return presentation;
  }
  return mergeCommandExecutionDisplay(presentation, display);
}

// commandPresentationStatusLabel returns the short Chinese state label shown in collapsed rows.
// commandPresentationStatusLabel 返回折叠行展示的短中文状态文案。
export function commandPresentationStatusLabel(state: ToolPresentationDisplayState): string {
  if (state === "running") return "执行中";
  if (state === "failed") return "执行失败";
  if (state === "stopped") return "已停止";
  return "已执行";
}

// fileChangeStatusLabel returns the localized action label for one file change row.
// fileChangeStatusLabel 返回单个文件变更行的本地化动作文案。
export function fileChangeStatusLabel(presentation: FileChangePresentation, operation: FileChangeOperation): string {
  if (presentation.state === "running") {
    return operationLabel(operation, "正在写入", "正在编辑", "正在删除", "正在变更");
  }
  if (presentation.state === "failed") {
    return operationLabel(operation, "写入失败", "编辑失败", "删除失败", "变更失败");
  }
  if (presentation.mode === "preview") {
    return operationLabel(operation, "将写入", "将编辑", "将删除", "将变更");
  }
  return operationLabel(operation, "已写入", "已编辑", "已删除", "已变更");
}

// resolveFileChangePresentation recognizes native vulcan-file calls and completed host change sets.
// resolveFileChangePresentation 识别原生 vulcan-file 调用和已完成的宿主变更集。
function resolveFileChangePresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  options: ToolPresentationResolveOptions,
): FileChangePresentation | null {
  const hostPayload = displayState !== "running" ? firstChangeSetPayload(blocks) : null;
  if (hostPayload) {
    return buildHostChangeSetPresentation(blocks, displayState, hostPayload, options);
  }
  const nativeOperation = nativeVulcanFileOperation(blocks);
  if (!nativeOperation) {
    return null;
  }
  return buildNativeVulcanFilePresentation(blocks, displayState, nativeOperation, options);
}

// buildHostChangeSetPresentation converts one host change_set payload into display rows.
// buildHostChangeSetPresentation 将宿主 change_set 载荷转换为展示行。
function buildHostChangeSetPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  payload: Record<string, unknown>,
  options: ToolPresentationResolveOptions,
): FileChangePresentation | null {
  const rawFiles = arrayValue(payload.files);
  if (!rawFiles.length) {
    return null;
  }
  const files = rawFiles
    .map((file, index) => buildHostChangeSetFile(file, index, blocks, options))
    .filter((file): file is FileChangeFilePresentation => Boolean(file));
  if (!files.length) {
    return null;
  }
  const mode = changeSetMode(payload.mode);
  const operation = summarizeOperation(files);
  return {
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(blocks, files, "change-set"),
    interactive: displayState !== "running" && files.some((file) => file.expandable),
    kind: "file_change",
    mode,
    operation,
    source: "host_change_set",
    state: displayState,
    totalAdded: sumKnown(files.map((file) => file.added)),
    totalRemoved: sumKnown(files.map((file) => file.removed)),
  };
}

// buildNativeVulcanFilePresentation builds a display model before a host change_set exists.
// buildNativeVulcanFilePresentation 在宿主 change_set 尚不存在时构建展示模型。
function buildNativeVulcanFilePresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  operation: Exclude<FileChangeOperation, "mixed">,
  options: ToolPresentationResolveOptions,
): FileChangePresentation | null {
  const paths = nativeVulcanFilePaths(blocks);
  const displayPaths = paths.length ? paths : ["文件"];
  const files = displayPaths.map((path, index) => {
    const rows = displayState === "failed" ? errorRowsFromBlocks(blocks) : [];
    return buildFilePresentation({
      added: null,
      blocks,
      expandable: rows.length > 0,
      index,
      operation,
      path,
      removed: null,
      rows,
      options,
    });
  });
  return {
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(blocks, files, `native-${operation}`),
    interactive: displayState === "failed",
    kind: "file_change",
    mode: "unknown",
    operation,
    source: "native_vulcan_file",
    state: displayState,
    totalAdded: sumKnown(files.map((file) => file.added)),
    totalRemoved: sumKnown(files.map((file) => file.removed)),
  };
}

// buildHostChangeSetFile converts one change_set file entry into one display file.
// buildHostChangeSetFile 将单个 change_set 文件条目转换为展示文件。
function buildHostChangeSetFile(
  value: unknown,
  index: number,
  blocks: ToolExecutionConversationBlock[],
  options: ToolPresentationResolveOptions,
): FileChangeFilePresentation | null {
  const file = objectValue(value);
  if (!file) {
    return null;
  }
  const operation = changeOperation(file.change);
  const path = stringValue(file.path) ?? `file-${index + 1}`;
  if (operation === "create") {
    const rows = insertRows(splitTextLines(textValue(file.content) ?? ""));
    const contentLines = numberValue(file.content_lines ?? file.contentLines);
    return buildFilePresentation({
      added: rows.length || contentLines,
      blocks,
      expandable: rows.length > 0,
      index,
      operation,
      path,
      removed: 0,
      rows,
      options,
    });
  }
  if (operation === "modify") {
    const rows = modifyRows(arrayValue(file.hunks));
    const added = rows.filter((row) => row.kind === "insert").length || numberValue(file.inserted_line_count ?? file.insertedLineCount);
    const removed = rows.filter((row) => row.kind === "delete").length || numberValue(file.deleted_line_count ?? file.deletedLineCount);
    return buildFilePresentation({
      added,
      blocks,
      expandable: rows.length > 0,
      index,
      operation,
      path,
      removed,
      rows,
      options,
    });
  }
  if (operation === "delete") {
    const rows = deleteRows(file);
    const removed = numberValue(file.total_line_count ?? file.totalLineCount)
      ?? numberValue(file.content_lines ?? file.contentLines)
      ?? rows.filter((row) => row.kind === "delete").length;
    return buildFilePresentation({
      added: 0,
      blocks,
      expandable: rows.length > 0,
      index,
      operation,
      path,
      removed,
      rows,
      options,
    });
  }
  return buildFilePresentation({
    added: null,
    blocks,
    expandable: false,
    index,
    operation,
    path,
    removed: null,
    rows: [],
    options,
  });
}

// buildFilePresentation applies shared path, icon, and id normalization to one file.
// buildFilePresentation 对单个文件应用共享路径、图标与标识归一逻辑。
function buildFilePresentation(args: {
  // Added line count for this file.
  // 当前文件的新增行数。
  added: number | null;
  // Blocks used to derive stable ids.
  // 用于派生稳定标识的块列表。
  blocks: ToolExecutionConversationBlock[];
  // Whether the file can be expanded.
  // 文件是否可展开。
  expandable: boolean;
  // File index inside the change-set payload.
  // 文件在变更集载荷中的序号。
  index: number;
  // Normalized operation for this file.
  // 当前文件的规范化操作。
  operation: FileChangeOperation;
  // Original path.
  // 原始路径。
  path: string;
  // Removed line count for this file.
  // 当前文件的删除行数。
  removed: number | null;
  // Render rows for this file.
  // 当前文件的渲染行。
  rows: FileChangeDiffRow[];
  // Resolver options.
  // 解析选项。
  options: ToolPresentationResolveOptions;
}): FileChangeFilePresentation {
  const displayPath = displayPathForFile(args.path, args.options.workspacePath);
  const diffFile = fileChangeDiffFile({
    added: args.added,
    index: args.index,
    operation: args.operation,
    path: displayPath.path,
    removed: args.removed,
    rows: args.rows,
  });
  return {
    added: args.added,
    diffFile,
    displayPath: displayPath.path,
    expandable: args.expandable && Boolean(diffFile),
    icon: resolveFileTypeIcon(args.path),
    id: `${args.index}:${compactHash(args.path)}:${args.operation}`,
    operation: args.operation,
    path: args.path,
    pathKind: displayPath.kind,
    removed: args.removed,
    rows: args.rows,
  };
}

// fileChangeDiffFile adapts change-set rows into the shared review diff renderer model.
// fileChangeDiffFile 将 change-set 行适配为共享代码审核 diff 渲染模型。
function fileChangeDiffFile(args: {
  // Added line count for this file.
  // 当前文件的新增行数。
  added: number | null;
  // File index inside the change-set payload.
  // 文件在变更集载荷中的序号。
  index: number;
  // Normalized operation for this file.
  // 当前文件的规范化操作。
  operation: FileChangeOperation;
  // Display path.
  // 展示路径。
  path: string;
  // Removed line count for this file.
  // 当前文件的删除行数。
  removed: number | null;
  // Render rows for this file.
  // 当前文件的渲染行。
  rows: FileChangeDiffRow[];
}): DiffFile | null {
  const lines = fileChangeDiffLines(args.rows);
  if (!lines.length) {
    return null;
  }
  const additions = args.added ?? lines.filter((line) => line.kind === "added").length;
  const deletions = args.removed ?? lines.filter((line) => line.kind === "removed").length;
  const oldStart = firstLineNumber(lines, "oldLine") ?? 1;
  const newStart = firstLineNumber(lines, "newLine") ?? 1;
  return {
    additions,
    changedBytes: changedLineBytes(lines),
    changedLines: additions + deletions,
    deletions,
    hunks: [{
      header: `@@ -${oldStart},${Math.max(1, deletions)} +${newStart},${Math.max(1, additions)} @@`,
      lines,
      newStart,
      oldStart,
    }],
    isBinary: args.rows.some((row) => row.kind === "binary"),
    isGitlink: false,
    isLarge: false,
    key: `change-set:${args.index}:${compactHash(args.path)}`,
    largeReasons: [],
    maxChangedLineBytes: maxChangedLineBytes(lines),
    newPath: args.operation === "delete" ? "/dev/null" : args.path,
    oldPath: args.operation === "create" ? "/dev/null" : args.path,
    rowCount: lines.length + 1,
    status: diffFileStatusForOperation(args.operation),
  };
}

// fileChangeDiffLines converts display rows into shared parsed diff lines.
// fileChangeDiffLines 将展示行转换为共享解析 diff 行。
function fileChangeDiffLines(rows: FileChangeDiffRow[]): DiffLine[] {
  return rows
    .filter((row) => row.kind !== "error")
    .map((row, index): DiffLine => {
      if (row.kind === "insert") {
        return {
          content: row.content,
          kind: "added",
          newLine: row.lineNumber ?? index + 1,
          oldLine: null,
        };
      }
      if (row.kind === "delete") {
        return {
          content: row.content,
          kind: "removed",
          newLine: null,
          oldLine: row.lineNumber ?? index + 1,
        };
      }
      return {
        content: row.content,
        kind: row.kind === "omit" || row.kind === "binary" ? "metadata" : "context",
        newLine: row.kind === "omit" || row.kind === "binary" ? null : row.lineNumber,
        oldLine: row.kind === "omit" || row.kind === "binary" ? null : row.lineNumber,
      };
    });
}

// diffFileStatusForOperation maps file-change operations to review diff statuses.
// diffFileStatusForOperation 将文件变更操作映射为代码审核 diff 状态。
function diffFileStatusForOperation(operation: FileChangeOperation): DiffFile["status"] {
  if (operation === "create") return "added";
  if (operation === "delete") return "deleted";
  return "modified";
}

// firstLineNumber returns the first concrete old or new line number.
// firstLineNumber 返回首个具体旧侧或新侧行号。
function firstLineNumber(lines: DiffLine[], key: "oldLine" | "newLine"): number | null {
  for (const line of lines) {
    const value = line[key];
    if (typeof value === "number") {
      return value;
    }
  }
  return null;
}

// changedLineBytes estimates changed text bytes for lightweight diff metadata.
// changedLineBytes 估算轻量 diff 元数据中的变更文本字节数。
function changedLineBytes(lines: DiffLine[]): number {
  let total = 0;
  for (const line of lines) {
    if (line.kind === "added" || line.kind === "removed") {
      total += textByteLength(line.content);
    }
  }
  return total;
}

// maxChangedLineBytes estimates the largest changed row byte size.
// maxChangedLineBytes 估算最大变更行字节数。
function maxChangedLineBytes(lines: DiffLine[]): number {
  let max = 0;
  for (const line of lines) {
    if (line.kind === "added" || line.kind === "removed") {
      max = Math.max(max, textByteLength(line.content));
    }
  }
  return max;
}

// textByteLength returns a UTF-8 byte estimate for diff rendering limits.
// textByteLength 返回用于 diff 渲染限制的 UTF-8 字节估算。
function textByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

// insertRows converts created content into green inserted rows.
// insertRows 将创建内容转换为绿色插入行。
function insertRows(lines: string[]): FileChangeDiffRow[] {
  return lines.map((line, index) => ({
    content: line,
    kind: "insert",
    lineNumber: index + 1,
  }));
}

// modifyRows converts hunk payloads into context, insert, and delete rows.
// modifyRows 将 hunk 载荷转换为上下文、插入与删除行。
function modifyRows(hunks: unknown[]): FileChangeDiffRow[] {
  const rows: FileChangeDiffRow[] = [];
  for (const rawHunk of hunks) {
    const hunk = objectValue(rawHunk);
    if (!hunk) {
      continue;
    }
    const deleted = lineEntries(hunk.delete, "delete");
    const inserted = lineEntries(hunk.insert, "insert");
    const firstChangedLine = deleted[0]?.lineNumber ?? inserted[0]?.lineNumber ?? 1;
    const beforeLines = splitTextLines(textValue(hunk.before) ?? "");
    const beforeStart = Math.max(1, firstChangedLine - beforeLines.length);
    rows.push(...beforeLines.map((line, index) => ({
      content: line,
      kind: "context" as const,
      lineNumber: beforeStart + index,
    })));
    rows.push(...deleted);
    rows.push(...inserted);
    const afterLines = splitTextLines(textValue(hunk.after) ?? "");
    const lastChangedLine = inserted.at(-1)?.lineNumber ?? deleted.at(-1)?.lineNumber ?? firstChangedLine;
    rows.push(...afterLines.map((line, index) => ({
      content: line,
      kind: "context" as const,
      lineNumber: lastChangedLine + index + 1,
    })));
  }
  return rows;
}

// deleteRows converts delete payloads into red removed rows with truncated separators.
// deleteRows 将删除载荷转换为带截断分隔符的红色删除行。
function deleteRows(file: Record<string, unknown>): FileChangeDiffRow[] {
  const mode = stringValue(file.content_mode ?? file.contentMode);
  const content = textValue(file.content);
  if (isBinaryFileContent(content)) {
    return [{ content: "Binary file", kind: "binary", lineNumber: null }];
  }
  if (mode === "truncated") {
    const total = numberValue(file.total_line_count ?? file.totalLineCount) ?? 0;
    const head = splitTextLines(textValue(file.content_head ?? file.contentHead) ?? "");
    const tail = splitTextLines(textValue(file.content_tail ?? file.contentTail) ?? "");
    const omitted = Math.max(0, total - head.length - tail.length);
    const tailStart = Math.max(head.length + 1, total - tail.length + 1);
    return [
      ...head.map((line, index) => ({ content: line, kind: "delete" as const, lineNumber: index + 1 })),
      ...(omitted > 0 ? [{ content: `已省略 ${omitted} 行`, kind: "omit" as const, lineNumber: null }] : []),
      ...tail.map((line, index) => ({ content: line, kind: "delete" as const, lineNumber: tailStart + index })),
    ];
  }
  return splitTextLines(content ?? "").map((line, index) => ({
    content: line,
    kind: "delete",
    lineNumber: index + 1,
  }));
}

// lineEntries converts LuaSkills line entry arrays into diff rows.
// lineEntries 将 LuaSkills 行条目数组转换为 diff 行。
function lineEntries(value: unknown, kind: "insert" | "delete"): FileChangeDiffRow[] {
  return arrayValue(value).map((entry) => {
    const item = objectValue(entry);
    const lineNumber = numberValue(item?.line);
    const content = textValue(item?.content) ?? "";
    return {
      content,
      kind,
      lineNumber,
    };
  });
}

// nativeVulcanFileOperation resolves the operation from known vulcan-file tool names.
// nativeVulcanFileOperation 从已知 vulcan-file 工具名解析操作。
function nativeVulcanFileOperation(blocks: ToolExecutionConversationBlock[]): Exclude<FileChangeOperation, "mixed"> | null {
  const names = blocks.flatMap((block) => [
    block.toolName,
    metadataString(block.rawMetadata, ["tool_name", "toolName", "name", "tool.name", "payload.tool", "host_result.payload.tool", "hostResult.payload.tool"]),
  ]).filter(Boolean) as string[];
  const normalized = names.map(normalizeToolName).join(" ");
  if (normalized.includes("vulcan_file_create")) return "create";
  if (normalized.includes("vulcan_file_edit")) return "modify";
  if (normalized.includes("vulcan_file_delete")) return "delete";
  return null;
}

// nativeVulcanFilePaths extracts file paths from visible call arguments.
// nativeVulcanFilePaths 从可见调用参数中提取文件路径。
function nativeVulcanFilePaths(blocks: ToolExecutionConversationBlock[]): string[] {
  const paths: string[] = [];
  for (const block of blocks) {
    const args = argumentsFromMetadata(block.rawMetadata);
    collectArgumentPaths(args, paths);
  }
  return uniqueStrings(paths);
}

// collectArgumentPaths appends file paths from single-file and batch argument shapes.
// collectArgumentPaths 从单文件与批量参数形态中追加文件路径。
function collectArgumentPaths(value: unknown, paths: string[]): void {
  const args = objectValue(value);
  if (!args) {
    return;
  }
  const file = stringValue(args.file) ?? stringValue(args.path);
  if (file) {
    paths.push(file);
  }
  for (const item of arrayValue(args.files)) {
    const object = objectValue(item);
    const path = stringValue(object?.file) ?? stringValue(object?.path);
    if (path) {
      paths.push(path);
    }
  }
}

// firstChangeSetPayload returns the first valid host_result change_set payload.
// firstChangeSetPayload 返回首个有效的 host_result change_set 载荷。
function firstChangeSetPayload(blocks: ToolExecutionConversationBlock[]): Record<string, unknown> | null {
  for (const block of blocks) {
    const hostResult = objectValue(metadataValue(block.rawMetadata, ["host_result", "hostResult"])) ?? objectValue(block.rawMetadata);
    if (stringValue(hostResult?.kind) === "change_set") {
      const payload = objectValue(hostResult?.payload);
      if (payload) {
        return payload;
      }
    }
  }
  return null;
}

// argumentsFromMetadata reads tool arguments from known transcript metadata shapes.
// argumentsFromMetadata 从已知转录 metadata 形态读取工具参数。
function argumentsFromMetadata(metadata: unknown): unknown {
  return metadataValue(metadata, [
    "arguments",
    "args",
    "input",
    "payload.arguments",
    "payload.args",
    "payload.input",
    "tool.arguments",
    "host_result.arguments",
    "hostResult.arguments",
    "host_result.payload.arguments",
    "hostResult.payload.arguments",
  ]);
}

// errorRowsFromBlocks converts a failed tool payload into expandable error rows.
// errorRowsFromBlocks 将失败工具载荷转换为可展开错误行。
function errorRowsFromBlocks(blocks: ToolExecutionConversationBlock[]): FileChangeDiffRow[] {
  const detail = errorDetailFromBlocks(blocks);
  if (!detail) {
    return [];
  }
  return splitTextLines(detail).map((line) => ({
    content: line,
    kind: "error",
    lineNumber: null,
  }));
}

// errorDetailFromBlocks extracts compact error text without opening the generic tool modal.
// errorDetailFromBlocks 提取紧凑错误文本，避免打开通用工具弹窗。
function errorDetailFromBlocks(blocks: ToolExecutionConversationBlock[]): string | null {
  const candidates: string[] = [];
  for (const block of blocks) {
    for (const path of [
      "error",
      "result.error",
      "payload.error",
      "host_result.error",
      "hostResult.error",
      "stderr",
      "payload.stderr",
      "host_result.payload.stderr",
      "hostResult.payload.stderr",
      "message",
      "payload.message",
    ]) {
      const value = metadataString(block.rawMetadata, [path]);
      if (value) {
        candidates.push(value);
      }
    }
    if (block.status === "failed" && block.body.trim()) {
      candidates.push(block.body.trim());
    }
  }
  return compactJoinedText(candidates);
}

// compactJoinedText joins unique text snippets into a bounded error detail.
// compactJoinedText 将唯一文本片段合并成有界错误详情。
function compactJoinedText(values: string[]): string | null {
  const unique = uniqueStrings(values.map((value) => value.trim()).filter(Boolean));
  if (!unique.length) {
    return null;
  }
  return unique.join("\n").slice(0, 6000);
}

// displayPathForFile returns workspace-relative paths when possible.
// displayPathForFile 在可能时返回工作区相对路径。
function displayPathForFile(path: string, workspacePath?: string | null): { path: string; kind: "project" | "external" | "unknown" } {
  const slashPath = slashPathForDisplay(path);
  const slashWorkspace = workspacePath ? slashPathForDisplay(workspacePath) : "";
  const normalizedPath = normalizePath(path);
  const normalizedWorkspace = workspacePath ? normalizePath(workspacePath) : "";
  if (!normalizedPath) {
    return { kind: "unknown", path };
  }
  if (normalizedWorkspace && pathInsideWorkspace(normalizedPath, normalizedWorkspace)) {
    const relative = slashPath.slice(slashWorkspace.length).replace(/^\/+/, "");
    return { kind: "project", path: compactMiddlePath(relative || basenameFromPath(path)) };
  }
  return { kind: isAbsolutePath(path) ? "external" : "unknown", path: compactMiddlePath(path) };
}

// pathInsideWorkspace checks whether a normalized file path belongs to a workspace root.
// pathInsideWorkspace 检查归一化文件路径是否属于工作区根目录。
function pathInsideWorkspace(path: string, workspace: string): boolean {
  return path === workspace || path.startsWith(`${workspace}/`);
}

// normalizePath normalizes slashes and casing for display-only path comparison.
// normalizePath 为纯展示路径比较归一化斜杠与大小写。
function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

// slashPathForDisplay normalizes separators while preserving display casing.
// slashPathForDisplay 归一化分隔符但保留展示大小写。
function slashPathForDisplay(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

// compactMiddlePath preserves the beginning and final file name for long paths.
// compactMiddlePath 为长路径保留开头与最终文件名。
function compactMiddlePath(path: string): string {
  const normalized = slashPathForDisplay(path);
  if (normalized.length <= 96) {
    return normalized;
  }
  const segments = normalized.split("/").filter(Boolean);
  const fileName = segments.at(-1) ?? normalized;
  const prefix = normalized.startsWith("/") ? `/${segments[0] ?? ""}` : segments[0] ?? "";
  if (!prefix || prefix === fileName) {
    return normalized.length > 96 ? `${normalized.slice(0, 42)}...${normalized.slice(-48)}` : normalized;
  }
  return `${prefix}/.../${fileName}`;
}

// isAbsolutePath checks common Windows and POSIX absolute path shapes.
// isAbsolutePath 检查常见 Windows 与 POSIX 绝对路径形态。
function isAbsolutePath(path: string): boolean {
  return /^[a-z]:[\\/]/i.test(path) || path.startsWith("/") || path.startsWith("\\\\");
}

// basenameFromPath extracts the last segment for path fallback labels.
// basenameFromPath 提取最终片段作为路径回退标签。
function basenameFromPath(path: string): string {
  return path.replace(/\\/g, "/").split("/").filter(Boolean).at(-1) ?? path;
}

// splitTextLines mirrors LuaSkills text line handling for display counts.
// splitTextLines 镜像 LuaSkills 文本行处理以保持展示计数一致。
function splitTextLines(source: string): string[] {
  if (!source) {
    return [];
  }
  const normalized = source.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  const withoutFinalNewline = normalized.endsWith("\n") ? normalized.slice(0, -1) : normalized;
  return withoutFinalNewline ? withoutFinalNewline.split("\n") : [];
}

// isBinaryFileContent detects the stable binary placeholder returned by vulcan-file delete.
// isBinaryFileContent 检测 vulcan-file delete 返回的稳定二进制占位。
function isBinaryFileContent(content: string | null): boolean {
  return content?.trim() === "Binary file";
}

// summarizeOperation collapses per-file operations into one row operation.
// summarizeOperation 将逐文件操作折叠为单个行操作。
function summarizeOperation(files: FileChangeFilePresentation[]): FileChangeOperation {
  const operations = new Set(files.map((file) => file.operation));
  return operations.size === 1 ? files[0]?.operation ?? "mixed" : "mixed";
}

// operationLabel returns one label from operation-specific variants.
// operationLabel 从操作专属变体中返回单个文案。
function operationLabel(operation: FileChangeOperation, create: string, modify: string, remove: string, mixed: string): string {
  if (operation === "create") return create;
  if (operation === "modify") return modify;
  if (operation === "delete") return remove;
  return mixed;
}

// changeOperation normalizes host file change words.
// changeOperation 规范化宿主文件变更词。
function changeOperation(value: unknown): FileChangeOperation {
  const normalized = stringValue(value)?.toLowerCase();
  if (normalized === "create") return "create";
  if (normalized === "modify" || normalized === "edit" || normalized === "update") return "modify";
  if (normalized === "delete" || normalized === "remove") return "delete";
  return "mixed";
}

// changeSetMode normalizes host change-set mode words.
// changeSetMode 规范化宿主变更集模式词。
function changeSetMode(value: unknown): "preview" | "applied" | "unknown" {
  const normalized = stringValue(value)?.toLowerCase();
  if (normalized === "preview") return "preview";
  if (normalized === "applied" || normalized === "apply") return "applied";
  return "unknown";
}

// sumKnown returns a total only when at least one numeric value is known.
// sumKnown 仅在至少存在一个已知数值时返回合计。
function sumKnown(values: Array<number | null>): number | null {
  let found = false;
  let total = 0;
  for (const value of values) {
    if (typeof value === "number") {
      found = true;
      total += value;
    }
  }
  return found ? total : null;
}

// uniqueStrings returns values with stable first-seen ordering.
// uniqueStrings 返回按首次出现排序的去重字符串。
function uniqueStrings(values: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const value of values) {
    if (!value || seen.has(value)) {
      continue;
    }
    seen.add(value);
    result.push(value);
  }
  return result;
}

// fileChangePresentationId creates a stable expansion key from tool identifiers and files.
// fileChangePresentationId 根据工具标识与文件列表创建稳定展开键。
function fileChangePresentationId(blocks: ToolExecutionConversationBlock[], files: FileChangeFilePresentation[], prefix: string): string {
  const identifier = blocks
    .map((block) => block.toolCallId ?? block.toolResultId ?? block.id)
    .find(Boolean);
  const fileKey = files.map((file) => `${file.operation}:${file.path}`).join("|");
  return `file-change:${prefix}:${identifier ?? compactHash(fileKey)}`;
}

// normalizeToolName normalizes tool names across MCP, LuaSkill, and UI aliases.
// normalizeToolName 归一化 MCP、LuaSkill 与 UI 别名中的工具名。
function normalizeToolName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "_");
}

// resolveCommandExecutionPresentation recognizes one-shot shell_command rows and excludes command sessions.
// resolveCommandExecutionPresentation 识别一次性 shell_command 行，并排除命令会话。
function resolveCommandExecutionPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
): CommandExecutionPresentation | null {
  if (blocks.some((block) => block.commandSessionId || metadataString(block.rawMetadata, ["host_result.kind", "hostResult.kind", "kind"]) === "command_session")) {
    return null;
  }
  const toolName = blocks.find((block) => block.toolName)?.toolName
    ?? blocks.map((block) => metadataString(block.rawMetadata, [
      "tool_name",
      "toolName",
      "host_result.payload.tool_name",
      "hostResult.payload.toolName",
      "host_result.payload.tool",
      "hostResult.payload.tool",
    ])).find(Boolean)
    ?? null;
  const hostKind = blocks
    .map((block) => metadataString(block.rawMetadata, ["host_result.kind", "hostResult.kind", "kind"]))
    .find(Boolean);
  if (toolName !== "shell_command" && hostKind !== "command_run") {
    return null;
  }

  const display = firstShellCommandDisplay(blocks);
  const command = display?.command
    ?? blocks.map((block) => commandFromMetadata(block.rawMetadata)).find(Boolean)
    ?? blocks.map((block) => commandFromText(block.body)).find(Boolean)
    ?? "";
  if (!command) {
    return null;
  }

  return {
    command,
    detailRef,
    exitCode: display?.exitCode ?? null,
    id: commandPresentationId(blocks, command),
    kind: "command_execution",
    state: displayState,
    stderr: display?.stderr ?? "",
    stdout: display?.stdout ?? "",
  };
}

// firstShellCommandDisplay returns the first structured shell_command display found on visible blocks.
// firstShellCommandDisplay 返回可见块上找到的首个结构化 shell_command 展示。
function firstShellCommandDisplay(blocks: ToolExecutionConversationBlock[]): ShellCommandDisplay | null {
  for (const block of blocks) {
    const display = shellCommandDisplayFromMetadata(block.rawMetadata) ?? parseShellCommandDisplayFromText(block.body);
    if (display) {
      return display;
    }
  }
  return null;
}

// mergeCommandExecutionDisplay applies structured shell_command fields without losing known values.
// mergeCommandExecutionDisplay 合并结构化 shell_command 字段，同时保留已有已知值。
function mergeCommandExecutionDisplay(
  presentation: CommandExecutionPresentation,
  display: ShellCommandDisplay,
): CommandExecutionPresentation {
  return {
    ...presentation,
    command: display.command || presentation.command,
    exitCode: display.exitCode ?? presentation.exitCode,
    stderr: display.stderr || presentation.stderr,
    stdout: display.stdout || presentation.stdout,
  };
}

// commandPresentationId creates a stable expansion key from tool identifiers and command text.
// commandPresentationId 根据工具标识与命令文本创建稳定展开键。
function commandPresentationId(blocks: ToolExecutionConversationBlock[], command: string): string {
  const identifier = blocks
    .map((block) => block.toolCallId ?? block.toolResultId ?? block.id)
    .find(Boolean);
  return `command:${identifier ?? compactHash(command)}`;
}

// ShellCommandDisplay stores parsed command output fields before they become a presentation model.
// ShellCommandDisplay 保存转为展示模型前解析出的命令输出字段。
interface ShellCommandDisplay {
  // Raw command line.
  // 原始命令行。
  command: string;
  // Parsed stdout text.
  // 解析出的 stdout 文本。
  stdout: string;
  // Parsed stderr text.
  // 解析出的 stderr 文本。
  stderr: string;
  // Parsed process exit code.
  // 解析出的进程退出码。
  exitCode: number | null;
}

// shellCommandDisplayFromMetadata reads a structured shell_command display from metadata.
// shellCommandDisplayFromMetadata 从 metadata 中读取结构化 shell_command 展示。
function shellCommandDisplayFromMetadata(metadata: unknown): ShellCommandDisplay | null {
  const display = objectValue(metadataValue(metadata, [
    "display",
    "metadata.display",
    "payload.display",
    "host_result.display",
    "hostResult.display",
    "host_result.payload.display",
    "hostResult.payload.display",
  ]));
  if (!display || stringValue(display.kind) !== "shell_command") {
    return null;
  }
  const command = stringValue(display.command) ?? "";
  const stdout = stringValue(display.stdout) ?? "";
  const stderr = stringValue(display.stderr) ?? "";
  const exitCode = numberValue(display.exit_code ?? display.exitCode);
  if (!command && !stdout && !stderr && exitCode === null) {
    return null;
  }
  return { command, exitCode, stderr, stdout };
}

// parseShellCommandDisplayFromText parses the stable shell_command model output format.
// parseShellCommandDisplayFromText 解析稳定的 shell_command 模型输出格式。
function parseShellCommandDisplayFromText(source: string): ShellCommandDisplay | null {
  const command = commandFromText(source) ?? "";
  const exitCode = exitCodeFromText(source);
  const stdout = sectionFromText(source, "[stdout]");
  const stderr = sectionFromText(source, "[stderr]");
  if (!command && !stdout && !stderr && exitCode === null) {
    return null;
  }
  return { command, exitCode, stderr, stdout };
}

// commandFromMetadata extracts the shell command argument from known metadata shapes.
// commandFromMetadata 从已知 metadata 形状中提取 shell 命令参数。
function commandFromMetadata(metadata: unknown): string | null {
  return metadataString(metadata, [
    "arguments.command",
    "payload.arguments.command",
    "host_result.arguments.command",
    "hostResult.arguments.command",
    "host_result.payload.arguments.command",
    "hostResult.payload.arguments.command",
    "host_result.payload.command",
    "hostResult.payload.command",
    "command",
  ]);
}

// commandFromText extracts a command header from rendered shell_command text.
// commandFromText 从已渲染 shell_command 文本中提取命令头。
function commandFromText(source: string): string | null {
  const match = source.match(/^Command:\s*(.+)$/im);
  return match?.[1]?.trim() || null;
}

// exitCodeFromText extracts the exit code from rendered shell_command text.
// exitCodeFromText 从已渲染 shell_command 文本中提取退出码。
function exitCodeFromText(source: string): number | null {
  const match = source.match(/^Exit code:\s*(-?\d+)$/im);
  return match ? numberValue(match[1]) : null;
}

// sectionFromText extracts a bracketed output section until the next bracketed section.
// sectionFromText 提取方括号输出区段直到下一个方括号区段。
function sectionFromText(source: string, marker: "[stdout]" | "[stderr]"): string {
  const lines = source.split(/\r?\n/);
  const markerIndex = lines.findIndex((line) => line.trim() === marker);
  if (markerIndex < 0) {
    return "";
  }
  const collected: string[] = [];
  for (const line of lines.slice(markerIndex + 1)) {
    if (/^\[(stdout|stderr)\]\s*$/.test(line.trim())) {
      break;
    }
    collected.push(line);
  }
  return collected.join("\n").trim();
}

// metadataString returns the first non-empty string at any candidate metadata path.
// metadataString 返回任一候选 metadata 路径上的首个非空字符串。
function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = stringValue(metadataValue(metadata, [path]));
    if (value) {
      return value;
    }
  }
  return null;
}

// metadataValue returns the value at the first existing dotted path.
// metadataValue 返回首个存在的点分路径值。
function metadataValue(metadata: unknown, paths: string[]): unknown {
  for (const candidate of paths) {
    const segments = candidate.split(".");
    let current: unknown = metadata;
    let found = true;
    for (const segment of segments) {
      if (!current || typeof current !== "object" || !(segment in current)) {
        found = false;
        break;
      }
      current = (current as Record<string, unknown>)[segment];
    }
    if (found) {
      return current;
    }
  }
  return null;
}

// objectValue narrows unknown values to plain records.
// objectValue 将未知值收窄为普通对象记录。
function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

// arrayValue narrows unknown values to arrays.
// arrayValue 将未知值收窄为数组。
function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

// stringValue returns trimmed strings and null for blank or non-string values.
// stringValue 返回裁剪后的字符串，空白或非字符串值返回 null。
function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

// textValue returns raw string values without trimming file content.
// textValue 返回不裁剪文件内容的原始字符串值。
function textValue(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

// numberValue normalizes numeric JSON values or numeric strings.
// numberValue 规范化 JSON 数值或数字字符串。
function numberValue(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.trunc(value);
  }
  if (typeof value === "string" && /^-?\d+$/.test(value.trim())) {
    return Number.parseInt(value.trim(), 10);
  }
  return null;
}

// compactHash creates a small deterministic hash for rows without backend ids.
// compactHash 为没有后端标识的行创建短小稳定哈希。
function compactHash(source: string): string {
  let hash = 0;
  for (let index = 0; index < source.length; index += 1) {
    hash = Math.imul(31, hash) + source.charCodeAt(index) | 0;
  }
  return Math.abs(hash).toString(36);
}
