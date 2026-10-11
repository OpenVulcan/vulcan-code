import * as React from "react";

import type { TurnChangeReviewActions } from "@/components/conversation/turn-change-review-context";
import type { DiffReviewMode, WorkspaceReviewFocusRequest } from "@/diff/types";

/** Inputs the review-focus actions need from the workspace shell.
 * 审核聚焦动作需要从工作区外壳获得的输入。 */
export interface ReviewFocusActionInputs {
  /** Conversation the review panel follows.
   * 审核面板跟随的会话。 */
  selectedConversationId: string | null;
  /** Whether a persisted chat conversation is selected; drafts have no review panel.
   * 是否选中了已持久化的聊天会话；草稿没有审核面板。 */
  hasSelectedChatConversation: boolean;
  /** Repository the shared Git controls currently target.
   * 共享 Git 控件当前指向的仓库。 */
  selectedGitRepositoryForActions: string | null;
  setSelectedGitRepositoryKey: (repositoryKey: string) => void;
  setReviewFocusRequest: React.Dispatch<React.SetStateAction<WorkspaceReviewFocusRequest>>;
  /** Opens or focuses the singleton review tab.
   * 打开或聚焦单例审核页签。 */
  openReviewTab: () => void;
  openRightContextPanel: () => void;
}

/** Actions that open the right review panel on a requested source.
 * 打开右侧审核面板并切到指定来源的动作。 */
export interface ReviewFocusActions {
  /** Opens the panel on live uncommitted changes, optionally of one repository.
   * 打开面板并展示实时未提交变更，可指定仓库。 */
  openReviewChanges: (repositoryKey?: string) => void;
  /** Lets turn statistics rows open the panel on the last-turn source.
   * 让轮次统计行打开面板并切到上一轮来源。 */
  turnChangeReviewActions: TurnChangeReviewActions;
}

/** Builds the review-focus actions of the workspace shell.
 * 构建工作区外壳的审核聚焦动作。 */
export function useReviewFocusActions(inputs: ReviewFocusActionInputs): ReviewFocusActions {
  const {
    hasSelectedChatConversation,
    openReviewTab,
    openRightContextPanel,
    selectedConversationId,
    selectedGitRepositoryForActions,
    setReviewFocusRequest,
    setSelectedGitRepositoryKey,
  } = inputs;
  const focusReview = React.useCallback((repositoryKey: string | undefined, mode?: DiffReviewMode) => {
    if (!hasSelectedChatConversation) {
      return;
    }
    const nextRepositoryKey = repositoryKey ?? selectedGitRepositoryForActions;
    if (nextRepositoryKey) {
      setSelectedGitRepositoryKey(nextRepositoryKey);
    }
    openReviewTab();
    setReviewFocusRequest((current) => ({
      sequence: current.sequence + 1,
      repositoryKey: nextRepositoryKey,
      ...(mode ? { mode } : {}),
    }));
    openRightContextPanel();
  }, [hasSelectedChatConversation, openReviewTab, openRightContextPanel, selectedGitRepositoryForActions, setReviewFocusRequest, setSelectedGitRepositoryKey]);
  const openReviewChanges = React.useCallback((repositoryKey?: string) => focusReview(repositoryKey), [focusReview]);
  const turnChangeReviewActions = React.useMemo<TurnChangeReviewActions>(
    () => hasSelectedChatConversation && selectedConversationId
      ? { sessionId: selectedConversationId, openLastTurnChanges: () => focusReview(undefined, "last_turn") }
      : {},
    [focusReview, hasSelectedChatConversation, selectedConversationId],
  );
  return { openReviewChanges, turnChangeReviewActions };
}
