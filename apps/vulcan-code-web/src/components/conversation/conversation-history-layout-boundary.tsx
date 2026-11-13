import { Component, type ReactNode, type RefObject } from "react";
import { captureConversationViewportAnchor, type ConversationViewportAnchorSnapshot } from "./chat-area-scroll";
import type { CleanedHistoryProps } from "./conversation-cleaned-history";
import { publishConversationTurnWindowLayout } from "./conversation-turn-window";

// ConversationHistoryLayoutProps binds the asynchronously published history to its transcript.
// ConversationHistoryLayoutProps 将异步发布的历史绑定到对应的会话时间线。
interface ConversationHistoryLayoutProps {
  // The publication can arrive after the retained runtime rows are already visible.
  // 发布结果可能在保留的运行时回合已经显示后才到达。
  history: CleanedHistoryProps["history"] | undefined;
  // The scrollport shared by the two sources.
  // 两个来源共用的滚动视口。
  scrollRef: RefObject<HTMLElement | null> | undefined;
  // The shared list of turn shells.
  // 共享的回合外壳列表。
  children: ReactNode;
}

// HistoryLayoutSnapshot keeps the pre-commit reading position bound to the exact viewport element.
// HistoryLayoutSnapshot 将提交前的阅读位置绑定到精确的视口元素。
interface HistoryLayoutSnapshot {
  // The viewport that contained the anchor before the commit.
  // 提交前包含锚点的视口。
  scrollport: HTMLElement;
  // The previous reading position, if a turn intersected the viewport.
  // 变化前的阅读位置，视口与回合相交时可用。
  anchor: ConversationViewportAnchorSnapshot | null;
}

// ConversationHistoryLayoutBoundary preserves the reader position when a publication inserts or removes shells.
// ConversationHistoryLayoutBoundary 在发布插入或移除回合外壳时保留阅读位置。
export class ConversationHistoryLayoutBoundary extends Component<ConversationHistoryLayoutProps, object, HistoryLayoutSnapshot | null> {
  // Capture the outgoing geometry before React changes the list. Returns null for ordinary render updates.
  // 在 React 修改列表前捕获原几何。普通渲染更新返回空值。
  getSnapshotBeforeUpdate(previous: ConversationHistoryLayoutProps): HistoryLayoutSnapshot | null {
    // The scrollport must already be mounted and the published display records must have changed.
    // 滚动视口必须已经挂载，且已发布的展示记录发生了变化。
    const scrollport = this.props.scrollRef?.current;
    if (!scrollport || previous.history === this.props.history) return null;
    return { scrollport, anchor: captureConversationViewportAnchor(scrollport) };
  }

  // Report the committed geometry before paint. The snapshot is present only for publication changes.
  // 在绘制前通知几何变化。仅发布变动时存在快照。
  componentDidUpdate(_previous: ConversationHistoryLayoutProps, _state: object, snapshot: HistoryLayoutSnapshot | null): void {
    if (snapshot && snapshot.scrollport === this.props.scrollRef?.current) {
      publishConversationTurnWindowLayout(snapshot.scrollport, snapshot.anchor);
    }
  }

  // Keep the shells as direct children of the transcript timeline.
  // 保持回合外壳是时间线的直接子元素。
  render(): ReactNode {
    return this.props.children;
  }
}
