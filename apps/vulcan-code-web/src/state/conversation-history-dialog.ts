import { createContext } from "react";

// One conversation surface owns the history dialog and both composer entry points.
// 同一会话界面持有历史弹窗及两个输入区入口。
export interface ConversationHistoryDialogState {
  // Session currently selected in the workspace.
  // 工作区当前选中的会话。
  sessionId: string;
  // Whether the selected session's dialog is open.
  // 当前会话的弹窗是否打开。
  open: boolean;
  // Changes visibility for the selected session.
  // 切换当前会话弹窗的可见状态。
  setOpen: (open: boolean) => void;
}

// The absence of a session owner keeps composer entries unavailable.
// 没有会话所有者时，输入区入口不可用。
export const ConversationHistoryDialogContext = createContext<ConversationHistoryDialogState | null>(null);
