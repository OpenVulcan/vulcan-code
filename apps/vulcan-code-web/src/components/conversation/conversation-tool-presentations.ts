import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import {
  askQuestionStatusLabel,
  hydrateAskQuestionPresentation,
  resolveAskQuestionPresentation,
} from "./tool-presentations/ask-question-presentation";
import {
  fileChangeStatusLabel,
  hydrateFileAccessPresentation,
  hydrateFileChangePresentation,
  resolveFileAccessPresentation,
  resolveFileChangePresentation,
} from "./tool-presentations/file-change-presentation";
import {
  hydrateMarkdownToolPresentation,
  resolveMarkdownToolPresentation,
} from "./tool-presentations/markdown-output-presentation";
import {
  hydrateLuaExecutionPresentation,
  resolveLuaExecutionPresentation,
} from "./tool-presentations/lua-execution-presentation";
import {
  commandPresentationStatusLabel,
  hydrateCommandExecutionPresentation,
  resolveCommandExecutionPresentation,
} from "./tool-presentations/command-execution-presentation";
import {
  hydrateSubagentOrchestrationPresentation,
  resolveSubagentOrchestrationPresentation,
} from "./tool-presentations/subagent-orchestration-presentation";
import {
  resolveProviderToolExtractPresentation,
  resolveProviderToolSearchPresentation,
  resolveProviderToolStructuredPresentation,
  resolveProviderToolTextPresentation,
} from "./tool-presentations/provider-tool-presentation";
import {
  resolveGenericToolMarkdownPresentation,
  resolveNativeSemanticToolPresentation,
} from "./tool-presentations/native-semantic-tool-presentation";
import type {
  ToolPresentation,
  ToolPresentationDisplayState,
  ToolPresentationResolveOptions,
} from "./tool-presentations/types";

export type {
  AskQuestionPresentation,
  AskQuestionPresentationItem,
  CommandExecutionPresentation,
  FileAccessItemPresentation,
  FileAccessLineRangePresentation,
  FileAccessPresentation,
  FileAccessPresentationKind,
  FileChangeDiffRow,
  FileChangeFilePresentation,
  FileChangeOperation,
  FileChangePresentation,
  GenericToolMarkdownPresentation,
  LuaExecutionPresentation,
  MarkdownToolPresentation,
  MarkdownToolPresentationIcon,
  MarkdownToolSummaryTarget,
  NativeSemanticToolFamily,
  NativeSemanticToolId,
  NativeSemanticToolPresentation,
  ProviderToolExtractPresentation,
  ProviderToolExtractPresentationFailure,
  ProviderToolExtractPresentationResult,
  ProviderToolSearchPresentation,
  ProviderToolSearchPresentationResult,
  ProviderToolStructuredPresentation,
  ProviderToolTextPresentation,
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
  hydrateFileAccessPresentation,
  hydrateFileChangePresentation,
  hydrateMarkdownToolPresentation,
  hydrateLuaExecutionPresentation,
  hydrateSubagentOrchestrationPresentation,
  hydrateAskQuestionPresentation,
  hydrateCommandExecutionPresentation,
};

// resolveToolPresentation builds a specialized presentation model and always ends with an inline unknown-tool card.
// resolveToolPresentation 构建专用展示模型，并始终以未知工具内联卡片收口。
export function resolveToolPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
  options: ToolPresentationResolveOptions = {},
): ToolPresentation {
  return resolveSubagentOrchestrationPresentation(blocks, displayState, detailRef)
    ?? resolveProviderToolTextPresentation(blocks, displayState)
    ?? resolveProviderToolStructuredPresentation(blocks, displayState)
    ?? resolveProviderToolExtractPresentation(blocks, displayState)
    ?? resolveProviderToolSearchPresentation(blocks, displayState)
    ?? resolveAskQuestionPresentation(blocks, displayState, detailRef)
    ?? resolveFileAccessPresentation(blocks, displayState, options, detailRef)
    ?? resolveFileChangePresentation(blocks, displayState, options, detailRef)
    ?? resolveLuaExecutionPresentation(blocks, displayState, detailRef)
    ?? resolveMarkdownToolPresentation(blocks, displayState, detailRef)
    ?? resolveCommandExecutionPresentation(blocks, displayState, detailRef, options)
    ?? resolveNativeSemanticToolPresentation(blocks, displayState, detailRef, options)
    ?? resolveGenericToolMarkdownPresentation(blocks, displayState, detailRef);
}
