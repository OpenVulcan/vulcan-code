import type {
  ConversationEventBody,
  ConversationEventSummary,
  PersistentChangeSet,
  PersistentChangeSetFile,
} from "@/service/conversation-event-window-store";

// This module holds the per-Turn change facts the transcript needs eagerly; diff projection stays in the lazily
// loaded workspace-review-v1-selector.
// 本模块保存转录需要立即使用的每轮变更事实；diff 投影仍留在懒加载的 workspace-review-v1-selector 中。

/** One typed ToolCall event whose persistent ChangeSet is present.
 * 一条已携带持久化 ChangeSet 的强类型 ToolCall 事件。 */
export type WorkspaceReviewToolCallV1 =
  Extract<ConversationEventBody, { kind: "tool_call" }>
  & { changeset: PersistentChangeSet };

/** Per-file net record of one Turn's applied changes, shown by the turn statistics row.
 * 单个 Turn 已应用变更的逐文件汇总，由轮次统计行展示。 */
export interface TurnChangeSummaryFileV1 {
  /** Path the file has after the Turn, or its last path when the Turn deleted it.
   * 文件在该 Turn 结束后的路径；被删除时为其最后路径。 */
  path: string;
  /** Original path when the Turn renamed the file.
   * 该 Turn 重命名文件时的原路径。 */
  renamedFrom: string | null;
  /** Net lifecycle of the file across the Turn.
   * 文件在整个 Turn 中的净生命周期。 */
  status: "added" | "deleted" | "modified" | "renamed";
  /** Inserted lines summed over every applied ChangeSet of the Turn.
   * 该 Turn 全部已应用 ChangeSet 的新增行数之和。 */
  additions: number;
  /** Deleted lines summed over every applied ChangeSet of the Turn.
   * 该 Turn 全部已应用 ChangeSet 的删除行数之和。 */
  deletions: number;
}

/** All applied changes of one Turn, grouped by file and sorted by path.
 * 单个 Turn 的全部已应用变更，按文件分组并按路径排序。 */
export interface TurnChangeSummaryV1 {
  files: TurnChangeSummaryFileV1[];
  additions: number;
  deletions: number;
}

/** Returns whether one event is a ToolCall whose ChangeSet was applied to the workspace.
 * 返回一条事件是否为其 ChangeSet 已应用到工作区的 ToolCall。
 *
 * Preview ChangeSets wait for approval and never touched a file, so they do not count as Turn changes.
 * 预览 ChangeSet 仍在等待审批、从未改动文件，因此不计入 Turn 变更。 */
export function isAppliedChangeSetToolCallV1(
  event: ConversationEventBody,
): event is WorkspaceReviewToolCallV1 {
  return isWorkspaceReviewToolCallV1(event) && event.changeset.mode === "applied";
}

/** Resolves which Turn the "last turn" review shows.
 * 解析「上一轮」审核展示的 Turn。
 *
 * The running Turn counts once it has applied a change; until then, and whenever no Turn is running, the newest
 * Turn before it is shown.
 * 进行中的 Turn 在产生已应用变更后即计入；在此之前以及没有进行中 Turn 时，展示其之前最新的 Turn。
 *
 * @param events canonical events in strict seq order.
 *               按严格 seq 顺序排列的 canonical 事件。
 * @param openTurnId identity of the running Turn, or null when none runs.
 *                   进行中 Turn 的身份；没有时为空。
 * @returns the Turn to show, or null when the window holds no other Turn.
 *          需要展示的 Turn；窗口中没有其他 Turn 时为空。 */
export function resolveLastReviewTurnIdV1(
  events: readonly ConversationEventSummary[],
  openTurnId: number | null,
): number | null {
  let previousTurnId: number | null = null;
  let openTurnChanged = false;
  for (const summary of events) {
    if (summary.turn_id === null) continue;
    if (summary.turn_id === openTurnId) {
      openTurnChanged ||= isAppliedChangeSetToolCallV1(summary.event);
      continue;
    }
    previousTurnId = summary.turn_id;
  }
  return openTurnChanged ? openTurnId : previousTurnId;
}

/** Summarizes the applied changes of every Turn present in the window.
 * 汇总窗口中每个 Turn 的已应用变更。
 *
 * @param events canonical events in strict seq order.
 *               按严格 seq 顺序排列的 canonical 事件。
 * @returns summaries keyed by Turn identity; Turns without applied changes are absent.
 *          以 Turn 身份为键的汇总；没有已应用变更的 Turn 不出现。 */
export function turnChangeSummariesV1(
  events: readonly ConversationEventSummary[],
): Map<number, TurnChangeSummaryV1> {
  const toolCallsByTurn = new Map<number, WorkspaceReviewToolCallV1[]>();
  for (const summary of events) {
    if (summary.turn_id === null || !isAppliedChangeSetToolCallV1(summary.event)) continue;
    const toolCalls = toolCallsByTurn.get(summary.turn_id) ?? [];
    toolCalls.push(summary.event);
    toolCallsByTurn.set(summary.turn_id, toolCalls);
  }
  const summaries = new Map<number, TurnChangeSummaryV1>();
  for (const [turnId, toolCalls] of toolCallsByTurn) {
    const files = trackTurnFilesV1(toolCalls).map((track): TurnChangeSummaryFileV1 => {
      const status = turnFileStatusV1(track);
      return {
        path: track.currentPath,
        renamedFrom: status === "renamed" ? track.originalPath : null,
        status,
        additions: track.records.reduce((total, file) => total + persistentFileAdditionCount(file), 0),
        deletions: track.records.reduce((total, file) => total + persistentFileDeletionCount(file), 0),
      };
    });
    if (files.length === 0) continue;
    summaries.set(turnId, {
      files,
      additions: files.reduce((total, file) => total + file.additions, 0),
      deletions: files.reduce((total, file) => total + file.deletions, 0),
    });
  }
  return summaries;
}

/** One file followed across the applied ChangeSets of a Turn.
 * 在一个 Turn 的已应用 ChangeSet 中被追踪的单个文件。 */
export interface TurnFileTrackV1 {
  /** Path before the Turn, or null when the Turn created the file.
   * Turn 之前的路径；文件由该 Turn 创建时为空。 */
  originalPath: string | null;
  /** Latest path the Turn gave the file.
   * 该 Turn 给文件的最新路径。 */
  currentPath: string;
  /** Producer records of this file in tool-call order.
   * 该文件按工具调用顺序排列的产出方记录。 */
  records: PersistentChangeSetFile[];
}

/** Groups the file records of applied ChangeSets by file, following renames, sorted by final path.
 * 按文件分组已应用 ChangeSet 的文件记录，跟随重命名，并按最终路径排序。 */
export function trackTurnFilesV1(toolCalls: readonly WorkspaceReviewToolCallV1[]): TurnFileTrackV1[] {
  const tracks = new Map<string, TurnFileTrackV1>();
  for (const toolCall of toolCalls) {
    toolCall.changeset.files.forEach((file, fileIndex) => {
      const path = requirePersistentPath(file.path, `files[${fileIndex}].path`);
      // sourcePath is where an existing track for this file is keyed before this record.
      // sourcePath 是本条记录之前该文件已有追踪所用的键。
      const sourcePath = file.change === "rename"
        ? requirePersistentPath(file.old_path, `files[${fileIndex}].old_path`)
        : path;
      const existing = tracks.get(sourcePath);
      tracks.delete(sourcePath);
      tracks.set(path, existing === undefined
        ? { originalPath: file.change === "create" ? null : sourcePath, currentPath: path, records: [file] }
        : { ...existing, currentPath: path, records: [...existing.records, file] });
    });
  }
  return [...tracks.values()].sort((left, right) =>
    left.currentPath < right.currentPath ? -1 : left.currentPath > right.currentPath ? 1 : 0);
}

/** Derives the net lifecycle of one tracked file.
 * 派生单个被追踪文件的净生命周期。 */
export function turnFileStatusV1(track: TurnFileTrackV1): TurnChangeSummaryFileV1["status"] {
  const lastChange = track.records[track.records.length - 1]?.change;
  if (lastChange === "delete") return "deleted";
  if (track.originalPath === null) return "added";
  return track.originalPath === track.currentPath ? "modified" : "renamed";
}

/** Returns complete additions for one persistent file record.
 * 返回一条持久化文件记录的完整新增行数。
 *
 * @param file validated persistent file record.
 *             已校验的持久化文件记录。
 * @returns producer-owned complete addition count.
 *          产出方拥有的完整新增行数。
 */
export function persistentFileAdditionCount(file: PersistentChangeSetFile): number {
  if (file.change === "create") return file.lifecycle_content?.total_line_count ?? 0;
  if (file.change === "delete") return 0;
  return file.hunks.reduce((total, hunk) => total + hunk.inserted.length, 0);
}

/** Returns complete deletions for one persistent file record.
 * 返回一条持久化文件记录的完整删除行数。
 *
 * @param file validated persistent file record.
 *             已校验的持久化文件记录。
 * @returns producer-owned complete deletion count.
 *          产出方拥有的完整删除行数。
 */
export function persistentFileDeletionCount(file: PersistentChangeSetFile): number {
  if (file.change === "delete") return file.lifecycle_content?.total_line_count ?? 0;
  if (file.change === "create") return 0;
  return file.hunks.reduce((total, hunk) => total + hunk.deleted.length, 0);
}

/** Requires an already validated path without trying alternate fields.
 * 要求已校验路径存在，不尝试其他候选字段。
 *
 * @param path exact field value from the persistent contract.
 *             来自持久化契约的精确字段值。
 * @param location field location used by the explicit error.
 *                 显式错误使用的字段位置。
 * @returns the non-empty producer-authored path.
 *          产出方记录的非空路径。
 */
export function requirePersistentPath(path: string | undefined, location: string): string {
  if (path === undefined || path.trim().length === 0) {
    throw new Error(`missing validated persistent path at ${location}`);
  }
  return path;
}

/** Narrows one canonical event body to a ToolCall carrying a persistent ChangeSet.
 * 将一条 canonical 事件体收窄为携带持久化 ChangeSet 的 ToolCall。
 *
 * @param event exact typed event body from the V1 snapshot.
 *              来自 V1 快照的精确强类型事件体。
 * @returns whether the event is a ToolCall with a present ChangeSet.
 *          该事件是否为已携带 ChangeSet 的 ToolCall。 */
export function isWorkspaceReviewToolCallV1(
  event: ConversationEventBody,
): event is WorkspaceReviewToolCallV1 {
  return event.kind === "tool_call" && event.changeset !== undefined;
}
