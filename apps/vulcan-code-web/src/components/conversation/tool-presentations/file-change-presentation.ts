import { resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { FileAccessAction, FileAccessItemPresentation, FileAccessLineRangePresentation, FileAccessPresentation, FileAccessPresentationKind, FileChangeDiffRow, FileChangeFilePresentation, FileChangeOperation, FileChangePresentation, FileTextFormatPresentation, ToolPresentationDisplayState, ToolPresentationResolveOptions } from "./types";
import { argumentsFromMetadata, arrayValue, booleanValue, compactHash, fixedToolName, metadataString, metadataValue, normalizeToolName, numberValue, objectValue, stringValue, textValue } from "./presentation-value-readers";
import { deleteRows, fileChangeDiffFile, insertRows, modifyRows, splitTextLines } from "./file-change-diff-presentation";
import { fixedHostPayload, fixedV1ToolEvent } from "./presentation-value-readers";
import { buildV1ChangeSetFilePresentation } from "./v1-change-set-presentation";
import { v1ModelResult } from "./v1-tool-domain-presentation";
import type { PersistentChangeSet } from "@/service/conversation-event-window-store";

/** Maximum display rows shared by current and historical file-access previews.
 * 当前与历史文件访问预览共用的最大展示行数。
 */
const FILE_ACCESS_PREVIEW_ROW_LIMIT = 2000;

/** Adapts the authoritative persisted changeset into the established interactive diff card.
 * 将权威持久化变更集适配为既有可交互差异卡。
 * Parameters retain source blocks, state, changeset, display options and detail identity; returns counts and rows from one source.
 * 参数保留源块、状态、变更集、展示选项与详情身份；返回同源的统计与差异行。 */
function buildPersistentChangeSetPresentation(blocks: ToolExecutionConversationBlock[], state: ToolPresentationDisplayState,
  changeset: PersistentChangeSet, options: ToolPresentationResolveOptions, detailRef?: ConversationDetailRef): FileChangePresentation {
  // Native request and completion share the same identity prefix so expansion survives the handoff.
  // 原生请求与完成阶段共享身份前缀，确保展开状态在交接后保留。
  const nativeId = exactNativeToolId(blocks);
  // source follows the same exact native-tool branches as the legacy host adapter.
  // source 遵循旧宿主适配器相同的精确原生工具分支。
  const source = nativeId === "file_edit" ? "native_file_edit" : nativeId === "apply_patch" ? "native_apply_patch" : nativeId === "file" ? "native_file_mutation" : "host_change_set";
  // prefix retains the established per-call expansion identity.
  // prefix 保留既有按调用划分的展开身份。
  const prefix = source === "native_file_edit" ? "native-file-edit" : source === "native_apply_patch" ? "native-apply-patch" : source === "native_file_mutation" ? "native-file-mutation" : "change-set";
  // An empty failed changeset describes no committed changes, not the absence of a requested file.
  // 失败的空变更集表示没有提交变更，而不是没有请求目标文件。
  if (state === "failed" && changeset.files.length === 0) {
    return buildNoEffectiveChangePresentation(blocks, state, source, options, detailRef);
  }
  // files reuse the V1 lifecycle decoder so full, truncated and omitted content have identical accounting.
  // files 复用 V1 生命周期解码器，使完整、截断与省略正文保持相同统计规则。
  const files = changeset.files.map((file, index) => {
    // canonical owns exact paths, line numbers and producer-preserved line counts.
    // canonical 持有精确路径、行号与产出方保留的行数。
    const canonical = buildV1ChangeSetFilePresentation(file);
    // Deleted files use the before snapshot; every surviving file uses its recorded after snapshot.
    // 已删除文件使用变更前快照；仍存在的文件使用记录的变更后快照。
    const format = file.change === "delete" ? file.before_text_format : file.after_text_format;
    // rows translate presentation tags only; omitted lines remain visible diagnostics, not invented source.
    // rows 仅转换展示标签；省略行保留为可见说明，不伪造源码。
    const rows: FileChangeDiffRow[] = canonical.rows.map(row => ({
      kind: row.kind === "added" ? "insert" : row.kind === "deleted" ? "delete" : row.kind === "omitted" ? "omit" : "context",
      lineNumber: row.lineNumber, content: row.content,
    }));
    return buildFilePresentation({ blocks, index, options, added: canonical.additions, removed: canonical.deletions,
      operation: canonical.change, path: canonical.path, previousPath: canonical.oldPath, resourceId: canonical.resourceId,
      operationIds: [], rows, expandable: rows.length > 0, readOnly: false, stableId: canonical.path,
      idOperation: source === "host_change_set" ? null : nativeId,
      textFormat: format ? { bom: format.bom ?? null, encoding: format.encoding ?? null, finalNewline: format.final_newline ?? null, lineEnding: format.line_ending ?? format.preferred_line_ending ?? null, mixedLineEndings: format.mixed_line_endings ?? null } : null,
      displayPathOverride: canonical.change === "rename" ? canonical.displayPath : undefined });
  });
  return { kind: "file_change", source, state, mode: changeset.mode, detailRef, files,
    id: fileChangePresentationId(blocks, files, prefix), operation: summarizeOperation(files),
    interactive: files.length > 1 || files.some(file => file.expandable), hasEffectiveChanges: files.length > 0,
    totalAdded: sumKnown(files.map(file => file.added)), totalRemoved: sumKnown(files.map(file => file.removed)),
    errorDetail: state === "failed" ? errorDetailFromBlocks(blocks) : null };
}

// fileChangeStatusLabel returns the compact fallback status label for a file-change card.
// fileChangeStatusLabel 返回文件变更卡片使用的紧凑兜底状态文案。
export function fileChangeStatusLabel(presentation: FileChangePresentation, operation: FileChangeOperation): string {
  if (presentation.state === "running") {
    return operationLabel(operation, "Writing", "Editing", "Deleting", "Changing");
  }
  if (presentation.hasEffectiveChanges !== null) {
    // execution reports actual execution outcome independently from file changes.
    // execution 独立于文件变更报告真实执行结果。
    const execution = presentation.state === "failed"
      ? "Failed"
      : presentation.state === "stopped"
        ? "Stopped"
        : "Succeeded";
    return `${execution} · ${presentation.hasEffectiveChanges ? "Edited" : "Not edited"}`;
  }
  if (presentation.state === "failed") {
    return operationLabel(operation, "Write failed", "Edit failed", "Delete failed", "Change failed");
  }
  if (presentation.mode === "preview") {
    return operationLabel(operation, "Will write", "Will edit", "Will delete", "Will change");
  }
  return operationLabel(operation, "Written", "Edited", "Deleted", "Changed");
}

// resolveFileChangePresentation recognizes native file tools and compatible completed host change sets.
// resolveFileChangePresentation 识别原生文件工具与兼容的已完成宿主变更集。
export function resolveFileChangePresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  options: ToolPresentationResolveOptions,
  detailRef?: ConversationDetailRef,
): FileChangePresentation | null {
  // Resolve file facts first; attach only the exact producer error code, never infer it from prose.
  // 先解析文件事实；仅附加产出方的精确错误码，不从描述文本推测。
  const presentation = resolveFileChangeContent(blocks, displayState, options, detailRef);
  if (!presentation || displayState !== "failed") return presentation;
  // V1 retains the original envelope even when its changeset shadows legacy host metadata.
  // 即使变更集覆盖旧式宿主元数据，V1 仍保留原始输出信封。
  const event = fixedV1ToolEvent(blocks);
  // Legacy records use the established host payload adapter; V1 uses its authoritative envelope.
  // 旧记录使用既有宿主载荷适配器；V1 使用自身权威信封。
  const payload = event
    ? metadataValue(event.output, ["host_result.payload"])
    : fixedHostPayload(blocks);
  return { ...presentation, errorCode: metadataString(payload, ["details.code"]) };
}

/** Resolves file rows from blocks, state, display options and optional detail identity; returns null for unrelated tools.
 * 根据源块、状态、展示选项与可选详情身份解析文件行；无关工具返回 null。 */
function resolveFileChangeContent(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  options: ToolPresentationResolveOptions,
  detailRef?: ConversationDetailRef,
): FileChangePresentation | null {
  // V1 changesets belong to the event, not the legacy host_result envelope.
  // V1 变更集属于事件本身，而非旧版 host_result 信封。
  const canonicalEvent = fixedV1ToolEvent(blocks);
  if (canonicalEvent?.changeset) {
    return buildPersistentChangeSetPresentation(blocks, displayState, canonicalEvent.changeset, options, detailRef);
  }
  const nativeToolId = exactNativeToolId(blocks);
  if (nativeToolId === "file") {
    // Whole-file writes expose counts without copying incomplete JSON into execution arguments.
    // 整文件写入暴露计数，不把不完整 JSON 复制为执行参数。
    if (displayState === "running" && options.requestPreview?.kind === "file_write") {
      const path = metadataString(options.requestPreview, ["title_fields.path"]);
      const lines = numberValue(metadataValue(options.requestPreview, ["progress_fields.new_line_count"]));
      if (path && lines !== null && Number.isSafeInteger(lines) && lines >= 0) {
        const file = buildFilePresentation({
          blocks, options, path, added: lines, removed: null, index: 0,
          operation: "modify", rows: [], expandable: false, resourceId: null,
          stableId: path, idOperation: "file", operationIds: [], previousPath: null, readOnly: false,
        });
        return {
          ...buildNoEffectiveChangePresentation(blocks, displayState, "native_file_mutation", options, detailRef),
          files: [file], totalAdded: lines, totalRemoved: null, mode: "preview",
        };
      }
    }
    // Read-only actions belong exclusively to FileAccessPresentation and must never fall through to "Not edited".
    // 只读动作仅属于 FileAccessPresentation，绝不能继续落入“未编辑”兜底。
    if (nativeFileAccessAction(blocks)) {
      return null;
    }
    const hostPayload = displayState !== "running" ? lastChangeSetPayload(blocks) : null;
    if (hostPayload) {
      return buildHostChangeSetPresentation(
        blocks,
        displayState,
        hostPayload,
        options,
        "native_file_mutation",
        detailRef,
      );
    }
    const mutationOperation = nativeFileMutationOperation(blocks);
    return mutationOperation
      ? buildNativeVulcanFilePresentation(
        blocks,
        displayState,
        mutationOperation,
        options,
        "native_file_mutation",
        detailRef,
      )
      : buildNoEffectiveChangePresentation(blocks, displayState, "native_file_mutation", options, detailRef);
  }
  if (nativeToolId === "file_edit") {
    const hostPayload = displayState !== "running" ? lastChangeSetPayload(blocks) : null;
    if (hostPayload) {
      return buildHostChangeSetPresentation(
        blocks,
        displayState,
        hostPayload,
        options,
        "native_file_edit",
        detailRef,
      );
    }
    if (displayState === "running") {
      const streamedPreview = nativeEditPreview(options.requestPreview, "file_edit");
      if (streamedPreview) {
        return buildNativeEditPreviewPresentation(blocks, displayState, streamedPreview, options, "native_file_edit", detailRef);
      }
      const argumentPreview = nativeFileEditArgumentsPreview(blocks);
      if (argumentPreview) {
        return buildNativeEditPreviewPresentation(blocks, displayState, argumentPreview, options, "native_file_edit", detailRef);
      }
      return buildNativeVulcanFilePresentation(blocks, displayState, "modify", options, "native_file_edit", detailRef)
        ?? buildNoEffectiveChangePresentation(blocks, displayState, "native_file_edit", options, detailRef);
    }
    return buildNoEffectiveChangePresentation(blocks, displayState, "native_file_edit", options, detailRef);
  }
  if (nativeToolId === "apply_patch") {
    const hostPayload = displayState !== "running" ? lastChangeSetPayload(blocks) : null;
    if (hostPayload) {
      return buildHostChangeSetPresentation(
        blocks,
        displayState,
        hostPayload,
        options,
        "native_apply_patch",
        detailRef,
      );
    }
    if (displayState === "running") {
      // streamedPreview is the newest bounded parser projection during request formation.
      // streamedPreview 是请求构造期间最新的有界解析器投影。
      const streamedPreview = nativeEditPreview(options.requestPreview, "apply_patch");
      if (streamedPreview) {
        return buildNativeEditPreviewPresentation(
          blocks,
          displayState,
          streamedPreview,
          options,
          "native_apply_patch",
          detailRef,
        );
      }
      // argumentPreview is available after arguments assemble but before the tool starts.
      // argumentPreview 在参数组装完成且工具尚未开始时可用。
      const argumentPreview = nativeApplyPatchArgumentsPreview(blocks);
      if (argumentPreview) {
        return buildNativeEditPreviewPresentation(
          blocks,
          displayState,
          argumentPreview,
          options,
          "native_apply_patch",
          detailRef,
        );
      }
    }
    return buildNoEffectiveChangePresentation(blocks, displayState, "native_apply_patch", options, detailRef);
  }
  const hostPayload = displayState !== "running" ? lastChangeSetPayload(blocks) : null;
  if (hostPayload) {
    return buildHostChangeSetPresentation(
      blocks,
      displayState,
      hostPayload,
      options,
      "host_change_set",
      detailRef,
    );
  }
  const nativeOperation = nativeVulcanFileOperation(blocks);
  if (!nativeOperation) {
    return null;
  }
  return buildNativeVulcanFilePresentation(blocks, displayState, nativeOperation, options, "native_vulcan_file", detailRef);
}

// resolveFileAccessPresentation recognizes only read-only actions from the exact native file contract.
// resolveFileAccessPresentation 仅识别精确原生 file 契约中的只读动作。
// Parameters: blocks contain the invocation/result records, displayState is authoritative lifecycle state,
// options owns workspace display normalization, and detailRef locates the complete terminal result.
// 参数：blocks 包含调用与返回记录，displayState 是权威生命周期状态，options 持有工作区展示归一化，
// detailRef 定位完整终态返回。
// Returns: one independent read-only file card, or null for every mutation and non-file tool.
// 返回值：返回一张独立只读文件卡；所有变更动作与非 file 工具返回 null。
export function resolveFileAccessPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  options: ToolPresentationResolveOptions,
  detailRef?: ConversationDetailRef,
): FileAccessPresentation | null {
  if (exactNativeToolId(blocks) !== "file") {
    return null;
  }
  const action = nativeFileAccessAction(blocks);
  return action
    ? buildNativeFileAccessPresentation(blocks, displayState, action, options, detailRef)
    : null;
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

// buildNativeEditPreviewPresentation maps safe request-stream metadata to the shared file card.
// buildNativeEditPreviewPresentation 将安全请求流元数据映射到共享文件卡片。
function buildNativeEditPreviewPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  preview: NativeFileEditPreviewFile[],
  options: ToolPresentationResolveOptions,
  source: "native_file_edit" | "native_apply_patch" = "native_file_edit",
  detailRef?: ConversationDetailRef,
): FileChangePresentation | null {
  // Parser totals remain exact when the bounded body/rows are truncated; argument-only previews lack them.
  // 正文及行预览截断后解析器总数仍精确；仅基于已完成参数的预览不带这些总数。
  const expectedKind = source === "native_apply_patch" ? "apply_patch" : "file_edit";
  const counts = options.requestPreview?.kind === expectedKind
    ? objectValue(options.requestPreview.progress_fields) : null;
  const totalNew = numberValue(counts?.new_line_count);
  const totalOld = numberValue(counts?.old_line_count);
  const files = preview.map((item, index) => {
    const added = preview.length === 1 && totalNew !== null
      ? totalNew : item.rows.filter((row) => row.kind === "insert").length;
    const removed = preview.length === 1 && totalOld !== null
      ? totalOld : item.rows.filter((row) => row.kind === "delete").length;
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
      idOperation: source === "native_apply_patch" ? "apply_patch" : "file_edit",
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
    detailRef,
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(
      blocks,
      files,
      source === "native_apply_patch" ? "native-apply-patch" : "native-file-edit",
    ),
    interactive: false,
    hasEffectiveChanges: null,
    kind: "file_change",
    mode: "preview",
    operation: summarizeOperation(files),
    source,
    state: displayState,
    totalAdded: totalNew ?? sumKnown(files.map((file) => file.added)),
    totalRemoved: totalOld ?? sumKnown(files.map((file) => file.removed)),
  };
}

// buildHostChangeSetPresentation converts one host change_set payload into display rows.
// buildHostChangeSetPresentation 将宿主 change_set 载荷转换为展示行。
function buildHostChangeSetPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  payload: Record<string, unknown>,
  options: ToolPresentationResolveOptions,
  source: "host_change_set" | "native_file_mutation" | "native_file_edit" | "native_apply_patch",
  detailRef?: ConversationDetailRef,
): FileChangePresentation | null {
  const rawFiles = arrayValue(payload.files);
  const correlation = nativeChangeSetCorrelation(payload);
  const files = rawFiles
    .map((file, index) => buildHostChangeSetFile(file, index, blocks, options, correlation, source))
    .filter((file): file is FileChangeFilePresentation => Boolean(file));
  if (!files.length) {
    return buildNoEffectiveChangePresentation(blocks, displayState, source, options, detailRef);
  }
  const mode = changeSetMode(payload.mode);
  const operation = summarizeOperation(files);
  return {
    detailRef,
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(
      blocks,
      files,
      source === "native_file_edit"
        ? "native-file-edit"
        : source === "native_apply_patch"
          ? "native-apply-patch"
          : source === "native_file_mutation"
            ? "native-file-mutation"
            : "change-set",
    ),
    interactive: displayState !== "running" && (files.length > 1 || files.some((file) => file.expandable)),
    hasEffectiveChanges: true,
    kind: "file_change",
    mode,
    operation,
    source,
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
  source: "native_vulcan_file" | "native_file_edit" | "native_file_mutation",
  detailRef?: ConversationDetailRef,
): FileChangePresentation | null {
  const paths = nativeFilePaths(blocks);
  // A file card requires one real path; request formation must not fabricate a generic `File` identity.
  // 文件卡片必须具备真实路径；请求构造阶段不得伪造通用 `File` 身份。
  if (!paths.length) {
    return null;
  }
  const displayPaths = paths;
  const files = displayPaths.map((path, index) => {
    const rows = displayState === "failed"
      ? errorRowsFromBlocks(blocks)
      : source === "native_file_mutation" && displayState === "running"
        ? nativeFileMutationArgumentRows(blocks, path)
        : [];
    return buildFilePresentation({
      added: rows.some((row) => row.kind === "insert")
        ? rows.filter((row) => row.kind === "insert").length
        : null,
      blocks,
      expandable: rows.length > 0,
      index,
      operation,
      path,
      removed: null,
      rows,
      resourceId: null,
      stableId: source === "native_vulcan_file" ? null : path,
      idOperation: source === "native_file_edit"
        ? "file_edit"
        : source === "native_file_mutation"
          ? "file"
          : null,
      operationIds: [],
      previousPath: null,
      readOnly: false,
      options,
    });
  });
  return {
    detailRef,
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    files,
    id: fileChangePresentationId(
      blocks,
      files,
      source === "native_file_edit"
        ? "native-file-edit"
        : source === "native_file_mutation"
          ? "native-file-mutation"
          : `native-${operation}`,
    ),
    interactive: displayState === "failed"
      || files.some((file) => file.expandable)
      || (displayState !== "running" && Boolean(detailRef)),
    hasEffectiveChanges: displayState === "running" ? null : false,
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
  detailRef?: ConversationDetailRef,
): FileAccessPresentation | null {
  const targets = nativeFileAccessTargets(blocks);
  const output = nativeFileAccessOutput(blocks);
  // V1 file inspections share their sole model body instead of persisting a second host object.
  // V1 文件检查共用唯一模型正文，不再持久化第二个宿主对象。
  const modelResult = v1ModelResult(fixedV1ToolEvent(blocks)?.output);
  const markdownBody = displayState !== "running" && typeof modelResult === "string"
    ? modelResult : output && isNumberedFileRead(output.metadata.schema) && output.body ? output.body : undefined;
  if (!targets.length) {
    return null;
  }
  const items = targets.map((target, index) => {
    const rows = displayState === "failed"
      ? errorRowsFromBlocks(blocks)
      : nativeFileResultRowsFromOutput(output, index, accessAction);
    const result = nativeFileAccessResult(output?.metadata ?? null, index);
    const file = buildFilePresentation({
      added: null,
      blocks,
      codePreview: accessAction === "read",
      expandable: rows.length > 0,
      index,
      operation: "mixed",
      path: target.path,
      removed: null,
      rows,
      resourceId: stringValue(result?.resource_id),
      operationIds: [],
      previousPath: null,
      readOnly: true,
      textFormat: nativeFileTextFormat(result),
      options,
    });
    return fileAccessItemFromFile(
      file,
      nativeFileAccessResultLineRanges(result, target.lineRanges),
    );
  });
  return {
    action: accessAction,
    markdownBody,
    emptyFile: accessAction === "read" && output?.metadata.total_line_count === 0,
    errorDetail: displayState === "failed" ? errorDetailFromBlocks(blocks) : null,
    detailRef,
    id: fileChangePresentationId(blocks, items, `native-file-${accessAction}`),
    interactive: displayState !== "running" && (markdownBody !== undefined || items.some((item) => item.expandable)),
    items,
    kind: fileAccessPresentationKind(accessAction),
    state: displayState,
  };
}

// fileAccessPresentationKind maps the exact native action to an independent card discriminator.
// fileAccessPresentationKind 将精确原生动作映射为独立卡片判别字段。
function fileAccessPresentationKind(action: FileAccessAction): FileAccessPresentationKind {
  if (action === "read") return "file_read";
  if (action === "list") return "file_list";
  if (action === "search") return "file_search";
  return "file_stat";
}

// fileAccessItemFromFile removes mutation-only fields from one shared low-level file row builder result.
// fileAccessItemFromFile 从共享底层文件行构建结果中移除仅属于变更的字段。
function fileAccessItemFromFile(
  file: FileChangeFilePresentation,
  lineRanges: FileAccessLineRangePresentation[],
): FileAccessItemPresentation {
  return {
    diffFile: file.diffFile,
    displayPath: file.displayPath,
    expandable: file.expandable,
    icon: file.icon,
    id: file.id,
    lineRanges,
    path: file.path,
    pathKind: file.pathKind,
    resourceId: file.resourceId,
    rows: file.rows,
    textFormat: file.textFormat,
  };
}

// buildNoEffectiveChangePresentation creates a terminal edit card without reusing expected preview files.
// buildNoEffectiveChangePresentation 创建不复用预期预览文件的终态未编辑卡片。
function buildNoEffectiveChangePresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  source: "host_change_set" | "native_file_mutation" | "native_file_edit" | "native_apply_patch",
  options: ToolPresentationResolveOptions,
  detailRef?: ConversationDetailRef,
): FileChangePresentation {
  // Request targets remain visible after failure, but request previews must never become applied diffs.
  // 失败后保留请求目标，但请求预览绝不能变成已应用差异。
  const targets = displayState !== "failed" ? [] : source === "native_apply_patch"
    ? (nativeApplyPatchArgumentsPreview(blocks) ?? []).map(file => ({ path: file.path, operation: file.operation }))
    : nativeFilePaths(blocks).map(path => ({ path, operation: nativeFileMutationOperation(blocks) ?? "modify" as const }));
  // Empty rows and unknown counts preserve the distinction between attempted and committed changes.
  // 空差异行与未知计数保留尝试编辑和已提交变更之间的区别。
  const files = targets.map((target, index) => buildFilePresentation({
    blocks, index, options, path: target.path, operation: target.operation,
    added: null, removed: null, rows: [], expandable: false, readOnly: false,
    resourceId: null, operationIds: [], previousPath: null, stableId: target.path,
    idOperation: exactNativeToolId(blocks),
  }));
  // Full diagnostics are expandable even when no host changeset was produced.
  // 即使没有产出宿主变更集，完整诊断仍可展开。
  const errorDetail = displayState === "failed" ? errorDetailFromBlocks(blocks) : null;
  return {
    detailRef,
    errorDetail,
    files,
    hasEffectiveChanges: displayState === "running" ? null : false,
    id: fileChangePresentationId(blocks, files, `no-effective-change:${source}`),
    interactive: Boolean(errorDetail),
    kind: "file_change",
    mode: "unknown",
    operation: "modify",
    source,
    state: displayState,
    totalAdded: 0,
    totalRemoved: 0,
  };
}

// buildHostChangeSetFile converts one change_set file entry into one display file.
// buildHostChangeSetFile 将单个 change_set 文件条目转换为展示文件。
function buildHostChangeSetFile(
  value: unknown,
  index: number,
  blocks: ToolExecutionConversationBlock[],
  options: ToolPresentationResolveOptions,
  correlation: NativeChangeSetCorrelation,
  source: "host_change_set" | "native_file_mutation" | "native_file_edit" | "native_apply_patch",
): FileChangeFilePresentation | null {
  const file = objectValue(value);
  if (!file) {
    return null;
  }
  const operation = changeOperation(file.change);
  const previousPath = stringValue(file.old_path ?? file.oldPath);
  const path = stringValue(file.new_path ?? file.newPath ?? file.path) ?? previousPath ?? `file-${index + 1}`;
  const resourceId = stringValue(file.resource_id ?? file.resourceId);
  const operationIds = (resourceId ? correlation.operationIdsByResource.get(resourceId) : null)
    ?? correlation.operationIdsByIndex.get(index)
    ?? [];
  const textFormat = (resourceId ? correlation.textFormatsByResource.get(resourceId) : null)
    ?? correlation.textFormatsByIndex.get(index)
    ?? nativeFileTextFormat(file);
  const requestPath = (resourceId ? correlation.requestPathsByResource.get(resourceId) : null)
    ?? correlation.requestPathsByIndex.get(index)
    ?? null;
  const stableId = source !== "host_change_set"
    ? requestPath
      ?? nativeRequestPathForHostPath(blocks, previousPath ?? path)
      ?? previousPath
      ?? path
    : null;
  const idOperation = source === "native_file_edit"
    ? "file_edit"
    : source === "native_apply_patch"
      ? "apply_patch"
      : source === "native_file_mutation"
        ? "file"
        : null;
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
      textFormat,
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
      textFormat,
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
      textFormat,
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
    textFormat,
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
  // Whether read-only rows contain source code that belongs in the line-number gutter.
  // 只读行是否为应进入代码行号槽的源代码内容。
  codePreview?: boolean;
  // Optional decoded text metadata reported by the native file runtime.
  // 原生文件运行时报告的可选已解码文本元数据。
  textFormat?: FileTextFormatPresentation | null;
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
  const diffFile = !args.readOnly || args.codePreview
    ? fileChangeDiffFile({
      added: args.readOnly ? 0 : args.added,
      index: args.index,
      operation: args.operation,
      path: displayPath.path,
      removed: args.readOnly ? 0 : args.removed,
      rows: args.rows,
    })
    : null;
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
    textFormat: args.textFormat ?? null,
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

// nativeFileMutationOperation maps one exact file action to its pre-terminal visual operation.
// nativeFileMutationOperation 将精确的 file 动作映射为终态前的视觉操作。
function nativeFileMutationOperation(
  blocks: ToolExecutionConversationBlock[],
): Exclude<FileChangeOperation, "mixed"> | null {
  for (const block of blocks) {
    const action = stringValue(objectValue(argumentsFromMetadata(block.rawMetadata))?.action);
    if (action === "write") return "modify";
    if (action === "delete") return "delete";
    if (action === "move") return "rename";
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

// NativeFileAccessTarget keeps one request target aligned with one result index.
// NativeFileAccessTarget 让单个请求目标与单个结果索引保持对齐。
interface NativeFileAccessTarget {
  // Original path supplied by the model.
  // 模型提供的原始路径。
  path: string;
  // Structured line ranges declared by this exact target.
  // 当前精确目标声明的结构化行范围。
  lineRanges: FileAccessLineRangePresentation[];
}

// nativeFileAccessTargets preserves target order without converting ranges into textual labels.
// nativeFileAccessTargets 保留目标顺序，且不把读取范围转换为文字标签。
function nativeFileAccessTargets(
  blocks: ToolExecutionConversationBlock[],
): NativeFileAccessTarget[] {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const args = objectValue(argumentsFromMetadata(blocks[index]?.rawMetadata));
    if (!args) {
      continue;
    }
    const path = stringValue(args.path);
    if (path) {
      return [{ lineRanges: nativeFileAccessArgumentRanges(args), path }];
    }
    const targets = arrayValue(args.targets)
      .map((value) => objectValue(value))
      .map<NativeFileAccessTarget | null>((target) => {
        const path = stringValue(target?.path);
        if (!target || !path) {
          return null;
        }
        return { lineRanges: nativeFileAccessArgumentRanges(target), path };
      })
      .filter((target): target is NativeFileAccessTarget => Boolean(target));
    if (targets.length) {
      return targets;
    }
    const paths: string[] = [];
    collectArgumentPaths(args, paths);
    if (paths.length) {
      return paths.map((path) => ({ lineRanges: [], path }));
    }
  }
  return [];
}

// nativeFileAccessArgumentRanges reads only the native v2 or declared historical target range fields.
// nativeFileAccessArgumentRanges 仅读取原生 v2 或已声明历史目标中的范围字段。
function nativeFileAccessArgumentRanges(
  value: Record<string, unknown>,
): FileAccessLineRangePresentation[] {
  // directStart and directEnd belong to the native v2 single-read contract.
  // directStart 与 directEnd 属于原生 v2 单文件读取契约。
  const directStart = numberValue(value.start_line);
  const directEnd = numberValue(value.end_line);
  if (directStart !== null && directStart > 0) {
    return [{
      endLine: directEnd !== null && directEnd >= directStart ? directEnd : null,
      startLine: directStart,
    }];
  }
  return arrayValue(value.segments)
    .map((segment) => objectValue(segment))
    .map<FileAccessLineRangePresentation | null>((segment) => {
      // start and count use the exact historical target segment fields.
      // start 与 count 使用精确的历史目标分段字段。
      const start = numberValue(segment?.start_line);
      const count = numberValue(segment?.line_count);
      return start !== null && start > 0 && count !== null && count > 0
        ? { endLine: start + count - 1, startLine: start }
        : null;
    })
    .filter((range): range is FileAccessLineRangePresentation => Boolean(range));
}

// collectArgumentPaths appends file paths from single-file and batch argument shapes.
// collectArgumentPaths 从单文件与批量参数形态中追加文件路径。
function collectArgumentPaths(value: unknown, paths: string[]): void {
  const args = objectValue(value);
  if (!args) {
    return;
  }
  const path = stringValue(args.path);
  if (path) {
    paths.push(path);
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

// nativeFileEditArgumentsPreview rebuilds the bounded exact-replacement preview retained in historical arguments.
// nativeFileEditArgumentsPreview 从历史参数中保留的有界精确替换内容重建预览。
function nativeFileEditArgumentsPreview(
  blocks: ToolExecutionConversationBlock[],
): NativeFileEditPreviewFile[] | null {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const preview = nativeFileEditArgumentsPreviewFromValue(
      objectValue(argumentsFromMetadata(blocks[index]?.rawMetadata)),
    );
    if (preview) {
      return preview;
    }
  }
  return null;
}

// nativeFileEditArgumentsPreviewFromValue converts the exact public file_edit schema into diff rows.
// nativeFileEditArgumentsPreviewFromValue 将公开 file_edit Schema 的精确参数转换为差异行。
function nativeFileEditArgumentsPreviewFromValue(
  argumentsValue: Record<string, unknown> | null,
): NativeFileEditPreviewFile[] | null {
  const path = stringValue(argumentsValue?.path);
  const oldString = textValue(argumentsValue?.old_string);
  const newString = textValue(argumentsValue?.new_string);
  if (!path || oldString === null || newString === null) {
    return null;
  }
  const rows = [
    ...deleteRows({ content: oldString }),
    ...insertRows(splitTextLines(newString)),
  ];
  return [{ operation: "modify", path, rows }];
}

// nativeApplyPatchArgumentsPreview rebuilds a bounded patch preview when only historical arguments survived.
// nativeApplyPatchArgumentsPreview 在仅剩历史参数时重建有界的 patch 预览。
function nativeApplyPatchArgumentsPreview(
  blocks: ToolExecutionConversationBlock[],
): NativeFileEditPreviewFile[] | null {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const argumentsValue = objectValue(argumentsFromMetadata(blocks[index]?.rawMetadata));
    const patch = textValue(argumentsValue?.patch);
    if (patch === null) {
      continue;
    }
    const affectedResources = applyPatchArgumentPaths(patch);
    return nativeEditPreview({
      affected_resources: affectedResources,
      body_preview: patch,
      kind: "apply_patch",
    }, "apply_patch");
  }
  return null;
}

// applyPatchArgumentPaths extracts ordered operation paths from one native Apply Patch request.
// applyPatchArgumentPaths 从一份原生 Apply Patch 请求中提取有序操作路径。
function applyPatchArgumentPaths(patch: string): string[] {
  const prefixes = ["*** Add File: ", "*** Delete File: ", "*** Update File: "];
  const paths: string[] = [];
  for (const line of splitTextLines(patch)) {
    const prefix = prefixes.find((candidate) => line.startsWith(candidate));
    const path = prefix ? line.slice(prefix.length).trim() : "";
    if (path) {
      paths.push(path);
    }
  }
  return uniqueStrings(paths);
}

// nativeFileMutationArgumentRows exposes the retained new body of a historical whole-file write.
// nativeFileMutationArgumentRows 展示历史整文件写入中仍保留的新正文。
function nativeFileMutationArgumentRows(
  blocks: ToolExecutionConversationBlock[],
  targetPath: string,
): FileChangeDiffRow[] {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const argumentsValue = objectValue(argumentsFromMetadata(blocks[index]?.rawMetadata));
    const path = stringValue(argumentsValue?.path);
    const content = textValue(argumentsValue?.content);
    if (stringValue(argumentsValue?.action) !== "write"
      || !path
      || normalizePath(path) !== normalizePath(targetPath)
      || content === null) {
      continue;
    }
    return insertRows(splitTextLines(content));
  }
  return [];
}

// nativeEditPreview parses only host-owned file_edit or Apply Patch preview shapes and never arbitrary prose.
// nativeEditPreview 仅解析宿主自有的 file_edit 或 Apply Patch 预览结构，不解析任意正文。
function nativeEditPreview(
  value: Record<string, unknown> | null | undefined,
  expectedKind: "file_edit" | "apply_patch",
): NativeFileEditPreviewFile[] | null {
  if (stringValue(value?.kind) !== expectedKind) {
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
  for (const item of structuredPreviewTruncated ? [] : structuredLines) {
    const line = objectValue(item);
    const resourceIndex = numberValue(line?.resource_index);
    const operationIndex = numberValue(line?.operation_index);
    const fileIndex = resourceIndex ?? operationIndex ?? -1;
    const side = stringValue(line?.side);
    const content = textValue(line?.content);
    const lineIndex = numberValue(line?.line_index);
    const file = files[fileIndex];
    if (!file || content === null || (side !== "old" && side !== "new" && side !== "context")) {
      continue;
    }
    structuredFileIndexes.add(fileIndex);
    file.rows.push({
      content,
      kind: side === "old" ? "delete" : side === "new" ? "insert" : "context",
      lineNumber: lineIndex === null ? null : lineIndex + 1,
    });
  }
  const body = textValue(value?.body_preview);
  if (!body) {
    return files;
  }
  let currentIndex: number | null = null;
  const nextFallbackLineNumbers = files.map(() => 1);
  for (const line of splitTextLines(body)) {
    const header = nativeEditPreviewHeader(line, paths, expectedKind);
    if (header) {
      currentIndex = paths.indexOf(header.path);
      const file = files[currentIndex];
      if (file) {
        file.operation = header.operation;
      }
      continue;
    }
    if (expectedKind === "apply_patch" && line.startsWith("*** Move to: ") && currentIndex !== null) {
      const file = files[currentIndex];
      if (file) {
        file.operation = "rename";
      }
      continue;
    }
    if (currentIndex !== null && structuredFileIndexes.has(currentIndex)) {
      continue;
    }
    const side = editPreviewBodySide(line, expectedKind);
    if (currentIndex === null || !side) {
      continue;
    }
    const file = files[currentIndex];
    if (!file) {
      continue;
    }
    const fallbackLineNumber = nextFallbackLineNumbers[currentIndex] ?? 1;
    nextFallbackLineNumbers[currentIndex] = fallbackLineNumber + 1;
    file.rows.push({
      content: side.content,
      kind: side.side === "old" ? "delete" : side.side === "new" ? "insert" : "context",
      lineNumber: fallbackLineNumber,
    });
  }
  return files;
}

// nativeEditPreviewHeader recognizes exact file_edit and Apply Patch operation headers.
// nativeEditPreviewHeader 识别精确的 file_edit 与 Apply Patch 操作头行。
function nativeEditPreviewHeader(
  line: string,
  paths: string[],
  expectedKind: "file_edit" | "apply_patch",
): { operation: Exclude<FileChangeOperation, "mixed">; path: string } | null {
  if (expectedKind === "apply_patch") {
    const prefixes = [
      ["*** Add File: ", "create"],
      ["*** Delete File: ", "delete"],
      ["*** Update File: ", "modify"],
    ] as const;
    for (const [prefix, operation] of prefixes) {
      const path = line.startsWith(prefix) ? line.slice(prefix.length) : null;
      if (path && paths.includes(path)) {
        return { operation, path };
      }
    }
    return null;
  }
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

// editPreviewBodySide converts one exact preview text line into a normalized diff side.
// editPreviewBodySide 将一条精确预览文本行转换为规范化 Diff 侧。
function editPreviewBodySide(
  line: string,
  expectedKind: "file_edit" | "apply_patch",
): { content: string; side: "old" | "new" | "context" } | null {
  if (expectedKind === "file_edit") {
    if (line.startsWith("- ")) return { content: line.slice(2), side: "old" };
    if (line.startsWith("+ ")) return { content: line.slice(2), side: "new" };
    return null;
  }
  if (line.startsWith("-") && !line.startsWith("*** ")) {
    return { content: line.slice(1), side: "old" };
  }
  if (line.startsWith("+")) return { content: line.slice(1), side: "new" };
  if (line.startsWith(" ")) return { content: line.slice(1), side: "context" };
  return null;
}

/** Tests the explicit numbered Markdown protocol versions; accepts a schema tag and returns membership.
 * 检查明确的带编号 Markdown 协议版本；接受 schema 标记并返回是否属于该协议。
 */
function isNumberedFileRead(schema: unknown): boolean {
  return schema === "vulcan.file_read.v2" || schema === "vulcan.file_read.v3";
}

/** Tests all supported Markdown read versions, including historical unnumbered v1.
 * 检查所有受支持的 Markdown 读取版本，包括历史无编号 v1。
 */
function isMarkdownFileRead(schema: unknown): boolean {
  return schema === "vulcan.file_read.v1" || isNumberedFileRead(schema);
}

/** Validates content-free read coordinates from metadata; returns null for invalid mappings.
 * 校验元数据中不含正文的读取坐标；映射无效时返回 null。
 */
function nativeMarkdownReadRange(result: Record<string, unknown>): {
  sourceStart: number;
  outputStart: number;
  lineCount: number;
} | null {
  // V3 separates rendering coordinates; v1/v2 history keeps its original mapping without guessing.
  // v3 分离展示坐标；v1/v2 历史按原始映射读取，不猜测字段。
  const range = objectValue(result.range);
  const sourceStart = numberValue(range?.source_start_line);
  const outputStart = numberValue(result.schema === "vulcan.file_read.v3"
    ? objectValue(result.render)?.output_start_line : range?.output_start_line);
  const lineCount = numberValue(range?.line_count);
  if (sourceStart === null || outputStart === null || lineCount === null
    || !Number.isSafeInteger(sourceStart) || sourceStart < 1
    || !Number.isSafeInteger(outputStart) || outputStart < 1
    || !Number.isSafeInteger(lineCount) || lineCount < 0
    || !Number.isSafeInteger(sourceStart + lineCount)) {
    return null;
  }
  return { sourceStart, outputStart, lineCount };
}

/** Projects source rows from the sole Markdown body using validated numeric coordinates.
 * 使用已校验的数值坐标，从唯一 Markdown 正文投影源文件行。
 * Empty bodies await detail hydration; invalid mappings return an explicit error row.
 * 空正文等待详情加载；无效映射返回明确错误行。
 */
function nativeMarkdownReadRows(result: Record<string, unknown>, body: string): FileChangeDiffRow[] {
  if (!body) {
    return [];
  }
  // The producer normalizes presentation separators to LF; source format remains in metadata.
  // 生产端将展示分隔符归一化为 LF；源文件格式保留在元数据中。
  const lines = body.split("\n");
  const range = nativeMarkdownReadRange(result);
  if (!range || range.outputStart - 1 + range.lineCount > lines.length) {
    return [{ content: "Invalid file read line mapping.", kind: "error", lineNumber: null }];
  }
  return lines.slice(range.outputStart - 1, range.outputStart - 1 + range.lineCount)
    .slice(0, FILE_ACCESS_PREVIEW_ROW_LIMIT).map((content, index) => {
      // v2/v3 own an exact positional prefix; v1 source text must remain untouched.
      // v2/v3 定义精确的位置前缀；v1 源文件正文必须保持原样。
      const lineNumber = range.sourceStart + index;
      const prefix = `L${lineNumber}: `;
      if (isNumberedFileRead(result.schema) && !content.startsWith(prefix)) {
        return { content: "Invalid file read line prefix.", kind: "error", lineNumber: null };
      }
      return {
        content: isNumberedFileRead(result.schema) ? content.slice(prefix.length) : content,
        kind: "context",
        lineNumber,
      };
    });
}

/** Returns visible rows for one result; output pairs the body and metadata of that result.
 * 返回单个结果的可见行；output 关联该结果的正文与元数据。
 */
function nativeFileResultRowsFromOutput(
  output: NativeFileAccessOutput | null,
  targetIndex: number,
  action: FileAccessAction,
): FileChangeDiffRow[] {
  const result = nativeFileAccessResult(output?.metadata ?? null, targetIndex);
  if (!result || !output) {
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
    if (isMarkdownFileRead(result.schema)) {
      return nativeMarkdownReadRows(result, output.body);
    }
    const legacyLines = arrayValue(result.lines);
    if (legacyLines.length) {
      return legacyLines.slice(0, FILE_ACCESS_PREVIEW_ROW_LIMIT).map((value) => {
        const line = objectValue(value);
        return {
          content: textValue(line?.content) ?? "",
          kind: "context" as const,
          lineNumber: numberValue(line?.line_number ?? line?.line),
        };
      });
    }
    const content = textValue(result.content);
    const startLine = numberValue(result.start_line) ?? 1;
    return splitTextLines(content ?? "").slice(0, FILE_ACCESS_PREVIEW_ROW_LIMIT).map((line, index) => ({
      content: line,
      kind: "context" as const,
      lineNumber: startLine + index,
    }));
  }
  if (action === "list") {
    return arrayValue(result.entries).slice(0, FILE_ACCESS_PREVIEW_ROW_LIMIT).map((value) => {
      const entry = objectValue(value);
      const path = textValue(entry?.path) ?? "";
      const kind = textValue(entry?.kind) ?? "entry";
      const size = numberValue(entry?.size);
      return {
        content: `[${kind}] ${path}${size === null ? "" : ` (${size} B)`}`,
        kind: "context" as const,
        lineNumber: null,
      };
    });
  }
  if (action === "search") {
    return arrayValue(result.matches).slice(0, FILE_ACCESS_PREVIEW_ROW_LIMIT).map((value) => {
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

// nativeFileAccessResult selects one authoritative result from native v2 or legacy batch output.
// nativeFileAccessResult 从原生 v2 或旧批量输出中选择一个权威结果。
function nativeFileAccessResult(
  output: Record<string, unknown> | null,
  targetIndex: number,
): Record<string, unknown> | null {
  const legacyResults = arrayValue(output?.results);
  if (legacyResults.length) {
    return objectValue(legacyResults[targetIndex]);
  }
  return targetIndex === 0 ? output : null;
}

// nativeFileTextFormat maps strict runtime text facts without inferring missing values.
// nativeFileTextFormat 映射严格运行时文本事实，不推断缺失值。
function nativeFileTextFormat(
  result: Record<string, unknown> | null,
): FileTextFormatPresentation | null {
  if (!result) {
    return null;
  }
  const encoding = stringValue(result.encoding);
  const lineEnding = stringValue(result.line_ending)
    ?? stringValue(result.preferred_line_ending);
  const bom = booleanValue(result.bom);
  const mixedLineEndings = booleanValue(result.mixed_line_endings);
  const finalNewline = booleanValue(result.final_newline);
  if (encoding === null && lineEnding === null && bom === null
    && mixedLineEndings === null && finalNewline === null) {
    return null;
  }
  return {
    bom,
    encoding,
    finalNewline,
    lineEnding,
    mixedLineEndings,
  };
}

/** Keeps one terminal block's metadata paired with its authoritative model body.
 * 将同一终态块的元数据与其权威模型正文关联。
 */
interface NativeFileAccessOutput {
  /** Presentation metadata; historical versions may also contain source text.
   * 展示元数据；历史版本也可能包含源文件正文。
   */
  metadata: Record<string, unknown>;
  /** Sole Markdown body in the native read contract.
   * 原生读取契约中的唯一 Markdown 正文。
   */
  body: string;
}

/** Selects a host payload with its body, or parses an explicit historical JSON result.
 * 选择同一宿主载荷与正文，或解析明确的历史 JSON 结果。
 * blocks are ordered tool events; returns null when no readable file result exists.
 * blocks 为有序工具事件；不存在可读取文件结果时返回 null。
 */
function nativeFileAccessOutput(
  blocks: ToolExecutionConversationBlock[],
): NativeFileAccessOutput | null {
  // Canonical events own both the model body and host metadata; legacy copies are not authoritative.
  // 规范事件同时持有模型正文与宿主元数据；旧式副本不是权威来源。
  const event = fixedV1ToolEvent(blocks);
  if (event) {
    const host = objectValue(event.output?.host_result);
    const metadata = host?.kind === "file_access" ? objectValue(host.payload) : null;
    const body = v1ModelResult(event.output);
    return metadata ? { metadata, body: typeof body === "string" ? body : "" } : null;
  }
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const block = blocks[index];
    const hostResult = objectValue(metadataValue(block?.rawMetadata, ["host_result", "hostResult"]));
    if (stringValue(hostResult?.kind) === "file_access") {
      const payload = objectValue(hostResult?.payload);
      if (payload) {
        return { metadata: payload, body: block.body };
      }
    }
  }
  // Legacy JSON stores its rows in the parsed object and does not use the Markdown body.
  // 旧版 JSON 将展示行存放在解析对象中，不使用 Markdown 正文。
  const metadata = visibleJsonOutput(blocks);
  return metadata ? { metadata, body: "" } : null;
}

// hydrateFileAccessPresentation restores complete read-only rows from one hidden terminal detail.
// hydrateFileAccessPresentation 从单条隐藏终态详情中恢复完整只读返回行。
export function hydrateFileAccessPresentation(
  presentation: FileAccessPresentation,
  payload: ToolDetailPayload | null,
): FileAccessPresentation {
  if (!payload) {
    return presentation;
  }
  // Delayed inspection details carry the exact same body as immediate model results.
  // 延迟检查详情携带与即时模型结果完全相同的正文。
  const modelResult = v1ModelResult({ model_output: payload.body });
  if (presentation.action !== "read" && typeof modelResult === "string") {
    return { ...presentation, markdownBody: modelResult, interactive: presentation.state !== "running" };
  }
  // output prefers the hidden authoritative host_result and only then accepts a strict legacy JSON body.
  // output 优先使用隐藏的权威 host_result，仅在其缺失时接受严格旧版 JSON 正文。
  const output = fileAccessOutputFromDetail(payload);
  if (!output) {
    return presentation;
  }
  const items = presentation.items.map((item, index) => {
    const rows = nativeFileResultRowsFromOutput(output, index, presentation.action);
    // A zero-line versioned read is authoritative and must clear an earlier requested range.
    // 带版本的零行读取是权威结果，必须清除先前的请求范围。
    const result = nativeFileAccessResult(output.metadata, index);
    if (!rows.length && !isMarkdownFileRead(result?.schema)) {
      return item;
    }
    const diffFile = presentation.action === "read"
      ? fileChangeDiffFile({
        added: 0,
        index,
        operation: "mixed",
        path: item.displayPath,
        removed: 0,
        rows,
      })
      : null;
    return {
      ...item,
      diffFile,
      expandable: rows.length > 0,
      lineRanges: nativeFileAccessResultLineRanges(
        result,
        item.lineRanges,
      ),
      textFormat: nativeFileTextFormat(result),
      rows,
    };
  });
  return {
    ...presentation,
    interactive: presentation.state !== "running" && items.some((item) => item.expandable),
    items,
    // Versioned numbered reads retain their original Markdown, including empty-range explanations.
    // 带编号的版本化读取保留原始 Markdown，也包括空范围说明。
    ...(isNumberedFileRead(output.metadata.schema) ? {
      markdownBody: output.body, interactive: presentation.state !== "running",
      emptyFile: output.metadata.total_line_count === 0,
    } : {}),
  };
}

// fileAccessOutputFromDetail returns the exact hidden file_access payload before considering body text.
// fileAccessOutputFromDetail 在考虑正文文本前返回精确的隐藏 file_access 载荷。
function fileAccessOutputFromDetail(
  payload: ToolDetailPayload,
): NativeFileAccessOutput | null {
  // hostResult is the terminal metadata boundary stored with the hidden detail record.
  // hostResult 是随隐藏详情记录保存的终态 metadata 边界。
  const hostResult = objectValue(metadataValue(payload.metadata, ["host_result", "hostResult"]));
  if (stringValue(hostResult?.kind) === "file_access") {
    // hostPayload is the strongly typed native file result projection.
    // hostPayload 是强类型原生文件结果投影。
    const hostPayload = objectValue(hostResult?.payload);
    if (hostPayload) {
      return { metadata: hostPayload, body: payload.body };
    }
  }
  // Historical detail records expose their source rows through JSON only.
  // 历史详情记录仅通过 JSON 暴露源文件行。
  const metadata = parseFileToolOutput(payload.body);
  return metadata ? { metadata, body: payload.body } : null;
}

// nativeFileAccessResultLineRanges prefers the actual returned range over the request range.
// nativeFileAccessResultLineRanges 优先使用实际返回范围，其次使用请求范围。
function nativeFileAccessResultLineRanges(
  result: Record<string, unknown> | null,
  requestRanges: FileAccessLineRangePresentation[],
): FileAccessLineRangePresentation[] {
  if (!result) {
    return requestRanges;
  }
  if (isMarkdownFileRead(result.schema)) {
    // The versioned mapping owns the returned range, including an explicitly empty read.
    // 带版本的映射拥有返回范围，也包含明确的空读取。
    const range = nativeMarkdownReadRange(result);
    return range && range.lineCount > 0
      ? [{ startLine: range.sourceStart, endLine: range.sourceStart + range.lineCount - 1 }]
      : [];
  }
  // actualRange is the explicit native v2 returned-range object.
  // actualRange 是原生 v2 明确返回的范围对象。
  const actualRange = objectValue(result.actual_range);
  // startLine and endLine are duplicated at the v2 payload root by the native contract.
  // startLine 与 endLine 由原生契约同时保存在 v2 载荷根级。
  const startLine = numberValue(actualRange?.start_line) ?? numberValue(result.start_line);
  const endLine = numberValue(actualRange?.end_line) ?? numberValue(result.end_line);
  return startLine !== null && startLine > 0
    ? [{
        endLine: endLine !== null && endLine >= startLine ? endLine : null,
        startLine,
      }]
    : requestRanges;
}

// hydrateFileChangePresentation keeps authoritative mutation change sets unchanged.
// hydrateFileChangePresentation 保持权威文件变更 change-set 不被详情载荷改写。
export function hydrateFileChangePresentation(
  presentation: FileChangePresentation,
  _payload: ToolDetailPayload | null,
): FileChangePresentation {
  return presentation;
}

// visibleJsonOutput finds the bounded native file result body without parsing arbitrary prose.
// visibleJsonOutput 仅解析有界的原生 file 结果正文，不尝试解析任意自然语言。
function visibleJsonOutput(blocks: ToolExecutionConversationBlock[]): Record<string, unknown> | null {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const output = parseFileToolOutput(blocks[index]?.body);
    if (output) {
      return output;
    }
  }
  return null;
}

// parseFileToolOutput accepts only the structured result envelope of the native file tool.
// parseFileToolOutput 仅接受原生 file 工具的结构化结果封装。
function parseFileToolOutput(body: string | null | undefined): Record<string, unknown> | null {
  const trimmed = body?.trim();
  if (!trimmed) {
    return null;
  }
  try {
    const object = objectValue(JSON.parse(trimmed) as unknown);
    if (!object) {
      return null;
    }
    if (Array.isArray(object.results)) {
      return object;
    }
    if (booleanValue(object.ok) !== null && Array.isArray(object.operations)) {
      return object;
    }
    return stringValue(object.schema) === "vulcan.file_access.v2"
      && ["read", "list", "stat"].includes(stringValue(object.action) ?? "")
      ? object
      : null;
  } catch {
    return null;
  }
}

// NativeChangeSetCorrelation maps native operation identity and request paths onto canonical files.
// NativeChangeSetCorrelation 将原生操作身份与请求路径映射到 canonical 文件。
interface NativeChangeSetCorrelation {
  // Ordered operation ids keyed by zero-based operation index.
  // 按零基准操作索引分组的有序操作标识。
  operationIdsByIndex: Map<number, string[]>;
  // Ordered operation ids keyed by canonical resource identity.
  // 按 canonical 资源身份分组的有序操作标识。
  operationIdsByResource: Map<string, string[]>;
  // Original request paths keyed by zero-based operation index.
  // 按零基准操作索引分组的原始请求路径。
  requestPathsByIndex: Map<number, string>;
  // Original request paths keyed by canonical resource identity.
  // 按 canonical 资源身份分组的原始请求路径。
  requestPathsByResource: Map<string, string>;
  // Final text format keyed by zero-based resource index.
  // 按零基准资源索引分组的最终文本格式。
  textFormatsByIndex: Map<number, FileTextFormatPresentation>;
  // Final text format keyed by canonical resource identity.
  // 按 canonical 资源身份分组的最终文本格式。
  textFormatsByResource: Map<string, FileTextFormatPresentation>;
}

// nativeChangeSetCorrelation reads both the native v2 extension and legacy LuaSkills v1 extension.
// nativeChangeSetCorrelation 同时读取原生 v2 扩展与旧版 LuaSkills v1 扩展。
function nativeChangeSetCorrelation(payload: Record<string, unknown>): NativeChangeSetCorrelation {
  const operationIdsByIndex = new Map<number, string[]>();
  const operationIdsByResource = new Map<string, string[]>();
  const requestPathsByIndex = new Map<number, string>();
  const requestPathsByResource = new Map<string, string>();
  const textFormatsByIndex = new Map<number, FileTextFormatPresentation>();
  const textFormatsByResource = new Map<string, FileTextFormatPresentation>();
  const extensions = objectValue(payload.extensions);
  const extension = objectValue(extensions?.["vulcan.file_tools.v2"])
    ?? objectValue(extensions?.["vulcan.file_edit.v1"]);
  for (const value of arrayValue(extension?.operation_results)) {
    const operation = objectValue(value);
    const operationIndex = numberValue(operation?.operation_index);
    const operationId = stringValue(operation?.operation_id)
      ?? (operationIndex === null ? null : `operation-index:${operationIndex}`);
    const resourceId = stringValue(operation?.resource_id);
    const requestPath = stringValue(operation?.request_path);
    if (!operationId) {
      continue;
    }
    if (resourceId) {
      appendUniqueMapValue(operationIdsByResource, resourceId, operationId);
      if (requestPath && !requestPathsByResource.has(resourceId)) {
        requestPathsByResource.set(resourceId, requestPath);
      }
    }
    if (operationIndex !== null) {
      appendUniqueMapValue(operationIdsByIndex, operationIndex, operationId);
      if (requestPath && !requestPathsByIndex.has(operationIndex)) {
        requestPathsByIndex.set(operationIndex, requestPath);
      }
    }
  }
  for (const [resourceIndex, value] of arrayValue(extension?.resources).entries()) {
    const resource = objectValue(value);
    const resourceId = stringValue(resource?.resource_id);
    const textFormat = nativeFileTextFormat(objectValue(resource?.after_text_format));
    if (!textFormat) {
      continue;
    }
    textFormatsByIndex.set(resourceIndex, textFormat);
    if (resourceId) {
      textFormatsByResource.set(resourceId, textFormat);
    }
  }
  const singleFileTextFormat = nativeFileTextFormat(extension);
  if (singleFileTextFormat && !textFormatsByIndex.size) {
    textFormatsByIndex.set(0, singleFileTextFormat);
  }
  return {
    operationIdsByIndex,
    operationIdsByResource,
    requestPathsByIndex,
    requestPathsByResource,
    textFormatsByIndex,
    textFormatsByResource,
  };
}

// appendUniqueMapValue appends one string exactly once to an ordered map bucket.
// appendUniqueMapValue 将一个字符串仅追加一次到有序映射桶。
function appendUniqueMapValue<TKey>(
  target: Map<TKey, string[]>,
  key: TKey,
  value: string,
): void {
  const values = target.get(key) ?? [];
  if (!values.includes(value)) {
    values.push(value);
    target.set(key, values);
  }
}

// lastChangeSetPayload returns the newest valid host_result change_set payload.
// lastChangeSetPayload 返回最新的有效 host_result change_set 载荷。
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
  // Read V1 native diagnostics from the original envelope before a changeset can shadow host metadata.
  // 在变更集可能覆盖宿主元数据之前，从原始信封读取 V1 原生诊断。
  const event = fixedV1ToolEvent(blocks);
  if (event) {
    // Native errors preserve both the compact message and structured details in host_result.payload.
    // 原生错误在 host_result.payload 中同时保留紧凑消息与结构化详情。
    const payload = metadataValue(event.output, ["host_result.payload"]);
    // Keep both producer messages when present; absent envelopes continue through existing body handling.
    // 若存在则同时保留两份产出方消息；缺少信封时继续使用既有正文处理。
    const detail = compactJoinedText([
      metadataString(payload, ["error"]) ?? "",
      metadataString(payload, ["details.message"]) ?? "",
    ]);
    if (detail) return detail;
  }
  const candidates: string[] = [];
  for (const block of blocks) {
    for (const path of [
      "error",
      "result.error",
      "payload.error",
      "host_result.error",
      "hostResult.error",
      "host_result.payload.error",
      "hostResult.payload.error",
      "host_result.payload.details.message",
      "hostResult.payload.details.message",
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
  // FileToolError::fmt adds "[code] " to the same message retained in details.message.
  // FileToolError::fmt 给 details.message 保留的同一消息添加“[code] ”前缀。
  // Remove only exact unprefixed duplicates; retain distinct context and differently coded errors.
  // 仅移除完全相同的无前缀重复消息；保留不同的补充内容和不同错误码的错误。
  const distinct = unique.filter(value => !unique.some(other =>
    other !== value && other.replace(/^\[[^\]\r\n]+\] /, "") === value,
  ));
  return distinct.join("\n").slice(0, 6000);
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
    return { kind: "project", path: relative ? compactMiddlePath(relative) : "." };
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

// fileChangePresentationId creates a stable expansion key from tool identifiers and target rows.
// fileChangePresentationId 根据工具标识与目标行创建稳定展开键。
function fileChangePresentationId(
  blocks: ToolExecutionConversationBlock[],
  files: Array<{ id: string }>,
  prefix: string,
): string {
  const identifier = blocks
    .map((block) => block.toolCallId ?? block.id)
    .find(Boolean);
  const fileKey = files.map((file) => file.id).sort().join("|");
  return `file-change:${prefix}:${identifier ?? compactHash(fileKey)}`;
}
