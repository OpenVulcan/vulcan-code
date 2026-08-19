import * as React from "react";
import { Wrench } from "lucide-react";
import {
  GeneratedMediaStrip,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import type {
  ConversationDetail,
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { cn } from "@/lib/utils";
import {
  mergeGeneratedMediaAttachments,
  toolExecutionMayHaveGeneratedMedia,
  useToolDetailMediaAttachments,
} from "./conversation-tool-execution-media";
import {
  buildToolExecutionDetailBody,
  preferredSubagentTaskDisplayBlock,
  toolDisplayStateLabel,
  toolExecutionDetailRef,
  toolExecutionDisplayState,
  toolExecutionErrorPreview,
  toolExecutionIdentifier,
  toolExecutionMediaAttachments,
  toolExecutionName,
  toolRequestProgress,
  toolRequestProgressStateLabel,
  toolRequestStreamMetricsLabel,
  toolExecutionSubagentLabel,
  toolExecutionSummary,
} from "./conversation-tool-execution-model";
import type {
  ToolExecutionConversationBlock,
  ToolExecutionTraceCallback,
} from "./conversation-tool-execution-types";
import type { CommandSessionSummary } from "@/types";
import { useI18n } from "@/i18n";
import { ToolPresentationCard } from "./conversation-tool-presentation-card";
import { terminalSummaryById } from "./conversation-terminal-model";
import { resolveToolPresentation } from "./conversation-tool-presentations";

export type {
  ToolExecutionConversationBlock,
  ToolExecutionConversationBlockKind,
  ToolExecutionTraceCallback,
} from "./conversation-tool-execution-types";
export { toolExecutionDisplayState } from "./conversation-tool-execution-model";

// ToolExecutionBlock renders one compact merged tool execution row.
// ToolExecutionBlock 渲染单条紧凑的合并工具执行行。
export function ToolExecutionBlock(props: {
  // Tool-related blocks merged into one compact row.
  // 合并成单条紧凑行的工具相关块。
  blocks: ToolExecutionConversationBlock[];
  // Conversation id that owns hidden detail records.
  // 拥有隐藏详情记录的会话标识。
  conversationId: string | null;
  // WebUI-session expansion memory for custom tool presentation cards.
  // 自定义工具展示卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Project workspace path used for file presentation path shortening.
  // 文件展示路径缩短使用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Parent conversation id used by routed subagent navigation.
  // 已路由子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Records one tool presentation expansion state change.
  // 记录一次工具展示展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from one hidden detail payload.
  // 从单条隐藏详情载荷中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local UI diagnostics.
  // 用于本地 UI 诊断的可选埋点回调。
  onTrace?: ToolExecutionTraceCallback;
}) {
  const { t } = useI18n();
  const callBlock = props.blocks.find((block) => block.kind === "tool-call");
  const resultBlock = props.blocks.find((block) => block.kind === "tool-result");
  const commandBlock = props.blocks.find((block) => block.kind === "command");
  const primaryBlock = preferredSubagentTaskDisplayBlock(props.blocks) ?? resultBlock ?? commandBlock ?? callBlock ?? props.blocks[0];
  if (!primaryBlock) {
    return null;
  }

  const summary = toolExecutionSummary(props.blocks, props.sessionRunning, t);
  const toolName = toolExecutionName(props.blocks);
  const subagentLabel = toolExecutionSubagentLabel(props.blocks, t);
  const displayState = toolExecutionDisplayState(props.blocks, props.sessionRunning);
  const mediaAttachments = toolExecutionMediaAttachments(props.blocks);
  const subagentTaskId = props.blocks.find((block) => block.subagentTaskId)?.subagentTaskId ?? null;
  const identifier = toolExecutionIdentifier(props.blocks);
  const errorPreview = toolExecutionErrorPreview(props.blocks);
  const requestProgress = toolRequestProgress(props.blocks);
  // requestCardProgress keeps request formation distinct from actual tool execution while reusing the resolved card.
  // requestCardProgress 在复用既有卡片的同时，保持请求构造与真实工具执行语义分离。
  const requestCardProgress = requestProgress ? {
    metricsLabel: toolRequestStreamMetricsLabel(requestProgress, t),
    revision: requestProgress.revision,
    statusLabel: toolRequestProgressStateLabel(requestProgress, t),
  } : null;
  const detailRefFields = toolExecutionDetailRef(props.blocks);
  const detailRef = props.conversationId && (detailRefFields || primaryBlock.toolCallId)
    ? { sessionId: props.conversationId, ...(detailRefFields ?? {}), ...(primaryBlock.toolCallId ? { toolCallId: primaryBlock.toolCallId } : {}) }
    : undefined;
  const commandSessionById = React.useMemo(
    () => terminalSummaryById(props.commandSessions),
    [props.commandSessions],
  );
  const presentation = React.useMemo(
    () => resolveToolPresentation(props.blocks, displayState, detailRef, {
      commandSessionById,
      commandSessions: props.commandSessions,
      workspacePath: props.workspacePath,
    }),
    [
      detailRef?.messageIndex,
      detailRef?.recordId,
      detailRef?.seq,
      detailRef?.sessionId,
      detailRef?.toolCallId,
      displayState,
      commandSessionById,
      props.blocks,
      props.commandSessions,
      props.workspacePath,
    ],
  );
  // localToolPresentationExpansionOverrides keeps clicks inside this leaf card subtree.
  // localToolPresentationExpansionOverrides 将点击更新限制在当前叶子卡片子树内。
  const [localToolPresentationExpansionOverrides, setLocalToolPresentationExpansionOverrides] = React.useState<ReadonlyMap<string, boolean>>(
    () => new Map<string, boolean>(),
  );
  // effectiveExpandedToolPresentationIds overlays immediate local choices on persisted surface memory.
  // effectiveExpandedToolPresentationIds 将即时本地选择覆盖到已持久化的界面记忆之上。
  const effectiveExpandedToolPresentationIds = React.useMemo(() => {
    if (localToolPresentationExpansionOverrides.size === 0) {
      return props.expandedToolPresentationIds;
    }
    // nextExpandedIds is isolated to this tool subtree and never mutates the persisted Set.
    // nextExpandedIds 仅属于当前工具子树，并且绝不修改已持久化的 Set。
    const nextExpandedIds = new Set(props.expandedToolPresentationIds);
    for (const [presentationId, expanded] of localToolPresentationExpansionOverrides) {
      if (expanded) {
        nextExpandedIds.add(presentationId);
      } else {
        nextExpandedIds.delete(presentationId);
      }
    }
    return nextExpandedIds;
  }, [localToolPresentationExpansionOverrides, props.expandedToolPresentationIds]);
  // handleToolPresentationExpandedChange updates the leaf immediately and persists memory as a transition.
  // handleToolPresentationExpandedChange 立即更新叶子卡片，并以过渡任务持久化记忆。
  // Parameters: presentationId identifies the clicked card and expanded is its requested next state.
  // 参数：presentationId 标识被点击的卡片，expanded 是其请求的下一状态。
  // Returns: nothing; the urgent local render and deferred surface-memory update are scheduled separately.
  // 返回值：无；紧急本地渲染与延后的界面记忆更新会被分别调度。
  const handleToolPresentationExpandedChange = React.useCallback((presentationId: string, expanded: boolean): void => {
    setLocalToolPresentationExpansionOverrides((current) => {
      // next overrides only the clicked presentation while retaining nested file-card choices.
      // next 只覆盖被点击的展示，同时保留嵌套文件卡片选择。
      const next = new Map(current);
      next.set(presentationId, expanded);
      return next;
    });
    React.startTransition(() => {
      props.onToolPresentationExpandedChange(presentationId, expanded);
    });
  }, [props.onToolPresentationExpandedChange]);
  const hydratedMediaAttachments = useToolDetailMediaAttachments(
    detailRef,
    mediaAttachments.length === 0 && toolExecutionMayHaveGeneratedMedia(toolName),
    props.onLoadToolDetail,
    props.onExtractGeneratedMediaAttachments,
  );
  const displayedMediaAttachments = mergeGeneratedMediaAttachments(mediaAttachments, hydratedMediaAttachments);
  const metadata = [
    ...(subagentLabel
      ? [[displayState === "running" ? t("conversation.subagent.calling") : t("conversation.subagent.call"), subagentLabel] as [string, string]]
      : toolName ? [[t("conversation.metadata.tool"), toolName] as [string, string]] : []),
    ...(identifier ? [[t("conversation.metadata.identifier"), identifier] as [string, string]] : []),
    ...(errorPreview ? [[t("conversation.metadata.error"), errorPreview] as [string, string]] : []),
    [t("conversation.metadata.status"), toolDisplayStateLabel(displayState, t)] as [string, string],
    ...(resultBlock ? [[t("conversation.metadata.result"), t("conversation.metadata.returned")] as [string, string]] : []),
  ];

  if (presentation) {
    return (
      <ToolPresentationCard
        expanded={effectiveExpandedToolPresentationIds.has(presentation.id)}
        expandedToolPresentationIds={effectiveExpandedToolPresentationIds}
        presentation={presentation}
        requestProgress={requestCardProgress}
        onExpandedChange={handleToolPresentationExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
      />
    );
  }

  // openFallbackDetail builds the full modal body lazily to keep transcript rows cheap.
  // openFallbackDetail 延迟构建完整弹窗正文，使转录行保持轻量。
  const openFallbackDetail = () => {
    props.onTrace?.("tool.open_fallback_detail", {
      toolName,
      displayState,
      primaryKind: primaryBlock.kind,
      subagentTaskId,
      blockKinds: props.blocks.map((block) => block.kind),
    });
    props.onOpenDetail({
      title: summary,
      kind: primaryBlock.kind,
      body: buildToolExecutionDetailBody(props.blocks, t),
      detailRef,
      metadata,
      mediaAttachments: displayedMediaAttachments,
    });
  };
  // openDetail always opens inline tool details; subagent side panels are only opened from the floating session tool panel.
  // openDetail 始终打开内联工具详情；子代理右侧面板只允许从浮动会话工具面板打开。
  const openDetail = () => {
    props.onTrace?.("tool.open", {
      toolName,
      displayState,
      subagentTaskId,
      parentConversationId: props.parentConversationId,
      blockKinds: props.blocks.map((block) => block.kind),
    });
    openFallbackDetail();
  };

  return (
    <div className="tool-execution-stack">
      <button
        className={cn("tool-execution-row", (displayState === "running" || requestProgress) && "running")}
        onClick={openDetail}
        type="button"
      >
        <Wrench size={15} />
        <span className={cn(displayState === "running" && "shimmer")}>{summary}</span>
      </button>
      {displayedMediaAttachments.length > 0 ? (
        <GeneratedMediaStrip
          attachments={displayedMediaAttachments}
          onOpenDetail={openDetail}
          onOpenMedia={props.onOpenMedia}
        />
      ) : null}
    </div>
  );
}
