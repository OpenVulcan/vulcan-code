import React from "react";
import {
  ConversationBranchWorkspaceV1,
  type ConversationBranchWorkspaceV1Props,
} from "@/components/conversation/conversation-branch-workspace-v1";
import { useSelectedConversationEventWindowExternalStore } from "@/state/conversation-event-window-react";
import { useConversationTranscriptSnapshot } from "../conversation-transcript-stream";

/** Renders the selected session's transcript from the latest event-window snapshot.
 * 用最新事件窗口快照渲染选中会话的转录。
 *
 * The transcript subscribes to structural changes. Streaming reply blocks read their own text from the same window.
 * 转录订阅结构变化，流式回复块从同一窗口读取各自的正文。
 *
 * @param props every transcript input except the snapshot, which is read here.
 *              除快照外的全部转录输入；快照在此读取。
 * @returns the branch transcript bound to the selected snapshot.
 *          绑定到选中快照的分支转录。 */
export const SelectedConversationTranscript = React.memo(function SelectedConversationTranscript(
  props: Omit<ConversationBranchWorkspaceV1Props, "snapshot" | "snapshotReader">,
) {
  // The selected owner and its Markdown leaves share one coalesced publication.
  // 选中归属和它的 Markdown 叶子共用一次合并发布。
  const reader = useSelectedConversationEventWindowExternalStore();
  const snapshot = useConversationTranscriptSnapshot(reader, props.sessionRunning);
  return <ConversationBranchWorkspaceV1 {...props} snapshot={snapshot} snapshotReader={reader} />;
});
