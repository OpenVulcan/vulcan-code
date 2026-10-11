import React from "react";
import type { AssistantDockState } from "./assistant-dock-model";

/** One floating assistant: a dedicated session moved out of the session list under its own name.
 * 一个浮动助手：以独立名称移出会话列表的专职会话。 */
export interface AssistantEntry {
  /** Session id behind the assistant.
   * 助手背后的会话标识。 */
  id: string;
  name: string;
  projectId: string | null;
  projectName: string | null;
  /** Live run state from the workbench row; the event window's open turn is only refreshed on load.
   * 来自工作台行的实时运行状态；事件窗口的开放回合只在加载时刷新。 */
  running: boolean;
}

/** Commands and state shared by the top-bar button, the floating windows, and the lock bar.
 * 顶部按钮、浮动窗口与锁定条共享的命令与状态。 */
export interface AssistantDockController {
  assistants: AssistantEntry[];
  state: AssistantDockState;
  /** Top-bar button; windows minimize into it.
   * 顶部按钮；窗口最小化时收入该按钮。 */
  buttonRef: React.RefObject<HTMLButtonElement | null>;
  toggle: (assistantId: string) => void;
  activate: (windowId: string, assistantId: string) => void;
  hide: (assistantId: string) => void;
  detach: (windowId: string, assistantId: string) => void;
  merge: (windowId: string) => void;
  closeWindow: (windowId: string) => void;
  focusWindow: (windowId: string) => void;
  /** Suggested assistant name for a session: its current title.
   * 会话的建议助手名称：其当前标题。 */
  defaultName: (conversationId: string) => string;
  /** Converts a memory-locked dedicated session and shows the new assistant; rejects on service errors.
   * 转换记忆已锁定的专职会话并显示新助手；服务出错时拒绝。 */
  convert: (conversationId: string, name: string) => Promise<void>;
  /** Returns the assistant to the session list and opens it there; rejects on service errors.
   * 将助手还原到会话列表并在其中打开；服务出错时拒绝。 */
  restore: (conversationId: string) => Promise<void>;
}

/** Provided by the workbench; null outside it (tests, isolated previews).
 * 由工作台提供；工作台之外（测试、隔离预览）为 null。 */
export const AssistantDockContext = React.createContext<AssistantDockController | null>(null);

/** Returns the assistant dock, or null outside the workbench.
 * 返回助手停靠控制器；工作台之外返回 null。 */
export function useAssistantDock(): AssistantDockController | null {
  return React.useContext(AssistantDockContext);
}
