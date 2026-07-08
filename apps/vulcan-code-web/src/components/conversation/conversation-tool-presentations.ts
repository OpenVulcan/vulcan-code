import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import {
  askQuestionStatusLabel,
  hydrateAskQuestionPresentation,
  resolveAskQuestionPresentation,
} from "./tool-presentations/ask-question-presentation";
import {
  fileChangeStatusLabel,
  resolveFileChangePresentation,
} from "./tool-presentations/file-change-presentation";
import {
  commandPresentationStatusLabel,
  hydrateCommandExecutionPresentation,
  resolveCommandExecutionPresentation,
} from "./tool-presentations/command-execution-presentation";
import {
  hydrateSubagentOrchestrationPresentation,
  resolveSubagentOrchestrationPresentation,
} from "./tool-presentations/subagent-orchestration-presentation";
import type {
  ToolPresentation,
  ToolPresentationDisplayState,
  ToolPresentationResolveOptions,
} from "./tool-presentations/types";

export type {
  AskQuestionPresentation,
  AskQuestionPresentationItem,
  CommandExecutionPresentation,
  FileChangeDiffRow,
  FileChangeFilePresentation,
  FileChangeOperation,
  FileChangePresentation,
  SubagentOrchestrationAgent,
  SubagentOrchestrationPresentation,
  SubagentUsageSummary,
  ToolPresentation,
  ToolPresentationDisplayState,
  ToolPresentationResolveOptions,
} from "./tool-presentations/types";
export {
  askQuestionStatusLabel,
  commandPresentationStatusLabel,
  fileChangeStatusLabel,
  hydrateSubagentOrchestrationPresentation,
  hydrateAskQuestionPresentation,
  hydrateCommandExecutionPresentation,
};

// resolveToolPresentation builds a specialized presentation model or returns null for default rendering.
// resolveToolPresentation 构建专用展示模型；无法识别时返回 null 以回退默认渲染。
export function resolveToolPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
  options: ToolPresentationResolveOptions = {},
): ToolPresentation | null {
  return resolveSubagentOrchestrationPresentation(blocks, displayState, detailRef)
    ?? resolveAskQuestionPresentation(blocks, displayState, detailRef)
    ?? resolveFileChangePresentation(blocks, displayState, options)
    ?? resolveCommandExecutionPresentation(blocks, displayState, detailRef, options);
}
