import React from "react";
import type { PlanArtifactActionPayload } from "@/components/conversation/conversation-plan-artifact";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import {
  planArtifactExecuteHiddenGuidance,
  planArtifactExecuteVisiblePrompt,
  planArtifactSuggestHiddenGuidance,
} from "@/components/conversation/chat-area/plan-artifact";

// UseChatAreaPlanActionsOptions carries the Plan artifact action dependencies from ChatArea.
// UseChatAreaPlanActionsOptions 承载来自 ChatArea 的计划 artifact 动作依赖。
export interface UseChatAreaPlanActionsOptions {
  // Pending Plan artifact captured from the transcript.
  // 从转录中捕获的待处理计划 artifact。
  pendingPlanArtifact: ConversationPlanArtifact | null;
  // Localized execute label used in the visible prompt.
  // 可见提示词中使用的本地化执行标签。
  executeLabel: string;
  // Current UI locale used for visible prompt formatting.
  // 用于可见提示词格式化的当前 UI 语言环境。
  locale: string;
  // Current reasoning effort forwarded to the submitted prompt.
  // 转发到提交提示词的当前推理强度。
  reasoningEffort: ChatAreaProps["workbench"]["reasoningSelection"] extends infer Selection
    ? Selection extends { selectedEffort?: infer Effort }
      ? Effort | null
      : null
    : null;
  // Handler that toggles Plan mode for the current session.
  // 切换当前会话计划模式的处理器。
  onSetSessionPlanMode: ChatAreaProps["onSetSessionPlanMode"];
  // Handler that submits the generated prompt.
  // 提交生成提示词的处理器。
  onSubmitPrompt: ChatAreaProps["onSubmitPrompt"];
}

// useChatAreaPlanActions builds stable handlers for execute/suggest/cancel Plan artifact actions.
// useChatAreaPlanActions 为执行、建议和取消计划 artifact 动作构建稳定处理器。
export function useChatAreaPlanActions(options: UseChatAreaPlanActionsOptions) {
  const handlePlanArtifactExecute = React.useCallback(async (payload: PlanArtifactActionPayload) => {
    const artifact = options.pendingPlanArtifact && options.pendingPlanArtifact.content.trim() === payload.artifact.content.trim()
      ? options.pendingPlanArtifact
      : payload.artifact;
    const disabled = await options.onSetSessionPlanMode(false);
    if (!disabled) {
      return;
    }
    const note = payload.note?.trim();
    const prompt = planArtifactExecuteVisiblePrompt(options.executeLabel, note, options.locale);
    await options.onSubmitPrompt({
      prompt,
      hiddenModelGuidance: planArtifactExecuteHiddenGuidance(artifact),
      attachments: [],
      executionMode: "build",
      reasoningEffort: options.reasoningEffort,
      modelSelection: null,
    });
  }, [options]);

  const handlePlanArtifactSuggest = React.useCallback(async (payload: PlanArtifactActionPayload) => {
    const artifact = options.pendingPlanArtifact && options.pendingPlanArtifact.content.trim() === payload.artifact.content.trim()
      ? options.pendingPlanArtifact
      : payload.artifact;
    const enabled = await options.onSetSessionPlanMode(true);
    if (!enabled) {
      return;
    }
    const note = payload.note?.trim() ?? "";
    await options.onSubmitPrompt({
      prompt: note,
      hiddenModelGuidance: planArtifactSuggestHiddenGuidance(artifact),
      attachments: [],
      executionMode: "plan",
      reasoningEffort: options.reasoningEffort,
      modelSelection: null,
    });
  }, [options]);

  const handlePlanArtifactCancel = React.useCallback(async () => {
    await options.onSetSessionPlanMode(true);
  }, [options]);

  return {
    handlePlanArtifactCancel,
    handlePlanArtifactExecute,
    handlePlanArtifactSuggest,
  };
}
