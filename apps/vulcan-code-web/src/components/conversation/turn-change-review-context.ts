import * as React from "react";

/** Workspace actions the turn statistics row may use for its change summary.
 * 轮次统计行的变更汇总可使用的工作区动作。 */
export interface TurnChangeReviewActions {
  /** Conversation the review panel follows; absent where no review panel exists.
   * 审核面板跟随的会话；不存在审核面板的界面中缺省。 */
  sessionId?: string;
  /** Opens the review panel on the last-turn source of that conversation.
   * 打开审核面板并切到该会话的上一轮来源。 */
  openLastTurnChanges?: () => void;
}

/** Provided by the workspace shell around the main transcript.
 * 由工作区外壳在主转录外层提供。 */
export const TurnChangeReviewActionsContext = React.createContext<TurnChangeReviewActions>({});

/** The Turn a transcript's last-turn review currently resolves to.
 * 某个转录的上一轮审核当前解析到的 Turn。 */
export interface TurnChangeReviewTarget {
  /** Conversation owning the transcript.
   * 持有该转录的会话。 */
  sessionId: string;
  /** Resolved Turn, or null when the window holds none.
   * 解析出的 Turn；窗口中没有 Turn 时为空。 */
  turnId: number | null;
}

/** Provided by the transcript that owns the event window.
 * 由持有事件窗口的转录提供。 */
export const TurnChangeReviewTargetContext = React.createContext<TurnChangeReviewTarget | null>(null);

/** Returns the action opening the last-turn review when the given Turn is what that review shows.
 * 当给定 Turn 正是上一轮审核所展示的 Turn 时，返回打开该审核的动作。
 *
 * @param turnId Turn of the statistics row asking.
 *               发起询问的统计行所属 Turn。
 * @returns the open action, or undefined when the review panel would show something else.
 *          打开动作；审核面板会展示其他内容时为 undefined。 */
export function useOpenTurnChangeReview(turnId: number | null): (() => void) | undefined {
  const actions = React.useContext(TurnChangeReviewActionsContext);
  const target = React.useContext(TurnChangeReviewTargetContext);
  return turnId !== null
    && target !== null
    && target.turnId === turnId
    && target.sessionId === actions.sessionId
    ? actions.openLastTurnChanges
    : undefined;
}
