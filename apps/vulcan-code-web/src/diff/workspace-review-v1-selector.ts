import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type {
  PersistentChangeSet,
  PersistentChangeSetFile,
  PersistentChangeSetHunk,
  PersistentLifecycleContent,
} from "@/service/conversation-event-window-store";
import { diffFileLargeReasons, diffStatsLargeReasons } from "./limits";
import {
  isWorkspaceReviewToolCallV1,
  persistentFileAdditionCount,
  persistentFileDeletionCount,
  requirePersistentPath,
  resolveLastReviewTurnIdV1,
  isAppliedChangeSetToolCallV1,
  trackTurnFilesV1,
  turnFileStatusV1,
  type TurnFileTrackV1,
  type WorkspaceReviewToolCallV1,
} from "./turn-change-summary-v1";
import type {
  DiffFile,
  DiffHunk,
  DiffLine,
  DiffPanelSnapshot,
} from "./types";

// textEncoder measures exact UTF-8 bytes for producer-authored changed lines.
// textEncoder 计算产出方已记录变更行的精确 UTF-8 字节数。
const textEncoder = new TextEncoder();

/** Latest canonical ToolCall selection consumed by the production Workspace Review host.
 * 生产 Workspace Review Host 消费的最新 canonical ToolCall 选择结果。 */
export interface WorkspaceReviewChangeSetSelectionV1 {
  /** Exact selected-session identity.
   * 精确的选中会话身份。 */
  session_id: string;
  /** Event-window generation that owns this selection.
   * 持有该选择结果的事件窗口代次。 */
  window_generation: number;
  /** Canonical tool event seq.
   * canonical 工具事件 seq。 */
  seq: number;
  /** In-place semantic revision of the selected tool event.
   * 选中工具事件的原位语义修订号。 */
  event_revision: number;
  /** Turn identity retained by the selected event.
   * 选中事件保留的 Turn 身份。 */
  turn_id: number | null;
  /** Exact typed ToolCall body, including the persistent ChangeSet.
   * 精确的强类型 ToolCall 事件体，包含持久化 ChangeSet。 */
  tool_call: WorkspaceReviewToolCallV1;
}

/** Selects the newest persisted ChangeSet by canonical event seq without legacy projection.
 * 不经过旧投影，按 canonical 事件 seq 选择最新持久化 ChangeSet。
 *
 * @param snapshot one atomically published selected-session V1 window.
 *                 一份原子发布的选中会话 V1 窗口。
 * @returns the latest owner-bound ToolCall selection, or null when none exists.
 *          最新的归属绑定 ToolCall 选择结果；不存在时返回 null。 */
export function selectLatestWorkspaceReviewChangeSetV1(
  snapshot: ConversationEventWindowControllerSnapshot,
): WorkspaceReviewChangeSetSelectionV1 | null {
  const owner = snapshot.owner;
  if (owner === null) return null;

  // previousSeq verifies that this pure selector never repairs or reorders malformed input.
  // previousSeq 验证该纯 selector 绝不修复或重排异常输入。
  let previousSeq: number | null = null;
  // selected advances only when a later real ToolCall seq carries a ChangeSet.
  // selected 仅在更晚的真实 ToolCall seq 携带 ChangeSet 时前进。
  let selected: WorkspaceReviewChangeSetSelectionV1 | null = null;
  for (const summary of snapshot.events) {
    if (previousSeq !== null && summary.seq !== previousSeq + 1) {
      throw new Error(
        `workspace review V1 selector requires consecutive seq: expected ${previousSeq + 1}, actual ${summary.seq}`,
      );
    }
    previousSeq = summary.seq;
    if (!isWorkspaceReviewToolCallV1(summary.event)) continue;
    selected = {
      session_id: owner.session_id,
      window_generation: owner.window_generation,
      seq: summary.seq,
      event_revision: summary.event_revision,
      turn_id: summary.turn_id,
      tool_call: summary.event,
    };
  }
  return selected;
}

/** The Turn whose applied ChangeSets the review panel shows as its "last turn" source.
 * 审核面板作为「上一轮」来源展示其已应用 ChangeSet 的 Turn。 */
export interface WorkspaceReviewTurnSelectionV1 {
  /** Exact selected-session identity.
   * 精确的选中会话身份。 */
  session_id: string;
  /** Event-window generation that owns this selection.
   * 持有该选择结果的事件窗口代次。 */
  window_generation: number;
  /** Selected Turn identity.
   * 选中的 Turn 身份。 */
  turn_id: number;
  /** Applied ChangeSet tool calls of the Turn in canonical seq order; empty when the Turn changed no file.
   * 该 Turn 按 canonical seq 排列的已应用 ChangeSet 工具调用；未改动文件时为空。 */
  tool_calls: readonly WorkspaceReviewToolCallV1[];
}

/** Selects the "last turn" review source from the selected-session V1 window.
 * 从选中会话 V1 窗口选择「上一轮」审核来源。
 *
 * @param snapshot one atomically published selected-session V1 window.
 *                 一份原子发布的选中会话 V1 窗口。
 * @returns the resolved Turn with its applied ChangeSets, or null before hydration or when no Turn exists.
 *          解析出的 Turn 及其已应用 ChangeSet；hydration 前或不存在 Turn 时为空。 */
export function selectLastTurnWorkspaceReviewV1(
  snapshot: ConversationEventWindowControllerSnapshot,
): WorkspaceReviewTurnSelectionV1 | null {
  const owner = snapshot.owner;
  if (owner === null) return null;
  const turnId = resolveLastReviewTurnIdV1(snapshot.events, snapshot.open_turn_id);
  if (turnId === null) return null;
  return {
    session_id: owner.session_id,
    window_generation: owner.window_generation,
    turn_id: turnId,
    tool_calls: snapshot.events.flatMap((summary) => summary.turn_id === turnId
      && isAppliedChangeSetToolCallV1(summary.event) ? [summary.event] : []),
  };
}

/** Projects every applied ChangeSet of one Turn into one review snapshot with one entry per file.
 * 把单个 Turn 的全部已应用 ChangeSet 投影为每个文件一项的审核快照。
 *
 * Hunks of the same file keep their tool-call order; each keeps the line numbers its producer recorded.
 * 同一文件的 hunk 保持工具调用顺序，各自保留产出方记录的行号。
 *
 * @param selection resolved "last turn" source, or null when the window holds no Turn.
 *                  已解析的「上一轮」来源；窗口中没有 Turn 时为空。
 * @returns one renderer-ready typed snapshot, empty when there is no Turn or it changed no file.
 *          一份可直接渲染的强类型快照；没有 Turn 或其未改动文件时为空。 */
export function workspaceReviewSnapshotFromTurnV1(
  selection: WorkspaceReviewTurnSelectionV1 | null,
): DiffPanelSnapshot {
  const toolCalls = selection?.tool_calls ?? [];
  const files = trackTurnFilesV1(toolCalls).map(projectTurnFileV1);
  const changeSets = toolCalls.map((toolCall) => toolCall.changeset);
  return reviewSnapshotFromFilesV1(files, changeSets);
}

/** Merges the projected records of one tracked file into one review file.
 * 把单个被追踪文件的投影记录合并为一份审核文件。 */
function projectTurnFileV1(track: TurnFileTrackV1): DiffFile {
  const projected = track.records.map(projectPersistentChangeSetFileV1);
  const status = turnFileStatusV1(track);
  const oldPath = status === "added" ? "/dev/null" : track.originalPath ?? track.currentPath;
  const newPath = status === "deleted" ? "/dev/null" : track.currentPath;
  const sum = (read: (file: DiffFile) => number) => projected.reduce((total, file) => total + read(file), 0);
  const changedLines = sum((file) => file.changedLines);
  const changedBytes = sum((file) => file.changedBytes);
  const maxChangedLineBytes = Math.max(0, ...projected.map((file) => file.maxChangedLineBytes));
  const largeReasons = diffFileLargeReasons({ changedLines, changedBytes, maxChangedLineBytes });
  const hunks = projected.flatMap((file) => file.hunks);
  return {
    key: `turn\u0000${track.originalPath ?? ""}\u0000${track.currentPath}`,
    oldPath,
    newPath,
    status,
    additions: sum((file) => file.additions),
    deletions: sum((file) => file.deletions),
    isBinary: false,
    isGitlink: false,
    fileMode: null,
    changedLines,
    changedBytes,
    maxChangedLineBytes,
    isLarge: largeReasons.length > 0,
    tooLargeToDisplay: projected.some((file) => file.tooLargeToDisplay),
    largeReasons,
    rowCount: sum((file) => file.rowCount ?? 0),
    estimatedHunkCount: hunks.length,
    oldFileLineCount: projected[0]?.oldFileLineCount,
    newFileLineCount: projected[projected.length - 1]?.newFileLineCount,
    isPartial: projected.some((file) => file.isPartial),
    rawDiff: projected.map((file) => file.rawDiff ?? "").filter(Boolean).join("\n"),
    hunks,
  };
}

/** Projects one selected persisted ChangeSet directly into the Workspace Review render model.
 * 把一份已选中的持久化 ChangeSet 直接投影为 Workspace Review 渲染模型。
 *
 * @param selection exact owner-bound ToolCall selection from the V1 event window.
 *                  来自 V1 事件窗口、已绑定精确归属的 ToolCall 选择。
 * @returns one renderer-ready typed snapshot without parsing legacy raw diff text.
 *          一份无需解析旧 raw diff 文本即可渲染的强类型快照。
 */
export function workspaceReviewSnapshotFromChangeSetV1(
  selection: WorkspaceReviewChangeSetSelectionV1,
): DiffPanelSnapshot {
  // changeSet is the already validated durable payload carried by the selected ToolCall.
  // changeSet 是选中 ToolCall 携带且已经过校验的持久化载荷。
  const changeSet = selection.tool_call.changeset;
  // files preserve the producer-authored file order without sorting or filesystem reads.
  // files 保留产出方记录的文件顺序，不排序也不读取当前文件系统。
  const files = changeSet.files.map(projectPersistentChangeSetFileV1);
  return reviewSnapshotFromFilesV1(files, [changeSet]);
}

/** Builds one review snapshot from projected files and the ChangeSets they came from.
 * 由已投影文件及其来源 ChangeSet 构建一份审核快照。 */
function reviewSnapshotFromFilesV1(
  files: DiffFile[],
  changeSets: readonly PersistentChangeSet[],
): DiffPanelSnapshot {
  // totalChangedLines is derived only from typed inserted/deleted or lifecycle totals.
  // totalChangedLines 仅从强类型新增、删除行或生命周期总数派生。
  const totalChangedLines = files.reduce((total, file) => total + file.changedLines, 0);
  // totalChangedBytes covers the exact retained changed-line content available in the event.
  // totalChangedBytes 覆盖事件中实际保留的全部变更行正文。
  const totalChangedBytes = files.reduce((total, file) => total + file.changedBytes, 0);
  // largeReasons applies the existing review thresholds to the typed aggregate facts.
  // largeReasons 把现有审核阈值应用于强类型聚合事实。
  const largeReasons = diffStatsLargeReasons({
    files: files.length,
    totalChangedLines,
    totalChangedBytes,
  });
  // rawDiff is a copy-only serialization produced from the typed model, never a parse input.
  // rawDiff 仅用于复制，由强类型模型序列化生成，绝不作为解析输入。
  const rawDiff = files.map((file) => file.rawDiff ?? "").filter(Boolean).join("\n");
  // completeness reflects explicit producer truncation or omission across all file records.
  // completeness 反映全部文件记录中产出方明确声明的截断或省略。
  const completeness = changeSetCompleteness(changeSets);
  return {
    source: "change_set",
    files,
    stats: {
      files: files.length,
      additions: files.reduce((total, file) => total + file.additions, 0),
      deletions: files.reduce((total, file) => total + file.deletions, 0),
      totalChangedLines,
      totalChangedBytes,
      isLarge: largeReasons.length > 0,
      largeReasons,
    },
    completeness,
    isTruncated: completeness !== "complete",
    rawDiff,
  };
}

/** Projects one validated persistent file record into one review file model.
 * 把一条已校验的持久化文件记录投影为一份审核文件模型。
 *
 * @param file producer-authored persistent file record.
 *             产出方记录的持久化文件记录。
 * @param fileIndex stable producer-order index used only for the React key.
 *                  仅用于 React key 的稳定产出顺序索引。
 * @returns one typed review file without path guessing.
 *          一份不进行路径猜测的强类型审核文件。
 */
function projectPersistentChangeSetFileV1(
  file: PersistentChangeSetFile,
  fileIndex: number,
): DiffFile {
  // path is mandatory in the durable Rust validator for every change kind.
  // path 在持久化 Rust 校验器中对所有变更类型都是必填字段。
  const path = requirePersistentPath(file.path, `files[${fileIndex}].path`);
  // oldPath follows the validated change-kind contract exactly.
  // oldPath 严格遵循已经校验的变更类型契约。
  const oldPath = file.change === "create"
    ? "/dev/null"
    : file.change === "rename"
      ? requirePersistentPath(file.old_path, `files[${fileIndex}].old_path`)
      : path;
  // newPath follows the validated change-kind contract exactly.
  // newPath 严格遵循已经校验的变更类型契约。
  const newPath = file.change === "delete"
    ? "/dev/null"
    : file.change === "rename"
      ? requirePersistentPath(file.new_path, `files[${fileIndex}].new_path`)
      : path;
  // hunks use lifecycle content only when the producer selected that exact representation.
  // hunks 仅在产出方明确选择生命周期正文时使用该表示。
  const hunks = file.lifecycle_content === undefined
    ? file.hunks.map(projectPersistentHunkV1)
    : [projectPersistentLifecycleHunkV1(file.change, file.lifecycle_content)];
  // additions preserves complete lifecycle totals even when the visible body is truncated.
  // additions 即使可见正文被截断，也保留完整生命周期总数。
  const additions = persistentFileAdditionCount(file);
  // deletions preserves complete lifecycle totals even when the visible body is truncated.
  // deletions 即使可见正文被截断，也保留完整生命周期总数。
  const deletions = persistentFileDeletionCount(file);
  // changedContents contains only exact changed rows retained by the durable event.
  // changedContents 仅包含持久化事件中实际保留的变更行。
  const changedContents = hunks.flatMap((hunk) => hunk.lines
    .filter((line) => line.kind === "added" || line.kind === "removed")
    .map((line) => line.content));
  // changedBytes is exact for retained rows and intentionally does not invent omitted bytes.
  // changedBytes 对已保留行是精确值，并且不会虚构被省略正文的字节数。
  const changedBytes = changedContents.reduce((total, content) => total + textEncoder.encode(content).byteLength, 0);
  // maxChangedLineBytes is the largest exact retained changed row.
  // maxChangedLineBytes 是已保留变更行中的最大精确字节数。
  const maxChangedLineBytes = changedContents.reduce(
    (maximum, content) => Math.max(maximum, textEncoder.encode(content).byteLength),
    0,
  );
  // changedLines uses complete declared counts rather than visible-row count.
  // changedLines 使用完整声明数量，而不是可见行数量。
  const changedLines = additions + deletions;
  // largeReasons applies the shared renderer thresholds to this typed file.
  // largeReasons 把共享渲染阈值应用于这份强类型文件。
  const largeReasons = diffFileLargeReasons({ changedLines, changedBytes, maxChangedLineBytes });
  // isPartial records producer-declared lifecycle truncation or omission.
  // isPartial 记录产出方明确声明的生命周期截断或省略。
  const isPartial = file.lifecycle_content !== undefined && file.lifecycle_content.mode !== "full";
  // rawDiff is generated after the typed hunk model exists and is used only for copy.
  // rawDiff 在强类型 hunk 模型完成后生成，并且只用于复制。
  const rawDiff = serializeTypedReviewFileV1(oldPath, newPath, file.change, hunks);
  return {
    key: `${file.change}\u0000${oldPath}\u0000${newPath}\u0000${fileIndex}`,
    oldPath,
    newPath,
    status: file.change === "create"
      ? "added"
      : file.change === "delete"
        ? "deleted"
        : file.change === "rename"
          ? "renamed"
          : "modified",
    additions,
    deletions,
    isBinary: false,
    isGitlink: false,
    fileMode: null,
    changedLines,
    changedBytes,
    maxChangedLineBytes,
    isLarge: largeReasons.length > 0,
    tooLargeToDisplay: file.lifecycle_content?.mode === "omitted",
    largeReasons,
    rowCount: hunks.reduce((total, hunk) => total + 1 + hunk.lines.length, 0),
    estimatedRowCount: file.lifecycle_content?.total_line_count,
    estimatedHunkCount: hunks.length,
    oldFileLineCount: persistentOldFileLineCount(file),
    newFileLineCount: persistentNewFileLineCount(file),
    isPartial,
    rawDiff,
    hunks,
  };
}

/** Projects one modify hunk while preserving exact producer line numbers and row order.
 * 投影一条修改 hunk，同时保留产出方的精确行号和行顺序。
 *
 * @param hunk persistent modify hunk.
 *             持久化修改 hunk。
 * @returns one renderer-ready review hunk.
 *          一份可直接渲染的审核 hunk。
 */
function projectPersistentHunkV1(hunk: PersistentChangeSetHunk): DiffHunk {
  // beforeLines is exact producer-authored leading context.
  // beforeLines 是产出方记录的精确前置上下文。
  const beforeLines = logicalPersistentLines(hunk.before);
  // afterLines is exact producer-authored trailing context.
  // afterLines 是产出方记录的精确后置上下文。
  const afterLines = logicalPersistentLines(hunk.after);
  // firstOldChanged is the first old-side changed line, or the paired insertion anchor.
  // firstOldChanged 是旧侧第一条变更行，或成对的插入锚点。
  const firstOldChanged = hunk.deleted[0]?.line ?? hunk.inserted[0]?.line ?? 1;
  // firstNewChanged is the first new-side changed line, or the paired deletion anchor.
  // firstNewChanged 是新侧第一条变更行，或成对的删除锚点。
  const firstNewChanged = hunk.inserted[0]?.line ?? hunk.deleted[0]?.line ?? 1;
  // oldStart includes the retained leading context without crossing line one.
  // oldStart 包含已保留前置上下文且不会越过第一行。
  const oldStart = Math.max(1, firstOldChanged - beforeLines.length);
  // newStart includes the retained leading context without crossing line one.
  // newStart 包含已保留前置上下文且不会越过第一行。
  const newStart = Math.max(1, firstNewChanged - beforeLines.length);
  // lines preserves context, deleted, inserted, then trailing-context order.
  // lines 保留前置上下文、删除、新增、后置上下文的事实顺序。
  const lines: DiffLine[] = [
    ...beforeLines.map((content, index) => ({
      kind: "context" as const,
      content,
      oldLine: oldStart + index,
      newLine: newStart + index,
    })),
    ...hunk.deleted.map((line) => ({
      kind: "removed" as const,
      content: line.content,
      oldLine: line.line,
      newLine: null,
    })),
    ...hunk.inserted.map((line) => ({
      kind: "added" as const,
      content: line.content,
      oldLine: null,
      newLine: line.line,
    })),
    ...afterLines.map((content, index) => ({
      kind: "context" as const,
      content,
      oldLine: (hunk.deleted.at(-1)?.line ?? firstOldChanged) + index + 1,
      newLine: (hunk.inserted.at(-1)?.line ?? firstNewChanged) + index + 1,
    })),
  ];
  // oldCount is the exact rendered old-side row span.
  // oldCount 是精确渲染的旧侧行跨度。
  const oldCount = beforeLines.length + hunk.deleted.length + afterLines.length;
  // newCount is the exact rendered new-side row span.
  // newCount 是精确渲染的新侧行跨度。
  const newCount = beforeLines.length + hunk.inserted.length + afterLines.length;
  return {
    header: `@@ -${oldStart},${oldCount} +${newStart},${newCount} @@`,
    oldStart,
    newStart,
    lines,
  };
}

/** Projects lifecycle content without consulting the current file system.
 * 在不读取当前文件系统的情况下投影生命周期正文。
 *
 * @param change validated file change kind.
 *               已校验的文件变更类型。
 * @param lifecycle exact full, truncated, or omitted producer content.
 *                  产出方记录的精确完整、截断或省略正文。
 * @returns one renderer-ready lifecycle hunk.
 *          一份可直接渲染的生命周期 hunk。
 */
function projectPersistentLifecycleHunkV1(
  change: PersistentChangeSetFile["change"],
  lifecycle: PersistentLifecycleContent,
): DiffHunk {
  // retainedRows preserves exact producer text and complete-file line numbering.
  // retainedRows 保留产出方精确正文及完整文件行号。
  const retainedRows = persistentLifecycleRowsV1(change, lifecycle);
  // totalLineCount is the producer-declared complete count when available.
  // totalLineCount 是产出方可用时声明的完整行数。
  const totalLineCount = lifecycle.total_line_count ?? 0;
  // oldCount assigns lifecycle rows to the old side only for deletion or pure rename context.
  // oldCount 仅针对删除或纯重命名上下文把生命周期行分配到旧侧。
  const oldCount = change === "create" ? 0 : totalLineCount;
  // newCount assigns lifecycle rows to the new side only for creation or pure rename context.
  // newCount 仅针对创建或纯重命名上下文把生命周期行分配到新侧。
  const newCount = change === "delete" ? 0 : totalLineCount;
  return {
    header: `@@ -1,${oldCount} +1,${newCount} @@`,
    oldStart: 1,
    newStart: 1,
    lines: retainedRows,
  };
}

/** Builds visible lifecycle rows from the exact retained body.
 * 从精确保留的正文构建可见生命周期行。
 *
 * @param change validated file change kind.
 *               已校验的文件变更类型。
 * @param lifecycle exact producer lifecycle content.
 *                  产出方记录的精确生命周期正文。
 * @returns visible rows with complete-file line numbers.
 *          带完整文件行号的可见行。
 */
function persistentLifecycleRowsV1(
  change: PersistentChangeSetFile["change"],
  lifecycle: PersistentLifecycleContent,
): DiffLine[] {
  // rowKind follows the file side that owns the lifecycle content.
  // rowKind 遵循承载生命周期正文的文件侧。
  const rowKind: DiffLine["kind"] = change === "create"
    ? "added"
    : change === "delete"
      ? "removed"
      : "context";
  if (lifecycle.mode === "omitted") {
    return [{
      kind: "metadata",
      content: lifecycle.total_line_count === undefined
        ? "content omitted by producer"
        : `${lifecycle.total_line_count} lines omitted by producer`,
      oldLine: null,
      newLine: null,
    }];
  }
  if (lifecycle.mode === "full") {
    return numberedLifecycleRowsV1(change, rowKind, logicalPersistentLines(lifecycle.content), 1);
  }
  // headLines and tailLines remain separate so tail numbering uses the complete total.
  // headLines 与 tailLines 保持分离，使尾部编号使用完整总数。
  const headLines = logicalPersistentLines(lifecycle.head);
  // tailLines contains the exact retained end of the complete file.
  // tailLines 包含完整文件末尾精确保留的正文。
  const tailLines = logicalPersistentLines(lifecycle.tail);
  // omittedCount is derived from the declared total and exact retained rows.
  // omittedCount 由声明总数与精确保留行数派生。
  const omittedCount = lifecycle.total_line_count - headLines.length - tailLines.length;
  if (omittedCount < 0) {
    throw new Error("persistent lifecycle content retains more rows than total_line_count");
  }
  return [
    ...numberedLifecycleRowsV1(change, rowKind, headLines, 1),
    ...(omittedCount > 0 ? [{
      kind: "metadata" as const,
      content: `${omittedCount} lines omitted by producer`,
      oldLine: null,
      newLine: null,
    }] : []),
    ...numberedLifecycleRowsV1(
      change,
      rowKind,
      tailLines,
      lifecycle.total_line_count - tailLines.length + 1,
    ),
  ];
}

/** Numbers retained lifecycle rows on the exact old/new file side.
 * 在精确的旧/新文件侧为已保留生命周期行编号。
 *
 * @param change validated file change kind.
 *               已校验的文件变更类型。
 * @param kind renderer row kind.
 *             渲染器行类型。
 * @param lines exact retained logical lines.
 *              精确保留的逻辑行。
 * @param startLine first complete-file line number.
 *                  完整文件中的首行编号。
 * @returns numbered review rows.
 *          已编号的审核行。
 */
function numberedLifecycleRowsV1(
  change: PersistentChangeSetFile["change"],
  kind: DiffLine["kind"],
  lines: readonly string[],
  startLine: number,
): DiffLine[] {
  return lines.map((content, index) => ({
    kind,
    content,
    oldLine: change === "create" ? null : startLine + index,
    newLine: change === "delete" ? null : startLine + index,
  }));
}

/** Splits producer text with the same newline semantics as the Rust validator.
 * 使用与 Rust 校验器相同的换行语义切分产出方文本。
 *
 * @param text producer-authored text.
 *             产出方记录的文本。
 * @returns logical lines without a synthetic trailing empty row.
 *          不包含虚假尾部空行的逻辑行。
 */
function logicalPersistentLines(text: string): string[] {
  // normalized aligns CRLF and lone CR with the durable validator.
  // normalized 把 CRLF 与独立 CR 对齐到持久化校验器语义。
  const normalized = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  if (normalized.length === 0) return [];
  // lines contains every logical line before final-newline adjustment.
  // lines 包含尾部换行调整前的全部逻辑行。
  const lines = normalized.split("\n");
  if (normalized.endsWith("\n")) lines.pop();
  return lines;
}

/** Returns the complete old-side line count when lifecycle content declares it.
 * 在生命周期正文声明完整数量时返回旧侧完整行数。
 *
 * @param file validated persistent file record.
 *             已校验的持久化文件记录。
 * @returns exact old-side count or null when the producer did not record it.
 *          精确旧侧行数；产出方未记录时返回 null。
 */
function persistentOldFileLineCount(file: PersistentChangeSetFile): number | null {
  if (file.change === "create") return 0;
  return file.lifecycle_content?.total_line_count ?? null;
}

/** Returns the complete new-side line count when lifecycle content declares it.
 * 在生命周期正文声明完整数量时返回新侧完整行数。
 *
 * @param file validated persistent file record.
 *             已校验的持久化文件记录。
 * @returns exact new-side count or null when the producer did not record it.
 *          精确新侧行数；产出方未记录时返回 null。
 */
function persistentNewFileLineCount(file: PersistentChangeSetFile): number | null {
  if (file.change === "delete") return 0;
  return file.lifecycle_content?.total_line_count ?? null;
}

/** Derives aggregate completeness from explicit lifecycle modes.
 * 从明确的生命周期模式派生聚合完整性。
 *
 * @param changeSets validated persistent ChangeSets shown together.
 *                   一同展示的已校验持久化 ChangeSet。
 * @returns complete, truncated, or partial review state.
 *          完整、截断或局部审核状态。
 */
function changeSetCompleteness(changeSets: readonly PersistentChangeSet[]): DiffPanelSnapshot["completeness"] {
  const files = changeSets.flatMap((changeSet) => changeSet.files);
  if (files.some((file) => file.lifecycle_content?.mode === "omitted")) return "partial";
  if (files.some((file) => file.lifecycle_content?.mode === "truncated")) return "truncated";
  return "complete";
}

/** Serializes one typed review file for the existing copy action only.
 * 仅为现有复制动作序列化一份强类型审核文件。
 *
 * @param oldPath exact old-side path.
 *                精确旧侧路径。
 * @param newPath exact new-side path.
 *                精确新侧路径。
 * @param change validated change kind.
 *               已校验的变更类型。
 * @param hunks typed review hunks.
 *              强类型审核 hunk。
 * @returns a unified textual representation that is never reparsed.
 *          一份绝不被重新解析的 unified 文本表示。
 */
function serializeTypedReviewFileV1(
  oldPath: string,
  newPath: string,
  change: PersistentChangeSetFile["change"],
  hunks: readonly DiffHunk[],
): string {
  // oldHeader uses Git's null path only for an exact create event.
  // oldHeader 仅对精确创建事件使用 Git 空路径。
  const oldHeader = oldPath === "/dev/null" ? oldPath : `a/${oldPath}`;
  // newHeader uses Git's null path only for an exact delete event.
  // newHeader 仅对精确删除事件使用 Git 空路径。
  const newHeader = newPath === "/dev/null" ? newPath : `b/${newPath}`;
  // metadata preserves exact lifecycle and rename semantics without inventing file modes.
  // metadata 保留精确生命周期与重命名语义，不虚构文件模式。
  const metadata = change === "rename"
    ? [`rename from ${oldPath}`, `rename to ${newPath}`]
    : [];
  return [
    `diff --git ${oldHeader} ${newHeader}`,
    ...metadata,
    `--- ${oldHeader}`,
    `+++ ${newHeader}`,
    ...hunks.flatMap((hunk) => [
      hunk.header,
      ...hunk.lines.map((line) => `${line.kind === "added" ? "+" : line.kind === "removed" ? "-" : " "}${line.content}`),
    ]),
  ].join("\n");
}
