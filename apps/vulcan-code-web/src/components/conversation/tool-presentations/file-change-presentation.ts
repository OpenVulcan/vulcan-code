import { resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { FileAccessAction, FileChangeDiffRow, FileChangeFilePresentation, FileChangeOperation, FileChangePresentation, ToolPresentationDisplayState, ToolPresentationResolveOptions } from "./types";
import { argumentsFromMetadata, arrayValue, compactHash, fixedToolName, metadataString, metadataValue, normalizeToolName, numberValue, objectValue, stringValue, textValue } from "./presentation-value-readers";
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
  const nativeToolId = exactNativeToolId(blocks);
  if (nativeToolId === "file") {
    const accessAction = nativeFileAccessAction(blocks);
    return accessAction
      ? buildNativeFileAccessPresentation(blocks, displayState, accessAction, options)
      : null;
  }
  if (nativeToolId === "file_edit") {
    const hostPayload = displayState !== "running" ? lastChangeSetPayload(blocks) : null;
    if (hostPayload) {
      return buildHostChangeSetPresentation(blocks, displayState, hostPayload, options, "native_file_edit");
    }
    const streamedPreview = nativeFileEditPreview(options.requestPreview);
    if (streamedPreview) {
      return buildNativeFileEditPreviewPresentation(blocks, displayState, streamedPreview, options);
    }
    return buildNativeVulcanFilePresentation(blocks, displayState, "modify", options, "native_file_edit");
  }
  const hostPayload = displayState !== "running" ? lastChangeSetPayload(blocks) : null;
  if (hostPayload) {
    return buildHostChangeSetPresentation(blocks, displayState, hostPayload, options, "host_change_set");
  }
  const nativeOperation = nativeVulcanFileOperation(blocks);
  if (!nativeOperation) {
    return null;
  }
  return buildNativeVulcanFilePresentation(blocks, displayState, nativeOperation, options, "native_vulcan_file");
}

// NativeFileEditPreviewFile is the bounded per-file projection of a parser preview.
// NativeFileEditPreviewFile 是请求解析器预览的有界逐文件投影。
interface NativeFileEditPreviewFile {
  // Previewed file path.
  // 预览中的文件路径。
  path: string;
  // Normalized operation shown before the host change_set exists.
  // 宿主 change_set 产生前显示的规范化操作。
  operation: Exclude<FileChangeOperation, "mixed">;
  // Diff-style lines already bounded by the host parser.
  // 宿主解析器已经限制长度的 diff 风格行。
  rows: FileChangeDiffRow[];
}

// buildNativeFileEditPreviewPresentation maps safe request-stream metadata to the shared file card.
// buildNativeFileEditPreviewPresentation 将安全请求流元数据映射到共享文件卡片。
function buildNativeFileEditPreviewPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  preview: NativeFileEditPreviewFile[],
  options: ToolPresentationResolveOptions,
): FileChangePresentation | null {
  const files = preview.map((item, index) => {
    const added = item.rows.filter((row) => row.kind === "insert").length;
    const removed = item.rows.filter((row) => row.kind === "delete").length;
    return buildFilePresentation({
      added,
      blocks,
      expandable: item.rows.length > 0,
      index,
      operation: item.operation,
      path: item.path,
      removed,
      rows: item.rows,
      resourceId: null,
      stableId: item.path,
      idOperation: "file_edit",
      operationIds: [],
      previousPath: null,
      readOnly: false,
      options,
    });
  });
  if (!files.length) {
    return null;
  }
  return {
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(blocks, files, "native-file-edit"),
    interactive: files.some((file) => file.expandable),
    kind: "file_change",
    mode: "preview",
    operation: summarizeOperation(files),
    source: "native_file_edit",
    state: displayState,
    totalAdded: sumKnown(files.map((file) => file.added)),
    totalRemoved: sumKnown(files.map((file) => file.removed)),
  };
}

// buildHostChangeSetPresentation converts one host change_set payload into display rows.
// buildHostChangeSetPresentation 将宿主 change_set 载荷转换为展示行。
function buildHostChangeSetPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  payload: Record<string, unknown>,
  options: ToolPresentationResolveOptions,
  source: "host_change_set" | "native_file_edit",
): FileChangePresentation | null {
  const rawFiles = arrayValue(payload.files);
  if (!rawFiles.length) {
    return null;
  }
  const operationIds = operationIdsForResource(payload);
  const files = rawFiles
    .map((file, index) => buildHostChangeSetFile(file, index, blocks, options, operationIds, source))
    .filter((file): file is FileChangeFilePresentation => Boolean(file));
  if (!files.length) {
    return null;
  }
  const mode = changeSetMode(payload.mode);
  const operation = summarizeOperation(files);
  return {
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(
      blocks,
      files,
      source === "native_file_edit" ? "native-file-edit" : "change-set",
    ),
    interactive: displayState !== "running" && files.some((file) => file.expandable),
    kind: "file_change",
    mode,
    operation,
    source,
    accessAction: null,
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
  source: "native_vulcan_file" | "native_file_edit",
): FileChangePresentation | null {
  const paths = nativeFilePaths(blocks);
  // A file card requires one real path; request formation must not fabricate a generic `File` identity.
  // 文件卡片必须具备真实路径；请求构造阶段不得伪造通用 `File` 身份。
  if (!paths.length) {
    return null;
  }
  const displayPaths = paths;
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
      resourceId: null,
      stableId: source === "native_file_edit" ? path : null,
      idOperation: source === "native_file_edit" ? "file_edit" : null,
      operationIds: [],
      previousPath: null,
      readOnly: false,
      options,
    });
  });
  return {
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(
      blocks,
      files,
      source === "native_file_edit" ? "native-file-edit" : `native-${operation}`,
    ),
    interactive: displayState === "failed",
    kind: "file_change",
    mode: "unknown",
    operation,
    source,
    state: displayState,
    totalAdded: sumKnown(files.map((file) => file.added)),
    totalRemoved: sumKnown(files.map((file) => file.removed)),
  };
}

// buildNativeFileAccessPresentation projects read-only file actions onto the shared file card shell.
// buildNativeFileAccessPresentation 将只读 file 动作映射到共享文件卡片外壳。
function buildNativeFileAccessPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  accessAction: FileAccessAction,
  options: ToolPresentationResolveOptions,
): FileChangePresentation | null {
  const paths = nativeFileAccessPaths(blocks);
  if (!paths.length) {
    return null;
  }
  const files = paths.map((path, index) => {
    const rows = displayState === "failed"
      ? errorRowsFromBlocks(blocks)
      : nativeFileResultRows(blocks, index, accessAction);
    return buildFilePresentation({
      added: null,
      blocks,
      expandable: rows.length > 0,
      index,
      operation: "mixed",
      path,
      removed: null,
      rows,
      resourceId: null,
      operationIds: [],
      previousPath: null,
      readOnly: true,
      options,
    });
  });
  return {
    accessAction,
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(blocks, files, `native-file-${accessAction}`),
    interactive: displayState !== "running" && files.some((file) => file.expandable),
    kind: "file_change",
    mode: "unknown",
    operation: "mixed",
    source: "native_file",
    state: displayState,
    totalAdded: null,
    totalRemoved: null,
  };
}

// buildHostChangeSetFile converts one change_set file entry into one display file.
// buildHostChangeSetFile 将单个 change_set 文件条目转换为展示文件。
function buildHostChangeSetFile(
  value: unknown,
  index: number,
  blocks: ToolExecutionConversationBlock[],
  options: ToolPresentationResolveOptions,
  operationIdsByResource: Map<string, string[]>,
  source: "host_change_set" | "native_file_edit",
): FileChangeFilePresentation | null {
  const file = objectValue(value);
  if (!file) {
    return null;
  }
  const operation = changeOperation(file.change);
  const previousPath = stringValue(file.old_path ?? file.oldPath);
  const path = stringValue(file.new_path ?? file.newPath ?? file.path) ?? previousPath ?? `file-${index + 1}`;
  const resourceId = stringValue(file.resource_id ?? file.resourceId);
  const operationIds = resourceId ? operationIdsByResource.get(resourceId) ?? [] : [];
  const stableId = source === "native_file_edit"
    ? nativeRequestPathForHostPath(blocks, previousPath ?? path) ?? previousPath ?? path
    : null;
  const idOperation = source === "native_file_edit" ? "file_edit" : null;
  const displayPathOverride = operation === "rename" && previousPath
    ? `${displayPathForFile(previousPath, options.workspacePath).path} → ${displayPathForFile(path, options.workspacePath).path}`
    : null;
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
      resourceId,
      stableId,
      idOperation,
      operationIds,
      previousPath,
      readOnly: false,
      displayPathOverride,
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
      resourceId,
      stableId,
      idOperation,
      operationIds,
      previousPath,
      readOnly: false,
      displayPathOverride,
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
      resourceId,
      stableId,
      idOperation,
      operationIds,
      previousPath,
      readOnly: false,
      displayPathOverride,
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
    resourceId,
    stableId,
    idOperation,
    operationIds,
    previousPath,
    readOnly: false,
    displayPathOverride,
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
  // Canonical resource identity used for stable multi-file correlation.
  // 用于稳定多文件关联的 canonical 资源标识。
  resourceId: string | null;
  // Native operation ids mapped to the resource.
  // 映射到该资源的原生操作标识。
  operationIds: string[];
  // Previous path for a rename row.
  // 重命名行的原始路径。
  previousPath: string | null;
  // Optional display label for a rename path pair.
  // 重命名路径对的可选展示标签。
  displayPathOverride?: string | null;
  // Whether rows are read-only result lines.
  // 当前行是否为只读结果行。
  readOnly: boolean;
  // Resolver options.
  // 解析选项。
  options: ToolPresentationResolveOptions;
  // Stable path identity shared by native streamed preview and terminal change_set rows.
  // 原生流式预览与终态 change_set 行共用的稳定路径身份。
  stableId?: string | null;
  // Optional operation identity used when the visual operation changes during one native call.
  // 当同一次原生调用中的视觉操作变化时使用的可选稳定操作身份。
  idOperation?: string | null;
}): FileChangeFilePresentation {
  const displayPath = displayPathForFile(args.path, args.options.workspacePath);
  const diffFile = args.readOnly
    ? null
    : fileChangeDiffFile({
        added: args.added,
        index: args.index,
        operation: args.operation,
        path: displayPath.path,
        removed: args.removed,
        rows: args.rows,
      });
  const identity = args.blocks
    .map((block) => block.toolCallId ?? block.id)
    .find(Boolean) ?? "file";
  const fileIdentity = args.stableId
    ? compactHash(stablePathIdentity(args.stableId, args.options.workspacePath))
    : args.resourceId ?? `${args.index}:${compactHash(args.path)}`;
  const operationIdentity = args.idOperation ?? args.operation;
  return {
    added: args.added,
    diffFile,
    displayPath: args.displayPathOverride ?? displayPath.path,
    expandable: args.expandable && (args.readOnly ? args.rows.length > 0 : Boolean(diffFile)),
    icon: resolveFileTypeIcon(args.path),
    id: `${identity}:${fileIdentity}:${operationIdentity}`,
    operationIds: args.operationIds,
    operation: args.operation,
    path: args.path,
    pathKind: displayPath.kind,
    previousPath: args.previousPath,
    readOnly: args.readOnly,
    resourceId: args.resourceId,
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
  for (const normalized of names.map(normalizeToolName)) {
    if (normalized === "vulcan_file_create") return "create";
    if (normalized === "vulcan_file_edit") return "modify";
    if (normalized === "vulcan_file_delete") return "delete";
  }
  return null;
}

// exactNativeToolId returns only the normalized public id of the current native call.
// exactNativeToolId 仅返回当前原生调用的规范化公开标识。
function exactNativeToolId(blocks: ToolExecutionConversationBlock[]): string | null {
  const name = fixedToolName(blocks);
  return name ? normalizeToolName(name) : null;
}

// nativeFileAccessAction reads the action selected by the exact native file contract.
// nativeFileAccessAction 读取原生 file 契约中明确选择的动作。
function nativeFileAccessAction(blocks: ToolExecutionConversationBlock[]): FileAccessAction | null {
  for (const block of blocks) {
    const action = stringValue(objectValue(argumentsFromMetadata(block.rawMetadata))?.action);
    if (action === "read" || action === "list" || action === "search" || action === "stat") {
      return action;
    }
  }
  return null;
}

// nativeFilePaths extracts file paths from visible call arguments.
// nativeFilePaths 从可见调用参数中提取文件路径。
function nativeFilePaths(blocks: ToolExecutionConversationBlock[]): string[] {
  const paths: string[] = [];
  for (const block of blocks) {
    const args = argumentsFromMetadata(block.rawMetadata);
    collectArgumentPaths(args, paths);
  }
  return uniqueStrings(paths);
}

// nativeRequestPathForHostPath bridges absolute terminal paths back to the original model path when possible.
// nativeRequestPathForHostPath 尽可能把终态绝对路径映射回模型原始路径。
function nativeRequestPathForHostPath(
  blocks: ToolExecutionConversationBlock[],
  hostPath: string,
): string | null {
  const requestPaths = nativeFilePaths(blocks);
  const normalizedHostPath = stablePathIdentity(hostPath);
  const exact = requestPaths.find((path) => stablePathIdentity(path) === normalizedHostPath);
  if (exact) {
    return exact;
  }
  const suffixMatches = requestPaths.filter((path) => {
    const normalizedPath = stablePathIdentity(path).replace(/^\/+/, "");
    return normalizedHostPath.endsWith(`/${normalizedPath}`);
  });
  if (!suffixMatches.length) {
    return null;
  }
  const longestLength = Math.max(
    ...suffixMatches.map((path) => stablePathIdentity(path).length),
  );
  const longestMatches = suffixMatches.filter(
    (path) => stablePathIdentity(path).length === longestLength,
  );
  return longestMatches.length === 1 ? longestMatches[0] : null;
}

// nativeFileAccessPaths preserves the exact target order used by the native file result array.
// nativeFileAccessPaths 保留原生 file 结果数组所使用的精确目标顺序。
function nativeFileAccessPaths(blocks: ToolExecutionConversationBlock[]): string[] {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const paths: string[] = [];
    collectArgumentPaths(argumentsFromMetadata(blocks[index]?.rawMetadata), paths);
    if (paths.length) {
      return paths;
    }
  }
  return [];
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
  for (const item of arrayValue(args.targets)) {
    const object = objectValue(item);
    const path = stringValue(object?.path);
    if (path) {
      paths.push(path);
    }
  }
  for (const item of arrayValue(args.files)) {
    const object = objectValue(item);
    const path = stringValue(object?.file);
    if (path) {
      paths.push(path);
    }
  }
  for (const item of arrayValue(args.operations)) {
    const object = objectValue(item);
    const path = stringValue(object?.path);
    if (path) {
      paths.push(path);
    }
  }
}

// nativeFileEditPreview parses only the host-owned file_edit preview shape and never arbitrary tool prose.
// nativeFileEditPreview 仅解析宿主自有的 file_edit 预览结构，不解析任意工具正文。
function nativeFileEditPreview(value: Record<string, unknown> | null | undefined): NativeFileEditPreviewFile[] | null {
  if (stringValue(value?.kind) !== "file_edit") {
    return null;
  }
  const paths = uniqueStrings(arrayValue(value?.affected_resources).map((item) => stringValue(item)).filter((item): item is string => Boolean(item)));
  if (!paths.length) {
    return null;
  }
  const files = paths.map<NativeFileEditPreviewFile>((path) => ({
    operation: "modify",
    path,
    rows: [],
  }));
  const structuredPreviewTruncated = arrayValue(value?.redaction_flags).some(
    (flag) => stringValue(flag) === "structured_preview_truncated",
  );
  const structuredFileIndexes = new Set<number>();
  const structuredLines = arrayValue(value?.structured_lines);
  if (!structuredPreviewTruncated) {
    for (const item of structuredLines) {
      const line = objectValue(item);
      const resourceIndex = numberValue(line?.resource_index);
      const path = resourceIndex === null ? null : paths[resourceIndex];
      const fileIndex = path ? paths.indexOf(path) : -1;
      const side = stringValue(line?.side);
      const content = textValue(line?.content);
      const file = fileIndex >= 0 ? files[fileIndex] : null;
      if (!file || content === null || (side !== "old" && side !== "new")) {
        continue;
      }
      structuredFileIndexes.add(fileIndex);
      file.rows.push({
        content,
        kind: side === "old" ? "delete" : "insert",
        lineNumber: null,
      });
    }
  }
  const body = textValue(value?.body_preview);
  if (!body) {
    return files;
  }
  let currentIndex: number | null = null;
  let nextLineNumber = 1;
  for (const line of splitTextLines(body)) {
    const header = nativeFileEditPreviewHeader(line, paths);
    if (header) {
      currentIndex = paths.indexOf(header.path);
      const file = files[currentIndex];
      if (file) {
        file.operation = header.operation;
        nextLineNumber = 1;
      }
      continue;
    }
    if (!structuredPreviewTruncated && currentIndex !== null && structuredFileIndexes.has(currentIndex)) {
      continue;
    }
    if (currentIndex === null || (!line.startsWith("- ") && !line.startsWith("+ "))) {
      continue;
    }
    const file = files[currentIndex];
    if (!file) {
      continue;
    }
    file.rows.push({
      content: line.slice(2),
      kind: line.startsWith("- ") ? "delete" : "insert",
      lineNumber: nextLineNumber,
    });
    nextLineNumber += 1;
  }
  return files;
}

// nativeFileEditPreviewHeader recognizes the exact path-and-operation line emitted by the Rust parser.
// nativeFileEditPreviewHeader 识别 Rust 解析器输出的精确路径与操作头行。
function nativeFileEditPreviewHeader(
  line: string,
  paths: string[],
): { operation: Exclude<FileChangeOperation, "mixed">; path: string } | null {
  const match = /^(.*) \((create|replace|delete_file|move_file)\)$/.exec(line);
  if (!match) {
    return null;
  }
  const path = match[1];
  if (!path || !paths.includes(path)) {
    return null;
  }
  const operation = match[2] === "create"
    ? "create"
    : match[2] === "delete_file"
      ? "delete"
      : match[2] === "move_file"
        ? "rename"
        : "modify";
  return { operation, path };
}

// nativeFileResultRows turns the bounded JSON result of one file target into display rows.
// nativeFileResultRows 将一个 file 目标的有界 JSON 结果转换为展示行。
function nativeFileResultRows(
  blocks: ToolExecutionConversationBlock[],
  targetIndex: number,
  action: FileAccessAction,
): FileChangeDiffRow[] {
  const output = visibleJsonOutput(blocks);
  const result = objectValue(arrayValue(output?.results)[targetIndex]);
  if (!result) {
    return [];
  }
  if (result.ok === false) {
    const error = objectValue(result.error);
    const code = textValue(error?.code);
    const message = textValue(error?.message);
    return [{
      content: `[${code ?? "file_error"}] ${message ?? "The file action failed."}`,
      kind: "error",
      lineNumber: null,
    }];
  }
  if (action === "read") {
    return arrayValue(result.lines).slice(0, 2000).map((value) => {
      const line = objectValue(value);
      return {
        content: textValue(line?.content) ?? "",
        kind: "context" as const,
        lineNumber: numberValue(line?.line),
      };
    });
  }
  if (action === "list") {
    return arrayValue(result.entries).slice(0, 2000).map((value, index) => {
      const entry = objectValue(value);
      return {
        content: textValue(entry?.path) ?? "",
        kind: "context" as const,
        lineNumber: index + 1,
      };
    });
  }
  if (action === "search") {
    return arrayValue(result.matches).slice(0, 2000).map((value) => {
      const match = objectValue(value);
      const path = textValue(match?.path) ?? "";
      const line = numberValue(match?.line);
      const text = textValue(match?.text) ?? "";
      return {
        content: `${path}:${line ?? "?"} ${text}`.trim(),
        kind: "context" as const,
        lineNumber: line,
      };
    });
  }
  return Object.entries(result)
    .filter(([key]) => !["ok", "path"].includes(key))
    .slice(0, 100)
    .map(([key, value]) => ({
      content: `${key}: ${typeof value === "string" ? value : JSON.stringify(value)}`.slice(0, 2000),
      kind: "context" as const,
      lineNumber: null,
    }));
}

// visibleJsonOutput finds the bounded native file result body without parsing arbitrary prose.
// visibleJsonOutput 仅解析有界的原生 file 结果正文，不尝试解析任意自然语言。
function visibleJsonOutput(blocks: ToolExecutionConversationBlock[]): Record<string, unknown> | null {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const body = blocks[index]?.body.trim();
    if (!body) {
      continue;
    }
    try {
      const parsed = JSON.parse(body) as unknown;
      const object = objectValue(parsed);
      if (object && Array.isArray(object.results)) {
        return object;
      }
    } catch {
      continue;
    }
  }
  return null;
}

// operationIdsForResource maps native extension operation ids to canonical resources.
// operationIdsForResource 将原生扩展中的操作标识映射到 canonical 资源。
function operationIdsForResource(payload: Record<string, unknown>): Map<string, string[]> {
  const result = new Map<string, string[]>();
  const extensions = objectValue(payload.extensions);
  const extension = objectValue(extensions?.["vulcan.file_edit.v1"]);
  for (const value of arrayValue(extension?.operation_results)) {
    const operation = objectValue(value);
    const operationId = stringValue(operation?.operation_id);
    const resourceId = stringValue(operation?.resource_id);
    if (!operationId || !resourceId) {
      continue;
    }
    const ids = result.get(resourceId) ?? [];
    if (!ids.includes(operationId)) {
      ids.push(operationId);
      result.set(resourceId, ids);
    }
  }
  return result;
}

// lastChangeSetPayload returns the newest valid host_result change_set payload.
// firstChangeSetPayload 返回首个有效的 host_result change_set 载荷。
function lastChangeSetPayload(blocks: ToolExecutionConversationBlock[]): Record<string, unknown> | null {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const block = blocks[index];
    if (!block) {
      continue;
    }
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

// stablePathIdentity maps relative and absolute project paths to one display-independent key.
// stablePathIdentity 将项目内相对路径和绝对路径映射为与展示无关的统一键。
function stablePathIdentity(path: string, workspacePath?: string | null): string {
  const normalizedPath = normalizePath(path).replace(/^\.\//, "");
  const normalizedWorkspace = workspacePath
    ? normalizePath(workspacePath)
    : "";
  if (normalizedWorkspace && pathInsideWorkspace(normalizedPath, normalizedWorkspace)) {
    return normalizedPath.slice(normalizedWorkspace.length).replace(/^\/+/, "");
  }
  return normalizedPath;
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
  if (normalized === "rename" || normalized === "move") return "rename";
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
  const fileKey = files.map((file) => file.id).sort().join("|");
  return `file-change:${prefix}:${identifier ?? compactHash(fileKey)}`;
}
