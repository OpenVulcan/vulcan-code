import React from "react";
import { conversationIsRunning } from "@/components/conversation/conversation-runtime";
import { convertConversationToAssistant, restoreAssistantConversation } from "@/service/dedicated-session-service";
import type { ConversationSummary, WorkbenchSnapshot } from "@/types";
import type { AssistantDockController, AssistantEntry } from "./assistant-dock-context";
import {
  EMPTY_ASSISTANT_DOCK,
  activateAssistant,
  closeAssistantWindow,
  detachAssistant,
  focusWindow,
  hideAssistant,
  mergeAssistantWindows,
  retainAssistants,
  showAssistant,
  toggleAssistant,
} from "./assistant-dock-model";

/** Workbench hooks the assistant dock needs.
 * 助手停靠所需的工作台接口。 */
export interface AssistantDockControllerOptions {
  workbench: WorkbenchSnapshot;
  /** Applies a committed service snapshot.
   * 应用已提交的服务快照。 */
  applySnapshot: (snapshot: WorkbenchSnapshot) => void;
  /** Moves the main view off a session that just became an assistant.
   * 将主视图从刚转为助手的会话移开。 */
  onLeaveConverted: (conversation: ConversationSummary) => void;
  /** Opens a session that was just restored from an assistant.
   * 打开刚由助手还原的会话。 */
  onRestored: (conversationId: string) => void;
}

/** Owns the floating-assistant windows and the convert/restore transitions.
 * 持有浮动助手窗口及转换/还原过程。 */
export function useAssistantDockController(options: AssistantDockControllerOptions): AssistantDockController {
  const { applySnapshot, onLeaveConverted, onRestored, workbench } = options;
  const buttonRef = React.useRef<HTMLButtonElement | null>(null);
  const [state, setState] = React.useState(EMPTY_ASSISTANT_DOCK);
  const assistants = React.useMemo<AssistantEntry[]>(() => {
    const projectNames = new Map(workbench.projects.map((project) => [project.id, project.name]));
    return workbench.conversations
      .filter((conversation) => Boolean(conversation.assistantName))
      .map((conversation) => ({
        id: conversation.id,
        name: conversation.assistantName ?? conversation.title,
        projectId: conversation.projectId ?? null,
        projectName: conversation.projectId ? projectNames.get(conversation.projectId) ?? null : null,
        running: conversationIsRunning(conversation),
      }));
  }, [workbench.conversations, workbench.projects]);
  // Windows drop assistants restored elsewhere or deleted with their project.
  // 窗口移除在别处还原或随项目删除的助手。
  React.useEffect(() => {
    const ids = new Set(assistants.map((assistant) => assistant.id));
    setState((current) => retainAssistants(current, ids));
  }, [assistants]);
  const convert = React.useCallback(async (conversationId: string, name: string) => {
    const conversation = workbench.conversations.find((candidate) => candidate.id === conversationId);
    const snapshot = await convertConversationToAssistant(conversationId, name);
    applySnapshot(snapshot);
    if (conversation) onLeaveConverted(conversation);
    setState((current) => showAssistant(current, conversationId));
  }, [applySnapshot, onLeaveConverted, workbench.conversations]);
  const restore = React.useCallback(async (conversationId: string) => {
    const snapshot = await restoreAssistantConversation(conversationId);
    setState((current) => hideAssistant(current, conversationId));
    applySnapshot(snapshot);
    onRestored(conversationId);
  }, [applySnapshot, onRestored]);
  return React.useMemo<AssistantDockController>(() => ({
    assistants,
    state,
    buttonRef,
    toggle: (assistantId) => setState((current) => toggleAssistant(current, assistantId)),
    activate: (windowId, assistantId) => setState((current) => activateAssistant(current, windowId, assistantId)),
    hide: (assistantId) => setState((current) => hideAssistant(current, assistantId)),
    detach: (windowId, assistantId) => setState((current) => detachAssistant(current, windowId, assistantId)),
    merge: (windowId) => setState((current) => mergeAssistantWindows(current, windowId)),
    closeWindow: (windowId) => setState((current) => closeAssistantWindow(current, windowId)),
    focusWindow: (windowId) => setState((current) => focusWindow(current, windowId)),
    defaultName: (conversationId) => workbench.conversations
      .find((conversation) => conversation.id === conversationId)?.title ?? "",
    convert,
    restore,
  }), [assistants, convert, restore, state, workbench.conversations]);
}
