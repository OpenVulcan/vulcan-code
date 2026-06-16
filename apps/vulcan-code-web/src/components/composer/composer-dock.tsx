import React from "react";
import { ArrowDown } from "lucide-react";
import { type ModelPickerChoice } from "@/components/composer/model-picker-control";
import {
  type ComposerAiCapabilityConfigurationSave,
  type ComposerSlashCommandResult,
  type ComposerSubmission,
} from "@/components/composer/composer-model";
import { ComposerPanel } from "@/components/composer/composer-panel";
import type {
  ComposerRuntimeCue,
  ComposerTraceCallback,
} from "@/components/composer/composer-control-types";
import { Button } from "@/components/ui/button";
import { ShinyText } from "@/components/ui/shiny-text";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type {
  AiCapabilitySummary,
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ContextUsageSnapshot,
  PendingPromptSummary,
  ProviderSummary,
  ReasoningSelectionSummary,
} from "@/types";

export { composerAttachmentMedia } from "@/components/composer/composer-attachment-media";

// ComposerDock renders composer-related floating controls.
// ComposerDock 渲染输入区相关的浮动控件。
export function ComposerDock(props: {
  // AI capability route summaries used by the composer capability tree.
  // 输入区能力树使用的 AI 能力路由摘要。
  aiCapabilities: AiCapabilitySummary[];
  // Default AI capability route summaries used to reset the session draft.
  // 用于重置会话草稿的默认 AI 能力路由摘要。
  defaultAiCapabilities: AiCapabilitySummary[];
  // Attachment capability gates used by composer attachment controls.
  // 输入区附件控件使用的附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[];
  // Owning project id used for attachment staging.
  // 附件暂存使用的所属项目标识。
  projectId: string | null;
  // Owning conversation id used for attachment staging.
  // 附件暂存使用的所属会话标识。
  conversationId: string | null;
  // Context usage snapshot shown by the composer orb.
  // 输入区圆盘展示的上下文占用快照。
  contextUsage: ContextUsageSnapshot | null;
  // Whether the dock is visually centered for an empty conversation.
  // 输入区浮层是否在空会话中居中显示。
  emptyMode: boolean;
  // Slash commands projected by the shared workbench service.
  // 共享工作台服务投影出的 slash 指令。
  slashCommands: ComposerSlashCommandSummary[];
  // Visible pending prompts retained for the selected conversation.
  // 当前选中会话保留的可见待发送提示词。
  pendingPrompts: PendingPromptSummary[];
  // Whether the selected conversation has an active backend run.
  // 当前选中会话是否存在活跃后端运行。
  isConversationRunning: boolean;
  // Whether streaming thinking should use the floating thinking surface.
  // 流式思考是否使用浮动思考面板。
  thinkingOverlayEnabled: boolean;
  // showScrollToBottom controls whether the bottom-jump button is visible.
  // showScrollToBottom 控制是否显示跳转到底部按钮。
  showScrollToBottom: boolean;
  // onScrollToBottom moves the current conversation to its latest content.
  // onScrollToBottom 将当前会话移动到最新内容。
  onScrollToBottom: () => void;
  // Providers exposed by the shared service API for model picking.
  // 共享 service API 暴露给模型选择器的 Provider 列表。
  providers: ProviderSummary[];
  // onSelectWorkbenchModel persists a selected workbench model.
  // onSelectWorkbenchModel 持久化选中的工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // onSelectWorkbenchReasoningEffort persists a selected reasoning effort.
  // onSelectWorkbenchReasoningEffort 持久化选中的思考档位。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // onSlashCommand routes one projected slash command.
  // onSlashCommand 路由单个投影 slash 指令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Reports the measured floating composer stack height.
  // 上报浮动输入区堆栈的实测高度。
  onComposerDockHeightChange: (height: number) => void;
  // onSubmitPrompt sends one composer prompt through the workbench runtime.
  // onSubmitPrompt 通过工作台运行时发送一条输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // onSaveAiCapabilityConfiguration persists one session-scoped capability draft.
  // onSaveAiCapabilityConfiguration 持久化单个会话级能力草稿。
  onSaveAiCapabilityConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
  // onStopConversationRun stops the selected backend conversation stream.
  // onStopConversationRun 停止当前选中的后端会话流。
  onStopConversationRun: () => Promise<boolean>;
  // Stages one browser file through the caller-owned service boundary.
  // 通过调用方持有的服务边界暂存单个浏览器文件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
  // Converts a local media path into a browser-safe resource URL.
  // 将本地媒体路径转换为浏览器安全资源地址。
  resolveMediaSourceUrl: (path: string) => string;
  // Optional trace callback for local UI diagnostics.
  // 用于本地 UI 诊断的可选埋点回调。
  onTrace?: ComposerTraceCallback;
  // Restores one pending prompt into the composer.
  // 将单条待发送提示词恢复到输入区。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Routes one pending prompt into the shared insertion queue.
  // 将单条待发送提示词路由进共享引导队列。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one pending prompt from the visible queue.
  // 从可见队列移除单条待发送提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Persists the pending prompt order.
  // 持久化待发送提示词顺序。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Updates the WebUI-only thinking display preference.
  // 更新仅 WebUI 使用的思考展示偏好。
  onThinkingOverlayEnabledChange: (enabled: boolean) => void;
  // Backend-resolved reasoning selector view for the active provider and model.
  // 后端根据当前激活 Provider 与模型解析出的思考选择器视图。
  reasoningSelection: ReasoningSelectionSummary | null;
  // runtimeCue shows the current session in-progress activity near the composer.
  // runtimeCue 在输入区附近显示当前会话的进行中活动。
  runtimeCue: ComposerRuntimeCue | null;
}) {
  // dockRef measures the full composer stack, including pending queue rows and runtime cues.
  // dockRef 测量完整输入区堆栈，包括待发送队列行与运行提示。
  const dockRef = React.useRef<HTMLDivElement | null>(null);
  const onComposerDockHeightChange = props.onComposerDockHeightChange;

  React.useLayoutEffect(() => {
    const element = dockRef.current;
    if (!element) {
      return;
    }
    const reportHeight = () => onComposerDockHeightChange(Math.ceil(element.getBoundingClientRect().height));
    reportHeight();
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(reportHeight);
    observer.observe(element);
    return () => observer.disconnect();
  }, [onComposerDockHeightChange]);

  return (
    <div className={cn("composer-dock", props.emptyMode && "empty-mode")} ref={dockRef}>
      {props.showScrollToBottom ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label="滚动到底部"
              className="scroll-to-bottom-button"
              onClick={props.onScrollToBottom}
              size="icon"
              type="button"
              variant="outline"
            >
              <ArrowDown size={18} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">滚动到底部</TooltipContent>
        </Tooltip>
      ) : null}
      {props.runtimeCue ? (
        <div className="composer-runtime-cue" aria-live="polite" aria-busy="true">
          <ShinyText text={props.runtimeCue.text} />
        </div>
      ) : null}
      <ComposerPanel
        aiCapabilities={props.aiCapabilities}
        defaultAiCapabilities={props.defaultAiCapabilities}
        attachmentCapabilities={props.attachmentCapabilities}
        projectId={props.projectId}
        conversationId={props.conversationId}
        contextUsage={props.contextUsage}
        slashCommands={props.slashCommands}
        pendingPrompts={props.pendingPrompts}
        isConversationRunning={props.isConversationRunning}
        thinkingOverlayEnabled={props.thinkingOverlayEnabled}
        providers={props.providers}
        onSelectWorkbenchModel={props.onSelectWorkbenchModel}
        onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
        onSlashCommand={props.onSlashCommand}
        onSubmitPrompt={props.onSubmitPrompt}
        onSaveAiCapabilityConfiguration={props.onSaveAiCapabilityConfiguration}
        onStopConversationRun={props.onStopConversationRun}
        onStageAttachmentBytes={props.onStageAttachmentBytes}
        resolveMediaSourceUrl={props.resolveMediaSourceUrl}
        onTrace={props.onTrace}
        onEditPendingPrompt={props.onEditPendingPrompt}
        onInsertPendingPrompt={props.onInsertPendingPrompt}
        onRemovePendingPrompt={props.onRemovePendingPrompt}
        onReorderPendingPrompts={props.onReorderPendingPrompts}
        onThinkingOverlayEnabledChange={props.onThinkingOverlayEnabledChange}
        reasoningSelection={props.reasoningSelection}
      />
    </div>
  );
}
