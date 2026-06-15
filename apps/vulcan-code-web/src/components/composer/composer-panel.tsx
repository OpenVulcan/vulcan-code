import React from "react";
import { Brain, Send, Square } from "lucide-react";
import { toast } from "sonner";
import {
  ModelPickerControl,
  modelPickerChoiceKey,
  modelPickerChoicesFromProviders,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import {
  aiCapabilitySignature,
  composerAttachmentAccept,
  composerSlashCommandMatchesInput,
  defaultAiCapabilityTargetOrder,
  defaultDisabledAiCapabilityKinds,
  defaultDisabledAiCapabilityTargetSelectors,
  exactComposerSlashCommand,
  hasBlockingComposerPopup,
  isComposerPrintableKey,
  isComposerTextEntryElement,
  serializeAiCapabilityTargetOrder,
  type AiCapabilityKind,
  type ComposerSelectOption,
  type ComposerSlashCommandResult,
  type ComposerSubmission,
} from "@/components/composer/composer-model";
import {
  ComposerAiCapabilityControl,
  ComposerAttachmentMenu,
  ComposerAttachmentStrip,
  ComposerSelectControl,
  ComposerSlashCommandPanel,
  ContextUsageOrb,
  PendingPromptQueueStrip,
  type ComposerAttachmentPreviewMedia,
} from "@/components/composer/composer-controls";
import { composerAttachmentMedia } from "@/components/composer/composer-attachment-media";
import { useComposerAttachments } from "@/components/composer/composer-attachments";
import { GeneratedMediaPlayerWindow, type GeneratedMediaAttachment } from "@/components/media/generated-media";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { useI18n } from "@/i18n";
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
import type { ComposerTraceCallback } from "@/components/composer/composer-control-types";

// composerShortcutHint returns a platform-specific keyboard hint for the composer placeholder.
// composerShortcutHint 返回输入区 placeholder 使用的平台化快捷键提示。
function composerShortcutHint(baseHint: string): string {
  const platformText = typeof navigator === "undefined"
    ? ""
    : `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (platformText.includes("mac")) {
    return baseHint;
  }
  return baseHint;
}

// ComposerPanel renders the first-version message input surface.
// ComposerPanel 渲染第一版消息输入区域。
export function ComposerPanel(props: {
  // AI capability route summaries used by the quick capability tree.
  // 快捷能力树使用的 AI 能力路由摘要。
  aiCapabilities: AiCapabilitySummary[];
  // Attachment capability gates used by add/drop/paste.
  // 添加、拖拽与粘贴使用的附件能力门控。
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
  // Providers exposed by the shared service API for model picking.
  // 共享 service API 暴露给模型选择器的 Provider 列表。
  providers: ProviderSummary[];
  // Persists one selected workbench model.
  // 持久化单个选中的工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Persists one selected workbench reasoning effort.
  // 持久化单个选中的工作台思考档位。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Routes one projected slash command through the workbench shell.
  // 通过工作台外壳路由单个投影 slash 指令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Sends one validated composer submission through the caller runtime.
  // 通过调用方运行时发送一条已校验输入区提交。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Stops the selected backend conversation stream.
  // 停止当前选中的后端会话流。
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
}) {
  // t translates stable composer labels while model names remain provider data.
  // t 翻译稳定输入区标签，模型名称仍保留为 provider 数据。
  const { locale, t } = useI18n();
  // draftInput stores the current unsent composer text.
  // draftInput 保存当前尚未发送的输入区文本。
  const [draftInput, setDraftInput] = React.useState("");
  // slashHighlightedIndex stores the highlighted command row while the slash panel is visible.
  // slashHighlightedIndex 保存 slash 面板可见时的高亮指令行。
  const [slashHighlightedIndex, setSlashHighlightedIndex] = React.useState(0);
  // slashDismissedInput stores the exact input dismissed by Escape.
  // slashDismissedInput 保存被 Escape 隐藏的精确输入内容。
  const [slashDismissedInput, setSlashDismissedInput] = React.useState<string | null>(null);
  // modelPickerOpenSignal opens the existing model picker from /model.
  // modelPickerOpenSignal 通过 /model 打开现有模型筛选器。
  const [modelPickerOpenSignal, setModelPickerOpenSignal] = React.useState(0);
  // reasoningPickerOpenSignal opens the existing reasoning selector from /reasoning.
  // reasoningPickerOpenSignal 通过 /reasoning 打开现有思考档位选择器。
  const [reasoningPickerOpenSignal, setReasoningPickerOpenSignal] = React.useState(0);
  // composerMediaPlayer stores an attachment preview opened from the composer strip.
  // composerMediaPlayer 保存从输入区附件条打开的附件预览。
  const [composerMediaPlayer, setComposerMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  // composerTextareaRef focuses the prompt field from global workspace typing.
  // composerTextareaRef 用于从工作台全局输入聚焦提示词输入框。
  const composerTextareaRef = React.useRef<HTMLTextAreaElement | null>(null);
  // composerAttachments owns attachment filtering, picker, paste, drag-drop, and staging behavior.
  // composerAttachments 管理附件过滤、选择器、粘贴、拖拽投放与暂存行为。
  const composerAttachments = useComposerAttachments({
    attachmentCapabilities: props.attachmentCapabilities,
    projectId: props.projectId,
    conversationId: props.conversationId,
    onStageAttachmentBytes: props.onStageAttachmentBytes,
  });
  const {
    attachments,
    setAttachments,
    isAttachmentDragOver,
    setIsAttachmentDragOver,
    attachmentFileInputRef,
    enabledAttachmentCapabilities,
    attachmentInputAccept,
    openAttachmentPicker,
    handleAttachmentInputChange,
    handleComposerPaste,
    handleComposerDragOver,
    handleComposerDrop,
    removeComposerAttachment,
  } = composerAttachments;
  // executionMode stores the local preview execution-mode selection.
  // executionMode 保存本地预览用的执行模式选择。
  const [executionMode, setExecutionMode] = React.useState("build");
  // modelChoice stores the selected provider/model key shown by the shared model picker.
  // modelChoice 保存共享模型筛选器展示的 provider/model 选择键。
  const [modelChoice, setModelChoice] = React.useState("");
  // disabledAiCapabilityKinds stores current-message quick-toggle disables.
  // disabledAiCapabilityKinds 保存当前消息级快捷开关禁用项。
  const [disabledAiCapabilityKinds, setDisabledAiCapabilityKinds] = React.useState<Set<AiCapabilityKind>>(() => new Set());
  // disabledAiCapabilityTargetSelectors stores current-message target-route disables.
  // disabledAiCapabilityTargetSelectors 保存当前消息级目标路由禁用项。
  const [disabledAiCapabilityTargetSelectors, setDisabledAiCapabilityTargetSelectors] = React.useState<Set<string>>(() => new Set());
  // aiCapabilityTargetOrder stores current-message target route ordering.
  // aiCapabilityTargetOrder 保存当前消息级目标路由顺序。
  const [aiCapabilityTargetOrder, setAiCapabilityTargetOrder] = React.useState<Map<AiCapabilityKind, string[]>>(() => new Map());
  // reasoningEffort stores the backend-synchronized reasoning-effort selection.
  // reasoningEffort 保存与后端同步的思考档位选择。
  const [reasoningEffort, setReasoningEffort] = React.useState("x-high");
  // isComposerRunning stores the local send/stop preview state.
  // isComposerRunning 保存输入区发送/停止的本地预览状态。
  const [isComposerRunning, setIsComposerRunning] = React.useState(false);
  // availableModelChoices flattens provider data into selectable workbench models.
  // availableModelChoices 将 Provider 数据扁平化为可选择的工作台模型。
  const availableModelChoices = React.useMemo(
    () => modelPickerChoicesFromProviders(props.providers),
    [props.providers],
  );
  // activeModelChoiceKey follows the service-reported active model when available.
  // activeModelChoiceKey 在可用时跟随 service 上报的激活模型。
  const activeModelChoiceKey = React.useMemo(() => {
    const activeChoice = availableModelChoices.find((choice) => choice.active) ?? availableModelChoices[0];
    return modelPickerChoiceKey(activeChoice);
  }, [availableModelChoices]);
  // capabilitySignature prevents unrelated workbench streaming snapshots from resetting prompt overrides.
  // capabilitySignature 防止无关的工作台流式快照重置提示词级能力覆盖。
  const capabilitySignature = React.useMemo(
    () => aiCapabilitySignature(props.aiCapabilities),
    [props.aiCapabilities],
  );

  React.useEffect(() => {
    if (!activeModelChoiceKey) {
      return;
    }
    const currentStillAvailable = availableModelChoices.some(
      (choice) => modelPickerChoiceKey(choice) === modelChoice,
    );
    if (!modelChoice || !currentStillAvailable) {
      setModelChoice(activeModelChoiceKey);
    }
  }, [activeModelChoiceKey, availableModelChoices, modelChoice]);

  React.useEffect(() => {
    setDisabledAiCapabilityKinds(defaultDisabledAiCapabilityKinds(props.aiCapabilities));
    setDisabledAiCapabilityTargetSelectors(defaultDisabledAiCapabilityTargetSelectors(props.aiCapabilities));
    setAiCapabilityTargetOrder(defaultAiCapabilityTargetOrder(props.aiCapabilities));
  }, [capabilitySignature]);
  // executionModeOptions mirrors iced ExecutionMode::ALL using current language labels.
  // executionModeOptions 使用当前语言标签对齐 iced 的 ExecutionMode::ALL。
  const executionModeOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "build", label: t("composer.mode.execute") },
    { value: "plan", label: t("composer.mode.plan") },
  ], [t]);
  // fallbackReasoningEffortOptions is only used when no backend selector view is available.
  // fallbackReasoningEffortOptions 仅在没有后端选择器视图时使用。
  const fallbackReasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "off", label: t("composer.reasoning.off") },
    { value: "low", label: t("composer.reasoning.low") },
    { value: "medium", label: t("composer.reasoning.medium") },
    { value: "high", label: t("composer.reasoning.high") },
    { value: "x-high", label: t("composer.reasoning.max") },
  ], [t]);
  // reasoningEffortOptions uses the shared backend selector view before falling back to local preview labels.
  // reasoningEffortOptions 优先使用共享后端选择器视图，再回退到本地预览标签。
  const reasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => {
    if (props.reasoningSelection?.options.length) {
      return props.reasoningSelection.options.map((option) => ({
        value: option.value,
        label: option.label,
      }));
    }
    return fallbackReasoningEffortOptions;
  }, [fallbackReasoningEffortOptions, props.reasoningSelection]);
  // serviceSelectedReasoningEffort mirrors the active backend-selected effort when available.
  // serviceSelectedReasoningEffort 在可用时镜像后端选中的生效思考档位。
  const serviceSelectedReasoningEffort = props.reasoningSelection?.selectedEffort;

  React.useEffect(() => {
    if (serviceSelectedReasoningEffort && serviceSelectedReasoningEffort !== reasoningEffort) {
      setReasoningEffort(serviceSelectedReasoningEffort);
      return;
    }
    if (!reasoningEffortOptions.some((option) => option.value === reasoningEffort)) {
      setReasoningEffort(reasoningEffortOptions[0]?.value ?? "medium");
    }
  }, [reasoningEffort, reasoningEffortOptions, serviceSelectedReasoningEffort]);
  // placeholder describes the platform-specific send and newline keyboard behavior.
  // placeholder 描述平台相关的发送与换行键盘行为。
  const placeholder = React.useMemo(() => {
    return t("composer.placeholder").replace("{shortcut}", composerShortcutHint(t("composer.shortcutHint")));
  }, [t]);
  // visibleSlashCommands mirrors iced prefix filtering for the slash command panel.
  // visibleSlashCommands 对齐 iced 前缀过滤逻辑，供 slash 指令面板展示。
  const visibleSlashCommands = React.useMemo(() => {
    if (!draftInput.startsWith("/") || slashDismissedInput === draftInput) {
      return [];
    }
    return props.slashCommands.filter((command) => composerSlashCommandMatchesInput(command, draftInput));
  }, [draftInput, props.slashCommands, slashDismissedInput]);
  // slashPanelVisible controls whether the current slash input has at least one match.
  // slashPanelVisible 控制当前 slash 输入是否存在至少一个匹配项。
  const slashPanelVisible = visibleSlashCommands.length > 0;
  // highlightedSlashCommand resolves the currently highlighted command row.
  // highlightedSlashCommand 解析当前高亮的指令行。
  const highlightedSlashCommand = visibleSlashCommands[Math.min(slashHighlightedIndex, Math.max(visibleSlashCommands.length - 1, 0))] ?? null;
  // exactSlashCommand mirrors iced exact trigger activation before the highlighted row.
  // exactSlashCommand 对齐 iced 精确触发优先于高亮项的激活规则。
  const exactSlashCommand = React.useMemo(
    () => exactComposerSlashCommand(visibleSlashCommands, draftInput),
    [draftInput, visibleSlashCommands],
  );

  React.useEffect(() => {
    setSlashHighlightedIndex((current) => {
      if (!visibleSlashCommands.length) {
        return 0;
      }
      return Math.min(current, visibleSlashCommands.length - 1);
    });
  }, [visibleSlashCommands.length]);

  // handleDraftInputChange tracks typed content and re-enables slash matching after edits.
  // handleDraftInputChange 跟踪输入内容，并在编辑后重新启用 slash 匹配。
  const handleDraftInputChange = React.useCallback((value: string) => {
    setDraftInput(value);
    if (slashDismissedInput !== null && value !== slashDismissedInput) {
      setSlashDismissedInput(null);
    }
  }, [slashDismissedInput]);

  React.useEffect(() => {
    // handleGlobalWorkspaceTyping redirects ordinary workspace typing into the composer.
    // handleGlobalWorkspaceTyping 将工作台中的普通键入重定向到输入区。
    const handleGlobalWorkspaceTyping = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !isComposerPrintableKey(event)) {
        return;
      }
      const textarea = composerTextareaRef.current;
      if (!textarea || document.activeElement === textarea) {
        return;
      }
      if (hasBlockingComposerPopup() || isComposerTextEntryElement(document.activeElement)) {
        return;
      }
      event.preventDefault();
      textarea.focus({ preventScroll: true });
      setDraftInput((current) => `${current}${event.key}`);
      if (slashDismissedInput !== null) {
        setSlashDismissedInput(null);
      }
    };
    document.addEventListener("keydown", handleGlobalWorkspaceTyping);
    return () => document.removeEventListener("keydown", handleGlobalWorkspaceTyping);
  }, [slashDismissedInput]);

  // activateSlashCommand dispatches one selected command and applies its UI route result.
  // activateSlashCommand 分派单个选中的指令并应用其 UI 路由结果。
  const activateSlashCommand = React.useCallback((command: ComposerSlashCommandSummary) => {
    const result = props.onSlashCommand(command);
    if (!result.handled) {
      return;
    }
    setDraftInput("");
    setSlashDismissedInput(null);
    setSlashHighlightedIndex(0);
    if (result.replacementText !== undefined) {
      setDraftInput(result.replacementText);
    }
    if (result.openTarget === "model") {
      setModelPickerOpenSignal((current) => current + 1);
    }
    if (result.openTarget === "reasoning") {
      setReasoningPickerOpenSignal((current) => current + 1);
    }
  }, [props]);

  // handleComposerSend validates input and forwards it to the real backend runtime.
  // handleComposerSend 校验输入并转发到真实后端运行时。
  const handleComposerSend = React.useCallback(async () => {
    const prompt = draftInput.trim();
    const hasSubmittableContent = Boolean(prompt || attachments.length > 0);
    props.onTrace?.("composer.send_or_stop", {
      isConversationRunning: props.isConversationRunning,
      hasSubmittableContent,
      promptChars: prompt.length,
      attachmentCount: attachments.length,
    });
    if (props.isConversationRunning && !hasSubmittableContent) {
      const stopped = await props.onStopConversationRun();
      if (!stopped) {
        toast.error("停止当前会话失败。");
      }
      return;
    }
    if (isComposerRunning) {
      return;
    }
    if (!hasSubmittableContent) {
      toast.info("请输入任务指令。");
      return;
    }
    // submittedInput preserves the exact textarea content for a failure-only restore.
    // submittedInput 保存精确输入内容，仅在发送失败时用于恢复。
    const submittedInput = draftInput;
    // submittedAttachments preserves staged files for a failure-only restore.
    // submittedAttachments 保存已暂存文件，仅在发送失败时用于恢复。
    const submittedAttachments = attachments;
    setDraftInput("");
    setAttachments([]);
    setIsComposerRunning(true);
    try {
      const sent = await props.onSubmitPrompt({
        prompt,
        attachments: submittedAttachments,
        executionMode,
        reasoningEffort,
        disabledAiCapabilityKinds: Array.from(disabledAiCapabilityKinds),
        disabledAiCapabilityTargetSelectors: Array.from(disabledAiCapabilityTargetSelectors),
        aiCapabilityTargetOrderOverrides: serializeAiCapabilityTargetOrder(aiCapabilityTargetOrder),
      });
      if (!sent) {
        setDraftInput((current) => current || submittedInput);
        setAttachments((current) => current.length ? current : submittedAttachments);
      }
    } finally {
      setIsComposerRunning(false);
    }
  }, [aiCapabilityTargetOrder, attachments, disabledAiCapabilityKinds, disabledAiCapabilityTargetSelectors, draftInput, executionMode, isComposerRunning, props, reasoningEffort]);

  // handleComposerStop stops the active backend conversation without consuming draft text.
  // handleComposerStop 在不消费草稿文本的前提下停止活跃后端会话。
  const handleComposerStop = React.useCallback(async () => {
    props.onTrace?.("composer.stop", {
      isConversationRunning: props.isConversationRunning,
      draftChars: draftInput.trim().length,
      attachmentCount: attachments.length,
    });
    const stopped = await props.onStopConversationRun();
    if (!stopped) {
      toast.error("停止当前会话失败。");
    }
  }, [attachments.length, draftInput, props]);

  // handlePendingPromptEdit restores one queued prompt into the composer draft.
  // handlePendingPromptEdit 将单条排队提示词恢复到输入区草稿。
  const handlePendingPromptEdit = React.useCallback(async (entryId: string) => {
    props.onTrace?.("pending_prompt.edit", { entryId });
    const entry = await props.onEditPendingPrompt(entryId);
    if (!entry) {
      return;
    }
    setDraftInput(entry.prompt);
    setAttachments(entry.attachments ?? []);
    window.requestAnimationFrame(() => composerTextareaRef.current?.focus({ preventScroll: true }));
  }, [props]);

  // handleAiCapabilityToggle updates the current-message disabled capability set.
  // handleAiCapabilityToggle 更新当前消息级禁用能力集合。
  const handleAiCapabilityToggle = React.useCallback((kind: AiCapabilityKind, enabled: boolean) => {
    setDisabledAiCapabilityKinds((current) => {
      const next = new Set(current);
      if (enabled) {
        next.delete(kind);
      } else {
        next.add(kind);
      }
      return next;
    });
  }, []);

  // handleAiCapabilityTargetToggle updates the current-message disabled target selector set.
  // handleAiCapabilityTargetToggle 更新当前消息级禁用目标选择器集合。
  const handleAiCapabilityTargetToggle = React.useCallback((targetSelector: string, enabled: boolean) => {
    setDisabledAiCapabilityTargetSelectors((current) => {
      const next = new Set(current);
      if (enabled) {
        next.delete(targetSelector);
      } else {
        next.add(targetSelector);
      }
      return next;
    });
  }, []);

  // handleAiCapabilityTargetReorder replaces the prompt-level route order for one capability.
  // handleAiCapabilityTargetReorder 替换单个能力的提示词级路由顺序。
  const handleAiCapabilityTargetReorder = React.useCallback((kind: AiCapabilityKind, targetSelectors: string[]) => {
    setAiCapabilityTargetOrder((current) => {
      const next = new Map(current);
      const currentOrder = next.get(kind) ?? [];
      if (currentOrder.length === targetSelectors.length && currentOrder.every((selector, index) => selector === targetSelectors[index])) {
        return current;
      }
      next.set(kind, targetSelectors);
      return next;
    });
  }, []);

  // handleModelChoiceChange applies one model choice locally and through the shared backend.
  // handleModelChoiceChange 在本地与共享后端应用单个模型选择。
  const handleModelChoiceChange = React.useCallback(async (choice: ModelPickerChoice) => {
    const previousChoice = modelChoice;
    const nextChoice = modelPickerChoiceKey(choice);
    setModelChoice(nextChoice);
    const ok = await props.onSelectWorkbenchModel(choice);
    if (!ok) {
      setModelChoice(previousChoice);
    }
  }, [modelChoice, props]);

  // handleReasoningEffortChange applies one backend-routed reasoning selection with rollback.
  // handleReasoningEffortChange 应用单个后端路由的思考档位选择并支持失败回滚。
  const handleReasoningEffortChange = React.useCallback(async (value: string) => {
    const previousEffort = reasoningEffort;
    setReasoningEffort(value);
    const ok = await props.onSelectWorkbenchReasoningEffort(value);
    if (!ok) {
      setReasoningEffort(previousEffort);
    }
  }, [props, reasoningEffort]);

  // handleComposerKeyDown maps Enter to send while keeping Shift+Enter as a newline.
  // handleComposerKeyDown 将 Enter 映射为发送，并保留 Shift+Enter 换行。
  const handleComposerKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (slashPanelVisible) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setSlashHighlightedIndex((current) => (current + 1) % visibleSlashCommands.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setSlashHighlightedIndex((current) => (current - 1 + visibleSlashCommands.length) % visibleSlashCommands.length);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setSlashDismissedInput(draftInput);
        return;
      }
      if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
        const command = exactSlashCommand ?? highlightedSlashCommand;
        if (command) {
          event.preventDefault();
          activateSlashCommand(command);
          return;
        }
      }
    }
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    void handleComposerSend();
  }, [
    activateSlashCommand,
    draftInput,
    exactSlashCommand,
    handleComposerSend,
    highlightedSlashCommand,
    slashPanelVisible,
    visibleSlashCommands.length,
  ]);

  return (
    <>
      {props.pendingPrompts.length ? (
        <PendingPromptQueueStrip
          entries={props.pendingPrompts}
          onTrace={props.onTrace}
          onEdit={(entryId) => void handlePendingPromptEdit(entryId)}
          onInsert={(entryId) => void props.onInsertPendingPrompt(entryId)}
          onRemove={(entryId) => void props.onRemovePendingPrompt(entryId)}
          onReorder={(entryIds, draggedEntryId, targetEntryId) => void props.onReorderPendingPrompts(entryIds, draggedEntryId, targetEntryId)}
        />
      ) : null}
      <section
        className={cn("composer-panel", isAttachmentDragOver && "attachment-drag-over")}
        onDragLeave={() => setIsAttachmentDragOver(false)}
        onDragOver={handleComposerDragOver}
        onDrop={handleComposerDrop}
      >
        <input
          accept={attachmentInputAccept}
          className="composer-attachment-input"
          multiple
          onChange={handleAttachmentInputChange}
          ref={attachmentFileInputRef}
          type="file"
        />
        {attachments.length ? (
          <ComposerAttachmentStrip
            attachments={attachments}
            resolvePreview={(attachment): ComposerAttachmentPreviewMedia | null => {
              const media = composerAttachmentMedia(attachment);
              if (!media) {
                return null;
              }
              return {
                kind: media.kind,
                sourceUrl: media.kind === "image" ? props.resolveMediaSourceUrl(media.localPath) : undefined,
              };
            }}
            onOpenMedia={(attachment) => {
              const media = composerAttachmentMedia(attachment);
              if (media) {
                setComposerMediaPlayer(media);
              }
            }}
            onRemove={removeComposerAttachment}
          />
        ) : null}
        {slashPanelVisible ? (
          <ComposerSlashCommandPanel
            commands={visibleSlashCommands}
            highlightedIndex={Math.min(slashHighlightedIndex, visibleSlashCommands.length - 1)}
            locale={locale}
            onActivate={activateSlashCommand}
            onHighlight={setSlashHighlightedIndex}
          />
        ) : null}
        <Textarea
          aria-label={t("composer.aria.input")}
          onChange={(event) => handleDraftInputChange(event.currentTarget.value)}
          onKeyDown={handleComposerKeyDown}
          onPaste={handleComposerPaste}
          placeholder={placeholder}
          ref={composerTextareaRef}
          value={draftInput}
        />
        <div className="composer-toolbar">
          <div className="composer-toolbar-left">
            <ComposerAttachmentMenu
              capabilities={enabledAttachmentCapabilities}
              thinkingOverlayEnabled={props.thinkingOverlayEnabled}
              onPick={openAttachmentPicker}
              onThinkingOverlayEnabledChange={props.onThinkingOverlayEnabledChange}
            />
            <ComposerSelectControl
              ariaLabel={t("composer.aria.mode")}
              className="mode no-indicator"
              fallbackLabel={t("composer.selectFallback")}
              onValueChange={setExecutionMode}
              options={executionModeOptions}
              value={executionMode}
            />
            <ModelPickerControl
              ariaLabel={t("composer.aria.model")}
              fallbackLabel={t("composer.selectFallback")}
              filterPlaceholder={t("composer.modelPicker.filter")}
              noMatchesLabel={t("composer.modelPicker.noMatches")}
              onValueChange={(choice) => void handleModelChoiceChange(choice)}
              openSignal={modelPickerOpenSignal}
              providers={props.providers}
              value={modelChoice}
            />
            <ComposerAiCapabilityControl
              ariaLabel={t("composer.aria.capability")}
              capabilities={props.aiCapabilities}
              disabledKinds={disabledAiCapabilityKinds}
              disabledTargetSelectors={disabledAiCapabilityTargetSelectors}
              targetOrder={aiCapabilityTargetOrder}
              locale={locale}
              onToggle={handleAiCapabilityToggle}
              onTargetReorder={handleAiCapabilityTargetReorder}
              onTargetToggle={handleAiCapabilityTargetToggle}
            />
          </div>
          <div className="composer-toolbar-right">
            <ContextUsageOrb snapshot={props.contextUsage} />
            <ComposerSelectControl
              ariaLabel={t("composer.aria.reasoning")}
              className="reasoning no-indicator"
              fallbackLabel={t("composer.selectFallback")}
              hideTriggerSubtitle
              icon={<Brain size={15} />}
              onValueChange={(value) => void handleReasoningEffortChange(value)}
              openSignal={reasoningPickerOpenSignal}
              options={reasoningEffortOptions}
              value={reasoningEffort}
            />
            {props.isConversationRunning && (draftInput.trim() || attachments.length > 0) ? (
              <Button
                aria-label={t("composer.aria.stop")}
                className="send-button running"
                onClick={() => void handleComposerStop()}
                size="icon"
                type="button"
              >
                <Square fill="currentColor" size={15} strokeWidth={0} />
              </Button>
            ) : null}
            <Button
              aria-label={props.isConversationRunning && !draftInput.trim() && attachments.length === 0 ? t("composer.aria.stop") : t("composer.aria.send")}
              className={cn("send-button", props.isConversationRunning && !draftInput.trim() && attachments.length === 0 && "running")}
              onClick={() => void handleComposerSend()}
              size="icon"
              type="button"
            >
              {props.isConversationRunning && !draftInput.trim() && attachments.length === 0 ? <Square fill="currentColor" size={15} strokeWidth={0} /> : <Send size={18} />}
            </Button>
          </div>
        </div>
        {composerMediaPlayer ? (
          <GeneratedMediaPlayerWindow attachment={composerMediaPlayer} onClose={() => setComposerMediaPlayer(null)} />
        ) : null}
      </section>
    </>
  );
}
