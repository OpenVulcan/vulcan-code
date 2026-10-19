import * as React from "react";
import { CodeXml } from "lucide-react";
import {
  GeneratedMediaStrip,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import type {
  ConversationDetail,
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import { useConversationToolRequestProgress } from "@/state/conversation-live-stream-context";
import {
  mergeGeneratedMediaAttachments,
  toolExecutionMayHaveGeneratedMedia,
  useToolDetailMediaAttachments,
} from "./conversation-tool-execution-media";
import {
  buildToolRawInvocationFallback,
  liveToolRequestProgress,
  preferredSubagentTaskDisplayBlock,
  toolExecutionAllowsRawInvocation,
  toolExecutionDetailRef,
  toolExecutionDetailLoadEnabled,
  toolExecutionDisplayState,
  toolExecutionMediaAttachments,
  toolExecutionName,
  toolExecutionSupportsDetailLookup,
  toolRequestProgress,
  toolRequestProgressForDisplay,
  toolRequestProgressLabel,
  toolRequestStreamId,
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
import { fixedV1ToolEvent } from "./tool-presentations/presentation-value-readers";

export type {
  ToolExecutionConversationBlock,
  ToolExecutionConversationBlockKind,
  ToolExecutionTraceCallback,
} from "./conversation-tool-execution-types";
export { toolExecutionDisplayState } from "./conversation-tool-execution-model";

// ToolRawInvocationButton renders the raw invocation action inside one expanded non-native card.
// ToolRawInvocationButton 在一张已展开的非原生工具卡片内部渲染原始调用操作。
export function ToolRawInvocationButton(props: {
  // Opens the raw invocation modal.
  // 打开原始调用弹窗。
  onOpen: () => void;
}) {
  // t resolves the exact localized raw-action copy.
  // t 解析精确的本地化原始操作文案。
  const { t } = useI18n();
  return (
    <div className="tool-presentation-raw-footer">
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="tool-presentation-raw-trigger">
            <button
              aria-label={t("toolPresentation.raw.open")}
              className="tool-presentation-raw-button"
              onClick={props.onOpen}
              type="button"
            >
              <CodeXml aria-hidden="true" size={13} />
            </button>
          </span>
        </TooltipTrigger>
        <TooltipContent side="top">{t("toolPresentation.raw.open")}</TooltipContent>
      </Tooltip>
    </div>
  );
}

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

  const displayState = toolExecutionDisplayState(props.blocks, props.sessionRunning);
  const toolName = toolExecutionName(props.blocks);
  const mediaAttachments = toolExecutionMediaAttachments(props.blocks);
  // retainedRequestProgress supplies the durable Started or Ready identity used to bind the leaf Store.
  // retainedRequestProgress 提供用于绑定叶子 Store 的持久 Started 或 Ready 身份。
  const retainedRequestProgress = toolRequestProgress(props.blocks);
  // liveRequestProgress changes only this tool-card subtree during argument token streaming.
  // liveRequestProgress 在参数 Token 流期间仅改变当前工具卡子树。
  const liveRequestProgress = useConversationToolRequestProgress(
    props.conversationId,
    toolRequestStreamId(props.blocks),
  );
  // resolvedRequestProgress prefers the newer external snapshot while retaining durable lifecycle boundaries.
  // resolvedRequestProgress 优先使用较新的外部快照，同时保留持久生命周期边界。
  const resolvedRequestProgress = retainedRequestProgress && liveRequestProgress
    ? liveToolRequestProgress(liveRequestProgress)
    : retainedRequestProgress;
  // requestProgressForDisplay stops the durable ready marker from overriding the actual tool execution state.
  // requestProgressForDisplay 阻止持久化的 ready 标记覆盖真实工具执行状态。
  const requestProgress = toolRequestProgressForDisplay(
    resolvedRequestProgress,
    displayState,
  );
  const detailRefFields = toolExecutionDetailRef(props.blocks);
  // v1BackedPresentation identifies cards whose complete persisted payload already came from the V1 ledger.
  // v1BackedPresentation 标识完整持久化载荷已来自 V1 账本的卡片。
  const v1ToolEvent = fixedV1ToolEvent(props.blocks);
  // Presence follows the same producer-owned event used by domain cards.
  // 是否存在取决于领域卡使用的同一生产者权威事件。
  const v1BackedPresentation = v1ToolEvent !== undefined;
  // supportsDetailLookup prevents a merely stopped request row from enabling a nonexistent terminal detail.
  // supportsDetailLookup 防止仅被停止的请求行启用不存在的终态详情。
  const supportsDetailLookup = toolExecutionSupportsDetailLookup(props.blocks);
  // detailLookupRefFields preserves the documented tool-call lookup when no hidden record id exists yet.
  // detailLookupRefFields 在尚无隐藏记录标识时保留已定义的工具调用查询路径。
  const detailLookupRefFields = detailRefFields || (supportsDetailLookup && primaryBlock.toolCallId)
    ? {
        ...(detailRefFields ?? {}),
        ...(primaryBlock.toolCallId ? { toolCallId: primaryBlock.toolCallId } : {}),
      }
    : null;
  // detailLoadEnabled requires a terminal host-owned detail reference; request formation alone is never loadable.
  // detailLoadEnabled 要求宿主持有的终态详情引用；仅请求构造状态绝不可加载。
  const detailLoadEnabled = !v1BackedPresentation && toolExecutionDetailLoadEnabled(
    displayState,
    requestProgress,
    detailLookupRefFields,
  );
  const detailRef = props.conversationId && detailLoadEnabled && detailLookupRefFields
    ? { sessionId: props.conversationId, ...detailLookupRefFields }
    : undefined;
  const commandSessionById = React.useMemo(
    () => terminalSummaryById(props.commandSessions),
    [props.commandSessions],
  );
  const presentation = React.useMemo(
    () => resolveToolPresentation(props.blocks, displayState, detailRef, {
      commandSessionById,
      commandSessions: props.commandSessions,
      requestPreview: resolvedRequestProgress?.preview ?? null,
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
      resolvedRequestProgress?.preview,
      props.workspacePath,
    ],
  );
  // Dedicated cards show semantic previews; only the generic card needs byte progress.
  // 专用卡显示语义预览；仅通用卡需要字节进度。
  const requestCardProgress = requestProgress ? {
    toolName: primaryBlock.toolName ?? primaryBlock.title,
    receiving: true,
    statusLabel: presentation.kind === "generic_tool_markdown" && !presentation.titleKey
      ? toolRequestProgressLabel(requestProgress, t) : "",
  } : null;
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
    detailLoadEnabled
      && mediaAttachments.length === 0
      && toolExecutionMayHaveGeneratedMedia(toolName),
    props.onLoadToolDetail,
    props.onExtractGeneratedMediaAttachments,
  );
  const displayedMediaAttachments = mergeGeneratedMediaAttachments(mediaAttachments, hydratedMediaAttachments);
  // rawInvocationAllowed trusts only the runtime binding source and never infers native ownership from a tool name.
  // rawInvocationAllowed 仅信任运行时绑定来源，绝不根据工具名称推断原生归属。
  const rawInvocationAllowed = !v1BackedPresentation && toolExecutionAllowsRawInvocation(props.blocks);
  // rawInvocationTitle identifies the raw invocation modal without changing the semantic card title.
  // rawInvocationTitle 标识原始调用弹窗，但不改变语义卡片标题。
  const rawInvocationTitle = t("toolPresentation.raw.title").replace("{tool}", toolName ?? primaryBlock.title);
  // openRawInvocation opens the terminal host envelope and records the explicit raw-detail action.
  // openRawInvocation 打开终态宿主信封，并记录明确的原始详情操作。
  const openRawInvocation = () => {
    if (!rawInvocationAllowed || !detailRef) {
      return;
    }
    props.onTrace?.("tool.open_raw_invocation", {
      toolName,
      displayState,
      primaryKind: primaryBlock.kind,
      blockKinds: props.blocks.map((block) => block.kind),
    });
    props.onOpenDetail({
      title: rawInvocationTitle,
      kind: "tool-raw",
      body: `\`\`\`json\n${JSON.stringify(buildToolRawInvocationFallback(props.blocks, displayState), null, 2)}\n\`\`\``,
      detailRef,
      mediaAttachments: displayedMediaAttachments,
    });
  };
  return (
    <div className="tool-execution-stack">
      <ToolPresentationCard
        v1ToolEvent={v1ToolEvent}
        expanded={effectiveExpandedToolPresentationIds.has(presentation.id)}
        expandedFooter={rawInvocationAllowed && detailRef ? (
          <ToolRawInvocationButton onOpen={openRawInvocation} />
        ) : null}
        expandedToolPresentationIds={effectiveExpandedToolPresentationIds}
        presentation={presentation}
        requestProgress={requestCardProgress}
        detailLoadEnabled={detailLoadEnabled}
        onExpandedChange={handleToolPresentationExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
      />
      {displayedMediaAttachments.length > 0 ? (
        <GeneratedMediaStrip
          attachments={displayedMediaAttachments}
          onOpenDetail={() => {
            const firstAttachment = displayedMediaAttachments[0];
            if (firstAttachment) props.onOpenMedia(firstAttachment);
          }}
          onOpenMedia={props.onOpenMedia}
        />
      ) : null}
    </div>
  );
}
