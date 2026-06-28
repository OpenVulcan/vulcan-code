import React from "react";
import {
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Clock3,
  Copy,
  GitFork,
  Pencil,
} from "lucide-react";

import {
  ComposerAttachmentStrip,
  type ComposerAttachmentPreviewMedia,
} from "@/components/composer/composer-controls";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import {
  type ConversationDetail,
  type ConversationDetailRef,
  type ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { ConversationBlocksView } from "@/components/conversation/conversation-block-renderer";
import {
  ConversationMeasuredCollapse,
  useConversationMeasuredExpansion,
} from "@/components/conversation/conversation-measured-collapse";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import type { PlanArtifactActionPayload } from "@/components/conversation/conversation-plan-artifact";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import {
  conversationMessageUnixMillis,
  formatShortConversationTimestamp,
  formatWorkProcessDuration,
} from "@/components/conversation/conversation-timeline";
import type {
  ConversationProcessRenderItem,
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import { metadataValue } from "@/components/conversation/conversation-metadata";
import type { CommandSessionSummary, ComposerAttachmentSummary } from "@/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";

// USER_PROMPT_COLLAPSE_LINE_LIMIT limits long user bubbles before the user expands them.
// USER_PROMPT_COLLAPSE_LINE_LIMIT 限制用户手动展开前长用户气泡的显示行数。
const USER_PROMPT_COLLAPSE_LINE_LIMIT = 20;

// ConversationProcessGroup renders one per-turn process header and its foldable rows.
// ConversationProcessGroup 渲染单个轮次过程标题及其可折叠处理行。
export function ConversationProcessGroup(props: {
  // autoExpandThinkingBlockId keeps parity with normal block rendering.
  // autoExpandThinkingBlockId 保持与普通块渲染一致。
  autoExpandThinkingBlockId: string | null;
  // Optional user-controlled collapsed state remembered by the conversation surface.
  // 会话界面记住的可选用户控制折叠状态。
  collapsedOverride?: boolean;
  // hiddenThinkingBlockIds hides thinking blocks filtered by the display mode.
  // hiddenThinkingBlockIds 隐藏由展示模式过滤的思考块。
  hiddenThinkingBlockIds: ReadonlySet<string>;
  // Current clock tick used for live duration projection.
  // 用于实时耗时投影的当前时钟刻度。
  nowMs: number;
  // Parent conversation id used by subagent navigation.
  // 子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Process group payload.
  // 过程分组载荷。
  process: ConversationProcessRenderItem;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // WebUI-session expansion memory for long user prompt bubbles.
  // 长用户提示词气泡使用的当前 WebUI 会话展开记忆。
  expandedUserPromptIds: Set<string>;
  // WebUI-session expansion memory for custom tool presentation cards.
  // 自定义工具展示卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Project workspace path used by file-oriented tool presentations.
  // 文件类工具展示使用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Records one user-triggered collapsed state change.
  // 记录一次用户触发的折叠状态变化。
  onCollapsedChange: (collapsed: boolean) => void;
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Opens one backend-backed user prompt edit flow.
  // 打开单个后端支持的用户提示词编辑流程。
  onEditUserPrompt?: (conversationId: string, messageId: string) => Promise<void> | void;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
  // Currently pending Plan artifact, when user confirmation is required.
  // 需要用户确认时的当前待处理计划产物。
  pendingPlanArtifact?: ConversationPlanArtifact | null;
  // Executes one pending Plan artifact.
  // 执行单个待确认计划产物。
  onPlanArtifactExecute?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Sends a suggestion for one pending Plan artifact.
  // 为单个待确认计划产物发送补充建议。
  onPlanArtifactSuggest?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Cancels one pending Plan artifact.
  // 取消单个待确认计划产物。
  onPlanArtifactCancel?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Subagent panel opener.
  // 子代理面板打开器。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Records one user prompt expansion state change.
  // 记录一次用户提示词展开状态变化。
  onUserPromptExpandedChange: (messageKey: string, expanded: boolean) => void;
  // Records one tool presentation expansion state change.
  // 记录一次工具展示展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Resolves composer attachments into generated-media compatible payloads.
  // 将输入区附件解析为兼容生成媒体的载荷。
  resolveComposerAttachmentMedia: (attachment: ComposerAttachmentSummary) => GeneratedMediaAttachment | null;
  // Resolves a local media file path into a renderable URL.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from hidden tool details.
  // 从隐藏工具详情中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local diagnostics.
  // 用于本地诊断的可选埋点回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}) {
  const { locale, t } = useI18n();
  const fallbackStartedAtMsRef = React.useRef(Date.now());
  React.useEffect(() => {
    fallbackStartedAtMsRef.current = Date.now();
  }, [props.process.id]);
  const collapsed = props.collapsedOverride ?? props.process.defaultCollapsed;
  const visibleGuidanceItems = props.process.guidanceItems.filter((item) => !renderItemHiddenByThinkingDisplay(item, props.hiddenThinkingBlockIds));
  const visibleProcessItems = props.process.items.filter((item) => !renderItemHiddenByThinkingDisplay(item, props.hiddenThinkingBlockIds));
  const visibleCollapsedProcessUserItems = collapsed ? visibleProcessItems.filter(isTurnProcessUserRenderItem) : [];
  const streamingProcessItemId = props.process.running ? visibleProcessItems.at(-1)?.id ?? null : null;
  const liveDurationMs = props.process.running
    ? Math.max(0, props.nowMs - (props.process.startedAtMs ?? fallbackStartedAtMsRef.current))
    : null;
  const displayedDurationMs = props.process.durationMs ?? liveDurationMs;
  const durationLabel = formatWorkProcessDuration(displayedDurationMs, locale);
  const headerText = durationLabel ? `${t("conversation.process.header")} ${durationLabel}` : t("conversation.process.header");
  const stoppedText = durationLabel
    ? t("conversation.process.stoppedAfter").replace("{duration}", durationLabel)
    : t("conversation.process.stopped");
  const bodyVisible = !props.process.collapsible || !collapsed;
  const processBodyCollapseId = `process-body:${props.process.id}`;
  return (
    <section
      className={cn("work-process-group", props.process.running && "running", props.process.stopped && "stopped")}
      data-process-body-visible={bodyVisible ? "true" : "false"}
      data-process-collapsed={collapsed ? "true" : "false"}
    >
      <button
        aria-expanded={bodyVisible}
        className="work-process-toggle"
        disabled={!props.process.collapsible}
        onClick={() => {
          if (!props.process.collapsible) {
            return;
          }
          dispatchConversationManualViewportInteraction();
          props.onCollapsedChange(!collapsed);
        }}
        type="button"
      >
        <Clock3 className="work-process-icon" size={14} />
        <span>{headerText}</span>
        {props.process.collapsible ? (collapsed ? <ChevronRight className="work-process-toggle-icon" size={13} /> : <ChevronDown className="work-process-toggle-icon" size={13} />) : null}
      </button>
      <div className="work-process-guidance">
        {visibleGuidanceItems.map((item) => (
          <ConversationRenderItemView
            autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
            hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
            expandedToolPresentationIds={props.expandedToolPresentationIds}
            workspacePath={props.workspacePath}
            commandSessions={props.commandSessions}
            item={item}
            itemStreaming={false}
            key={item.id}
            parentConversationId={props.parentConversationId}
            sessionRunning={props.sessionRunning}
            expandedUserPromptIds={props.expandedUserPromptIds}
            onCopyText={props.onCopyText}
            onEditUserPrompt={props.onEditUserPrompt}
            onOpenDetail={props.onOpenDetail}
            onOpenMedia={props.onOpenMedia}
            onOpenSubagentPanel={props.onOpenSubagentPanel}
            pendingPlanArtifact={props.pendingPlanArtifact}
            onPlanArtifactExecute={props.onPlanArtifactExecute}
            onPlanArtifactSuggest={props.onPlanArtifactSuggest}
            onPlanArtifactCancel={props.onPlanArtifactCancel}
            onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
            onUserPromptExpandedChange={props.onUserPromptExpandedChange}
            resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
            resolveMediaSourceUrl={props.resolveMediaSourceUrl}
            onLoadToolDetail={props.onLoadToolDetail}
            onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
            onTrace={props.onTrace}
            forceEagerMarkdown
          />
        ))}
        {visibleCollapsedProcessUserItems.map((item) => (
          <ConversationRenderItemView
            autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
            hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
            expandedToolPresentationIds={props.expandedToolPresentationIds}
            workspacePath={props.workspacePath}
            commandSessions={props.commandSessions}
            item={item}
            itemStreaming={false}
            key={item.id}
            parentConversationId={props.parentConversationId}
            sessionRunning={props.sessionRunning}
            expandedUserPromptIds={props.expandedUserPromptIds}
            onCopyText={props.onCopyText}
            onEditUserPrompt={props.onEditUserPrompt}
            onOpenDetail={props.onOpenDetail}
            onOpenMedia={props.onOpenMedia}
            onOpenSubagentPanel={props.onOpenSubagentPanel}
            pendingPlanArtifact={props.pendingPlanArtifact}
            onPlanArtifactExecute={props.onPlanArtifactExecute}
            onPlanArtifactSuggest={props.onPlanArtifactSuggest}
            onPlanArtifactCancel={props.onPlanArtifactCancel}
            onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
            onUserPromptExpandedChange={props.onUserPromptExpandedChange}
            resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
            resolveMediaSourceUrl={props.resolveMediaSourceUrl}
            onLoadToolDetail={props.onLoadToolDetail}
            onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
            onTrace={props.onTrace}
            forceEagerMarkdown
          />
        ))}
      </div>
      <ConversationMeasuredCollapse collapseId={processBodyCollapseId} expanded={bodyVisible}>
        <div className="work-process-body">
          {visibleProcessItems.map((item) => (
            <ConversationRenderItemView
              autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
              hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
              expandedToolPresentationIds={props.expandedToolPresentationIds}
              workspacePath={props.workspacePath}
              commandSessions={props.commandSessions}
              item={item}
              itemStreaming={item.id === streamingProcessItemId}
              key={item.id}
              parentConversationId={props.parentConversationId}
              sessionRunning={props.sessionRunning}
              expandedUserPromptIds={props.expandedUserPromptIds}
              onCopyText={props.onCopyText}
              onEditUserPrompt={props.onEditUserPrompt}
              onOpenDetail={props.onOpenDetail}
              onOpenMedia={props.onOpenMedia}
              onOpenSubagentPanel={props.onOpenSubagentPanel}
              pendingPlanArtifact={props.pendingPlanArtifact}
              onPlanArtifactExecute={props.onPlanArtifactExecute}
              onPlanArtifactSuggest={props.onPlanArtifactSuggest}
              onPlanArtifactCancel={props.onPlanArtifactCancel}
              onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
              onUserPromptExpandedChange={props.onUserPromptExpandedChange}
              resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
              resolveMediaSourceUrl={props.resolveMediaSourceUrl}
              onLoadToolDetail={props.onLoadToolDetail}
              onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
              onTrace={props.onTrace}
              forceEagerMarkdown
            />
          ))}
          {props.process.stopped ? (
            <div className="work-process-stopped-note">
              {stoppedText}
            </div>
          ) : null}
        </div>
      </ConversationMeasuredCollapse>
    </section>
  );
}

// isTurnProcessUserRenderItem returns whether one user row should stay visible while its process is collapsed.
// isTurnProcessUserRenderItem 返回单条用户行是否应在过程折叠时保持可见。
function isTurnProcessUserRenderItem(item: ConversationRenderItem): boolean {
  if (item.role !== "user") {
    return false;
  }
  return metadataValue(item.userMessage?.metadata ?? item.sourceMessage?.metadata, ["turn_process"]) === true;
}

// ConversationRenderItemView renders one projected transcript item.
// ConversationRenderItemView 渲染单条投影后的转录展示项。
export function ConversationRenderItemView(props: {
  // Projected transcript row.
  // 投影后的转录行。
  item: ConversationRenderItem;
  // Thinking block that should auto expand while running.
  // 运行中应自动展开的思考块。
  autoExpandThinkingBlockId: string | null;
  // Thinking blocks hidden by the display mode.
  // 被展示模式隐藏的思考块。
  hiddenThinkingBlockIds: ReadonlySet<string>;
  // Whether this row is the currently streaming process item.
  // 该行是否为当前流式过程项。
  itemStreaming?: boolean;
  // Whether this row is the final assistant answer for one completed turn.
  // 该行是否为已完成回合的最终 Assistant 回答。
  finalAssistantOutput?: boolean;
  // Force Markdown blocks in externally hidden panels to stay mounted when visible.
  // 强制外层隐藏面板中的 Markdown 块在可见时保持直接挂载。
  forceEagerMarkdown?: boolean;
  // Parent conversation id used by subagent routing and local expansion memory.
  // 子代理路由与本地展开记忆使用的父会话标识。
  parentConversationId: string | null;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // WebUI-session expansion memory for long user prompt bubbles.
  // 长用户提示词气泡使用的当前 WebUI 会话展开记忆。
  expandedUserPromptIds: Set<string>;
  // WebUI-session expansion memory for custom tool presentation cards.
  // 自定义工具展示卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Project workspace path used by file-oriented tool presentations.
  // 文件类工具展示使用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Opens one backend-backed user prompt edit flow.
  // 打开单个后端支持的用户提示词编辑流程。
  onEditUserPrompt?: (conversationId: string, messageId: string) => Promise<void> | void;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
  // Currently pending Plan artifact, when user confirmation is required.
  // 需要用户确认时的当前待处理计划产物。
  pendingPlanArtifact?: ConversationPlanArtifact | null;
  // Executes one pending Plan artifact.
  // 执行单个待确认计划产物。
  onPlanArtifactExecute?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Sends a suggestion for one pending Plan artifact.
  // 为单个待确认计划产物发送补充建议。
  onPlanArtifactSuggest?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Cancels one pending Plan artifact.
  // 取消单个待确认计划产物。
  onPlanArtifactCancel?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Subagent panel opener.
  // 子代理面板打开器。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Records one user prompt expansion state change.
  // 记录一次用户提示词展开状态变化。
  onUserPromptExpandedChange: (messageKey: string, expanded: boolean) => void;
  // Records one tool presentation expansion state change.
  // 记录一次工具展示展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Resolves composer attachments into generated-media compatible payloads.
  // 将输入区附件解析为兼容生成媒体的载荷。
  resolveComposerAttachmentMedia: (attachment: ComposerAttachmentSummary) => GeneratedMediaAttachment | null;
  // Resolves a local media file path into a renderable URL.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from hidden tool details.
  // 从隐藏工具详情中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local diagnostics.
  // 用于本地诊断的可选埋点回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const userBody = React.useMemo(
    () => stripUserPromptHeading(props.item.userMessage?.body ?? ""),
    [props.item.userMessage?.body],
  );
  const userPromptTextRef = React.useRef<HTMLParagraphElement | null>(null);
  const [userPromptOverflowing, setUserPromptOverflowing] = React.useState(false);
  const [userPromptCopied, setUserPromptCopied] = React.useState(false);
  // userPromptCollapsedHeight stores the stable preview height for long user prompts.
  // userPromptCollapsedHeight 保存长用户提示词的稳定预览高度。
  const [userPromptCollapsedHeight, setUserPromptCollapsedHeight] = React.useState(0);
  const userPromptKey = React.useMemo(() => {
    const message = props.item.userMessage;
    const messageId = message?.id ?? props.item.id;
    const surfaceId = props.parentConversationId ?? message?.conversationId ?? "conversation";
    return `${surfaceId}:${messageId}`;
  }, [props.item.id, props.item.userMessage, props.parentConversationId]);
  const [measuredUserPromptExpanded, setMeasuredUserPromptExpanded] = useConversationMeasuredExpansion(
    `user-prompt:${userPromptKey}`,
    props.expandedUserPromptIds.has(userPromptKey),
  );
  const userPromptExpanded = props.expandedUserPromptIds.has(userPromptKey) || measuredUserPromptExpanded;
  const userTimestampMs = conversationMessageUnixMillis(props.item.userMessage);
  const userTimestamp = formatShortConversationTimestamp(props.item.userMessage);
  const handleCopyUserPrompt = React.useCallback(async () => {
    if (!userBody) {
      return;
    }
    await props.onCopyText(userBody);
    setUserPromptCopied(true);
  }, [props, userBody]);
  React.useLayoutEffect(() => {
    if (props.item.role !== "user" || !userBody) {
      setUserPromptOverflowing(false);
      return;
    }
    const element = userPromptTextRef.current;
    if (!element) {
      return;
    }
    const measureOverflow = () => {
      const computedStyle = window.getComputedStyle(element);
      const lineHeight = Number.parseFloat(computedStyle.lineHeight);
      const effectiveLineHeight = Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : 20;
      const collapsedHeight = effectiveLineHeight * USER_PROMPT_COLLAPSE_LINE_LIMIT;
      setUserPromptCollapsedHeight(collapsedHeight);
      setUserPromptOverflowing(element.scrollHeight > collapsedHeight + 1);
    };
    measureOverflow();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measureOverflow);
      return () => window.removeEventListener("resize", measureOverflow);
    }
    const observer = new ResizeObserver(measureOverflow);
    observer.observe(element);
    return () => observer.disconnect();
  }, [props.item.role, userBody, userPromptExpanded]);
  if (props.item.role === "user") {
    const attachments = props.item.userMessage?.attachments ?? [];
    return (
      <article className="message-row user">
        {attachments.length ? (
          <ComposerAttachmentStrip
            attachments={attachments}
            resolvePreview={(attachment): ComposerAttachmentPreviewMedia | null => {
              const media = props.resolveComposerAttachmentMedia(attachment);
              if (!media) {
                return null;
              }
              return {
                kind: media.kind,
                sourceUrl: media.kind === "image" ? props.resolveMediaSourceUrl(media.localPath) : undefined,
              };
            }}
            onOpenMedia={(attachment) => {
              const media = props.resolveComposerAttachmentMedia(attachment);
              if (media) {
                props.onOpenMedia(media);
              }
            }}
          />
        ) : null}
        <div className="user-bubble">
          {userBody && userPromptOverflowing ? (
            <ConversationMeasuredCollapse
              collapseId={`user-prompt-body:${userPromptKey}`}
              collapsedHeight={userPromptCollapsedHeight}
              expanded={userPromptExpanded}
            >
              <p className="user-prompt-text" ref={userPromptTextRef}>
                {userBody}
              </p>
            </ConversationMeasuredCollapse>
          ) : userBody ? (
            <p
              className="user-prompt-text"
              ref={userPromptTextRef}
            >
              {userBody}
            </p>
          ) : null}
          {userPromptOverflowing ? (
            <button
              aria-expanded={userPromptExpanded}
              className="user-prompt-expand-toggle"
              onClick={() => {
                setMeasuredUserPromptExpanded(!userPromptExpanded);
                props.onUserPromptExpandedChange(userPromptKey, !userPromptExpanded);
              }}
              type="button"
            >
              <span>{userPromptExpanded ? t("conversation.userPrompt.collapse") : t("conversation.userPrompt.expand")}</span>
              {userPromptExpanded ? (
                <ChevronUp className="user-prompt-expand-icon" size={14} />
              ) : (
                <ChevronDown className="user-prompt-expand-icon" size={14} />
              )}
            </button>
          ) : null}
        </div>
        <div className="user-message-meta">
          {userTimestamp && userTimestampMs ? (
            <time dateTime={new Date(userTimestampMs).toISOString()}>{userTimestamp}</time>
          ) : null}
          {userBody ? (
            <button
              aria-label={t("conversation.userPrompt.copy")}
              className="user-message-copy"
              onClick={() => void handleCopyUserPrompt()}
              onMouseLeave={() => setUserPromptCopied(false)}
              title={t("conversation.userPrompt.copy")}
              type="button"
            >
              {userPromptCopied ? <Check size={14} /> : <Copy size={14} />}
            </button>
          ) : null}
          {props.item.userMessage?.editableUserPrompt ? (
            <button
              aria-label={t("conversation.userPrompt.edit")}
              className="user-message-copy"
              onClick={() => void props.onEditUserPrompt?.(props.item.userMessage!.conversationId, props.item.userMessage!.id)}
              title={t("conversation.userPrompt.edit")}
              type="button"
            >
              <Pencil size={14} />
            </button>
          ) : null}
        </div>
      </article>
    );
  }

  const visibleBlocks = props.hiddenThinkingBlockIds.size > 0
    ? props.item.blocks.filter((block) => !props.hiddenThinkingBlockIds.has(block.id))
    : props.item.blocks;
  if (visibleBlocks.length === 0) {
    return null;
  }

  return (
    <article className={cn("message-row", props.item.role, props.finalAssistantOutput && "assistant-final-output")}>
      <div className="assistant-flow">
        {props.item.role === "system" ? <span className="message-role-label">SYSTEM</span> : null}
        <ConversationBlocksView
          autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
          blocks={visibleBlocks}
          conversationId={props.item.sourceMessage?.conversationId ?? props.item.userMessage?.conversationId ?? null}
          expandedToolPresentationIds={props.expandedToolPresentationIds}
          workspacePath={props.workspacePath}
          commandSessions={props.commandSessions}
          forceEagerMarkdown={props.forceEagerMarkdown}
          itemStreaming={Boolean(props.itemStreaming)}
          parentConversationId={props.parentConversationId}
          sessionRunning={props.sessionRunning}
          onOpenDetail={props.onOpenDetail}
          onOpenMedia={props.onOpenMedia}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          pendingPlanArtifact={props.pendingPlanArtifact}
          onCopyText={props.onCopyText}
          onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
          onPlanArtifactExecute={props.onPlanArtifactExecute}
          onPlanArtifactSuggest={props.onPlanArtifactSuggest}
          onPlanArtifactCancel={props.onPlanArtifactCancel}
          onLoadToolDetail={props.onLoadToolDetail}
          onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
          onTrace={props.onTrace}
        />
      </div>
    </article>
  );
}

// ConversationFinalAssistantMeta renders fixed actions below one final assistant turn.
// ConversationFinalAssistantMeta 渲染单个最终 Assistant 回合下方的固定操作信息。
export function ConversationFinalAssistantMeta(props: {
  // Final assistant row used for raw copy content and timestamp.
  // 用于原文复制内容与时间戳的最终 Assistant 行。
  item: ConversationRenderItem;
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Creates a forked copy of the owning conversation.
  // 创建所属会话的分叉副本。
  onForkConversation?: (conversationId: string, beforeMessageId?: string | null) => Promise<boolean> | boolean;
  // Optional user message id that marks the first message excluded from the fork.
  // 可选用户消息标识，表示分叉时第一个被排除的消息。
  forkBeforeMessageId?: string | null;
}) {
  const { t } = useI18n();
  const finalAssistantRawText = finalAssistantOutputText(props.item);
  const finalAssistantTimestampMs = conversationMessageUnixMillis(props.item.sourceMessage);
  const finalAssistantTimestamp = formatShortConversationTimestamp(props.item.sourceMessage);
  const conversationId = props.item.sourceMessage?.conversationId ?? null;
  const [finalAssistantCopied, setFinalAssistantCopied] = React.useState(false);
  const handleCopyFinalAssistantOutput = React.useCallback(async () => {
    if (!finalAssistantRawText) {
      return;
    }
    await props.onCopyText(finalAssistantRawText);
    setFinalAssistantCopied(true);
  }, [finalAssistantRawText, props]);
  const handleForkConversation = React.useCallback(async () => {
    if (!conversationId) {
      return;
    }
    await props.onForkConversation?.(conversationId, props.forkBeforeMessageId ?? null);
  }, [conversationId, props]);
  return (
    <div className="assistant-final-meta">
      {finalAssistantRawText ? (
        <button
          aria-label={finalAssistantCopied ? t("conversation.action.copiedFinal") : t("conversation.action.copyFinal")}
          className="assistant-final-meta-button"
          onClick={() => void handleCopyFinalAssistantOutput()}
          onMouseLeave={() => setFinalAssistantCopied(false)}
          title={finalAssistantCopied ? t("conversation.action.copiedFinal") : t("conversation.action.copyFinal")}
          type="button"
        >
          {finalAssistantCopied ? <Check size={13} /> : <Copy size={13} />}
        </button>
      ) : null}
      {conversationId && props.onForkConversation ? (
        <button
          aria-label={t("conversation.action.fork")}
          className="assistant-final-meta-button"
          onClick={() => void handleForkConversation()}
          title={t("conversation.action.fork")}
          type="button"
        >
          <GitFork size={13} />
        </button>
      ) : null}
      {finalAssistantTimestamp && finalAssistantTimestampMs ? (
        <time dateTime={new Date(finalAssistantTimestampMs).toISOString()}>{finalAssistantTimestamp}</time>
      ) : null}
    </div>
  );
}

// renderItemHiddenByThinkingDisplay hides a render row when all its thinking blocks are display-filtered.
// renderItemHiddenByThinkingDisplay 在一条渲染行的思考块全部被展示策略过滤时隐藏整行。
export function renderItemHiddenByThinkingDisplay(item: ConversationRenderItem, hiddenThinkingBlockIds: ReadonlySet<string>): boolean {
  return hiddenThinkingBlockIds.size > 0
    && item.blocks.length > 0
    && item.blocks.every((block) => block.kind === "thinking" && hiddenThinkingBlockIds.has(block.id));
}

// stripUserPromptHeading removes legacy transcript headings from visible user bubbles.
// stripUserPromptHeading 从可见用户气泡中移除旧版转录标题。
function stripUserPromptHeading(body: string): string {
  return body
    .replace(/^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:用户指令|User Prompt|USER)(?:\*\*)?\s*\n+/i, "")
    .trim();
}

// finalAssistantOutputText returns the raw Markdown source for one final assistant row.
// finalAssistantOutputText 返回单条最终 Assistant 行的 Markdown 原文。
function finalAssistantOutputText(item: ConversationRenderItem): string {
  const sourceBody = item.sourceMessage?.body?.trim();
  if (sourceBody) {
    return sourceBody;
  }
  return item.blocks
    .filter((block) => block.kind === "markdown")
    .map((block) => block.body.trim())
    .filter(Boolean)
    .join("\n\n");
}
