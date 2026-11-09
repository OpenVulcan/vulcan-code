import React from "react";
import {
  ConversationBranchWorkspaceV1,
  type ConversationBranchWorkspaceV1Props,
} from "@/components/conversation/conversation-branch-workspace-v1";
import { useSelectedConversationEventWindowSnapshot } from "@/state/conversation-event-window-react";

/** Renders the selected session's transcript from the latest event-window snapshot.
 * 用最新事件窗口快照渲染选中会话的转录。
 *
 * This is the only full-snapshot subscriber under ChatArea: each streamed frame re-renders the transcript from here
 * while ChatArea, whose element props stay unchanged, is not rendered again. It is memoized, so a ChatArea render
 * that leaves every transcript prop unchanged does not project the transcript again.
 * 这是 ChatArea 之下唯一的完整快照订阅者：每个流式帧从这里重新渲染转录，而元素属性不变的 ChatArea 不会再次渲染。
 * 该组件经过缓存，ChatArea 渲染若未改变任何转录属性，转录不会重新投影。
 *
 * @param props every transcript input except the snapshot, which is read here.
 *              除快照外的全部转录输入；快照在此读取。
 * @returns the branch transcript bound to the selected snapshot.
 *          绑定到选中快照的分支转录。 */
export const SelectedConversationTranscript = React.memo(function SelectedConversationTranscript(
  props: Omit<ConversationBranchWorkspaceV1Props, "snapshot">,
) {
  const snapshot = useSelectedConversationEventWindowSnapshot();
  return <ConversationBranchWorkspaceV1 {...props} snapshot={snapshot} />;
});
