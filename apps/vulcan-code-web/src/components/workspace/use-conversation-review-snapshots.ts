import * as React from "react";

import {
  selectLastTurnWorkspaceReviewV1,
  selectLatestWorkspaceReviewChangeSetV1,
  workspaceReviewSnapshotFromChangeSetV1,
  workspaceReviewSnapshotFromTurnV1,
} from "@/diff/workspace-review-v1-selector";
import type { DiffPanelSnapshot } from "@/diff/types";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import { conversationSelectionEqual, useSelectedConversationEventWindowSelector } from "@/state/conversation-event-window-react";

/** Review snapshots the panel derives from the selected conversation instead of the Git service.
 * 审核面板从选中会话（而非 Git 服务）派生的审核快照。 */
export interface ConversationReviewSnapshots {
  /** Latest single applied or previewed ChangeSet, shown while no Git snapshot is loaded.
   * 最新的单个 ChangeSet，在尚未加载 Git 快照时展示。 */
  latestChangeSet: DiffPanelSnapshot | null;
  /** Last-turn source; present exactly while that source is selected, even when the Turn changed no file.
   * 上一轮来源；恰在该来源被选中时存在，即使该 Turn 未改动文件。 */
  lastTurn: DiffPanelSnapshot | null;
}

/** Derives the conversation-owned review snapshots from the selected-session V1 event window.
 * 从选中会话 V1 事件窗口派生会话持有的审核快照。
 *
 * Each selector reads the window only while its owner is the selected conversation and re-renders only when its
 * selection changes.
 * 每个选择器仅在窗口归属为选中会话时读取，且仅在选择结果变化时重新渲染。
 *
 * @param selectedConversationId conversation the review panel follows.
 *                               审核面板跟随的会话。
 * @param lastTurnMode whether the last-turn source is selected.
 *                     是否选中了上一轮来源。 */
export function useConversationReviewSnapshots(
  selectedConversationId: string | null,
  lastTurnMode: boolean,
): ConversationReviewSnapshots {
  const latestSelection = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => snapshot.owner?.session_id === selectedConversationId
      ? selectLatestWorkspaceReviewChangeSetV1(snapshot)
      : null,
    [selectedConversationId],
  ), conversationSelectionEqual);
  const lastTurnSelection = useSelectedConversationEventWindowSelector(React.useCallback(
    (snapshot: ConversationEventWindowControllerSnapshot) => lastTurnMode
      && snapshot.owner?.session_id === selectedConversationId
      ? selectLastTurnWorkspaceReviewV1(snapshot)
      : null,
    [lastTurnMode, selectedConversationId],
  ), conversationSelectionEqual);
  const latestChangeSet = React.useMemo(
    () => latestSelection === null ? null : workspaceReviewSnapshotFromChangeSetV1(latestSelection),
    [latestSelection],
  );
  const lastTurn = React.useMemo(
    () => lastTurnMode ? workspaceReviewSnapshotFromTurnV1(lastTurnSelection) : null,
    [lastTurnMode, lastTurnSelection],
  );
  return { latestChangeSet, lastTurn };
}
