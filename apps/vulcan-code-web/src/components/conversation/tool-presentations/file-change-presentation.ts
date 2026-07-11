import { resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { FileChangeDiffRow, FileChangeFilePresentation, FileChangeOperation, FileChangePresentation, ToolPresentationDisplayState, ToolPresentationResolveOptions } from "./types";
import { argumentsFromMetadata, arrayValue, compactHash, fixedHostPayload, fixedToolName, metadataString, metadataValue, normalizeToolName, numberValue, objectValue, stringValue, textValue } from "./presentation-value-readers";
import { deleteRows, fileChangeDiffFile, insertRows, modifyRows, splitTextLines } from "./file-change-diff-presentation";

// fileChangeStatusLabel returns the compact fallback status label for a file-change card.
// fileChangeStatusLabel 返回文件变更卡片使用的紧凑兜底状态文案。
export function fileChangeStatusLabel(presentation: FileChangePresentation, operation: FileChangeOperation): string {
  if (presentation.state === "running") {
    return operationLabel(operation, "Writing", "Editing", "Deleting", "Changing");
  }
  if (presentation.state === "failed") {
    return operationLabel(operation, "Write failed", "Edit failed", "Delete failed", "Change failed");
  }
  if (presentation.mode === "preview") {
    return operationLabel(operation, "Will write", "Will edit", "Will delete", "Will change");
  }
  return operationLabel(operation, "Written", "Edited", "Deleted", "Changed");
}

// resolveFileChangePresentation recognizes native vulcan-file calls and completed host change sets.
// resolveFileChangePresentation 识别原生 vulcan-file 调用和已完成的宿主变更集。
export function resolveFileChangePresentation(
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
  const displayPaths = paths.length ? paths : ["File"];
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

// nativeVulcanFileOperation resolves the operation from known vulcan-file tool names.
// nativeVulcanFileOperation 从已知 vulcan-file 工具名解析操作。
function nativeVulcanFileOperation(blocks: ToolExecutionConversationBlock[]): Exclude<FileChangeOperation, "mixed"> | null {
  const names = blocks.flatMap((block) => [
    block.toolName,
    metadataString(block.rawMetadata, ["tool_name"]),
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
  const file = stringValue(args.file);
  if (file) {
    paths.push(file);
  }
  for (const item of arrayValue(args.files)) {
    const object = objectValue(item);
    const path = stringValue(object?.file);
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

// errorRowsFromBlocks converts compact tool errors into expandable diff-style rows.
// errorRowsFromBlocks 将紧凑工具错误转换为可展开的 diff 风格行。
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
    .map((block) => block.toolCallId ?? block.id)
    .find(Boolean);
  const fileKey = files.map((file) => `${file.operation}:${file.path}`).join("|");
  return `file-change:${prefix}:${identifier ?? compactHash(fileKey)}`;
}
