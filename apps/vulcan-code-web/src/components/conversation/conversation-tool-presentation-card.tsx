import * as React from "react";
import {
  Bot,
  ChevronDown,
  ChevronRight,
  Code2,
  FileText,
  FolderTree,
  GitBranch,
  MessageCircleQuestionMark,
  Pencil,
  Search,
  TerminalSquare,
  Trash2,
} from "lucide-react";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { ChangeSetDiffPreview } from "@/components/conversation/conversation-change-set-diff-preview";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { FileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { TerminalOutputView } from "@/components/conversation/conversation-terminal-output";
import {
  terminalStateClass,
  type TerminalVisualState,
} from "@/components/conversation/conversation-terminal-model";
import {
  hydrateAskQuestionPresentation,
  hydrateCommandExecutionPresentation,
  hydrateMarkdownToolPresentation,
  hydrateSubagentOrchestrationPresentation,
  type AskQuestionPresentation,
  type AskQuestionPresentationItem,
  type CommandExecutionPresentation,
  type FileChangeFilePresentation,
  type FileChangeOperation,
  type FileChangePresentation,
  type MarkdownToolPresentation,
  type MarkdownToolPresentationIcon,
  type SubagentOrchestrationAgent,
  type SubagentOrchestrationPresentation,
  type SubagentUsageSummary,
  type ToolPresentation,
  type ToolPresentationDisplayState,
} from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";

// ToolPresentationCard renders one recognized tool presentation with shared expansion behavior.
// ToolPresentationCard 使用共享展开行为渲染单个已识别工具展示。
export function ToolPresentationCard(props: {
  // Presentation model resolved by the adapter layer.
  // 由适配器层解析出的展示模型。
  presentation: ToolPresentation;
  // Whether the card is expanded in the current transcript surface.
  // 当前转录界面中该卡片是否已展开。
  expanded: boolean;
  // Expansion id set shared with the virtualized transcript surface.
  // 与虚拟滚动转录界面共享的展开标识集合。
  expandedToolPresentationIds: Set<string>;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for rich presentation data.
  // 加载单条隐藏工具详情记录用于补充展示数据。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
}) {
  if (props.presentation.kind === "command_execution") {
    return (
      <CommandExecutionPresentationCard
        expanded={props.expanded}
        presentation={props.presentation}
        onExpandedChange={props.onExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
      />
    );
  }
  if (props.presentation.kind === "file_change") {
    return (
      <FileChangePresentationCard
        expanded={props.expanded}
        expandedToolPresentationIds={props.expandedToolPresentationIds}
        presentation={props.presentation}
        onExpandedChange={props.onExpandedChange}
      />
    );
  }
  if (props.presentation.kind === "markdown_tool_output") {
    return (
      <MarkdownToolPresentationCard
        expanded={props.expanded}
        presentation={props.presentation}
        onExpandedChange={props.onExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
      />
    );
  }
  if (props.presentation.kind === "ask_question") {
    return (
      <AskQuestionPresentationCard
        expanded={props.expanded}
        presentation={props.presentation}
        onExpandedChange={props.onExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
      />
    );
  }
  if (props.presentation.kind === "subagent_orchestration") {
    return (
      <SubagentOrchestrationPresentationCard
        expanded={props.expanded}
        presentation={props.presentation}
        onExpandedChange={props.onExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
      />
    );
  }
  return null;
}

// commandPresentationDetailCache avoids refetching hidden details after virtualized remounts.
// commandPresentationDetailCache 避免虚拟滚动重挂后重复请求隐藏详情。
const commandPresentationDetailCache = new Map<string, CommandExecutionPresentation>();

// askQuestionPresentationDetailCache avoids refetching completed ask answers after virtualized remounts.
// askQuestionPresentationDetailCache 避免虚拟滚动重挂后重复请求已完成 ask 答案。
const askQuestionPresentationDetailCache = new Map<string, AskQuestionPresentation>();

// markdownToolPresentationDetailCache avoids refetching full Markdown-output detail after remounts.
// markdownToolPresentationDetailCache 避免重挂后重复请求完整 Markdown 输出详情。
const markdownToolPresentationDetailCache = new Map<string, MarkdownToolPresentation>();

// subagentOrchestrationPresentationDetailCache avoids refetching hidden subagent status details.
// subagentOrchestrationPresentationDetailCache 避免重复请求隐藏子代理状态详情。
const subagentOrchestrationPresentationDetailCache = new Map<string, SubagentOrchestrationPresentation>();

// MarkdownToolPresentationCard renders exact-tool Markdown output.
// MarkdownToolPresentationCard 渲染精确工具标识对应的 Markdown 输出。
function MarkdownToolPresentationCard(props: {
  // Markdown-output presentation model.
  // Markdown 输出展示模型。
  presentation: MarkdownToolPresentation;
  // Whether this Markdown-output card is expanded.
  // 当前 Markdown 输出卡片是否展开。
  expanded: boolean;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for full Markdown output.
  // 加载单条隐藏工具详情记录用于完整 Markdown 输出。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    expanded,
    onExpandedChange,
    onLoadToolDetail,
    presentation,
  } = props;
  const detailCacheKey = markdownToolPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<MarkdownToolPresentation>(
    () => detailCacheKey ? markdownToolPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  const [loadingDetail, setLoadingDetail] = React.useState(false);
  React.useEffect(() => {
    const cached = detailCacheKey ? markdownToolPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);
  React.useEffect(() => {
    if (!expanded || !presentation.detailRef || !detailCacheKey || markdownToolPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    setLoadingDetail(true);
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateMarkdownToolPresentation(presentation, payload);
        markdownToolPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          setHydratedPresentation(presentation);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingDetail(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);
  const markdown = hydratedPresentation.markdown.trim();
  const markdownBlock = markdown ? markdownToolCodeBlock(markdown) : "";
  const title = t(hydratedPresentation.titleKey);
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);
  return (
    <div className={cn("tool-presentation-card markdown-tool", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary markdown-tool-summary"
        onClick={handleToggle}
        title={title}
        type="button"
      >
        <MarkdownToolIcon className="markdown-tool-icon" icon={hydratedPresentation.icon} size={15} />
        <span className={cn("tool-presentation-state", hydratedPresentation.state === "running" && "shimmer")}>{markdownToolStatusLabel(hydratedPresentation.state, t)}</span>
        <span className="tool-presentation-command">{title}</span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`markdown-tool:${presentation.id}`} expanded={expanded}>
        <div className="markdown-tool-block">
          {markdownBlock ? (
            <MarkdownMessage content={markdownBlock} eager />
          ) : (
            <div className="markdown-tool-empty">{loadingDetail ? t("toolPresentation.output.loading") : t("toolPresentation.output.empty")}</div>
          )}
        </div>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// markdownToolCodeBlock wraps raw tool output in a Markdown code fence.
// markdownToolCodeBlock 将原始工具输出包裹为 Markdown 代码块。
function markdownToolCodeBlock(markdown: string): string {
  return `\`\`\`markdown\n${markdown}\n\`\`\``;
}

// MarkdownToolIcon renders the icon selected by the exact tool-id adapter.
// MarkdownToolIcon 渲染精确工具标识适配器选择的图标。
function MarkdownToolIcon(props: {
  // Icon slot selected by the presentation adapter.
  // 展示适配器选择的图标槽位。
  icon: MarkdownToolPresentationIcon;
  // CSS class attached to the icon.
  // 附加到图标上的 CSS 类名。
  className?: string;
  // Icon size in pixels.
  // 图标像素尺寸。
  size: number;
}) {
  if (props.icon === "ast_tree") {
    return <GitBranch className={props.className} size={props.size} />;
  }
  if (props.icon === "file_text") {
    return <FileText className={props.className} size={props.size} />;
  }
  if (props.icon === "ast_detail") {
    return <Code2 className={props.className} size={props.size} />;
  }
  if (props.icon === "search") {
    return <Search className={props.className} size={props.size} />;
  }
  return <FolderTree className={props.className} size={props.size} />;
}

// markdownToolStatusLabel returns the localized lifecycle label for Markdown-output cards.
// markdownToolStatusLabel 返回 Markdown 输出卡片的本地化生命周期文案。
function markdownToolStatusLabel(
  state: ToolPresentationDisplayState,
  t: (key: TranslationKey) => string,
): string {
  if (state === "running") return t("toolPresentation.markdownOutput.running");
  if (state === "failed") return t("toolPresentation.markdownOutput.failed");
  if (state === "stopped") return t("toolPresentation.markdownOutput.stopped");
  return t("toolPresentation.markdownOutput.completed");
}

// markdownToolPresentationDetailCacheKey returns a cache key for one Markdown-output detail reference.
// markdownToolPresentationDetailCacheKey 返回单条 Markdown 输出详情引用的缓存键。
function markdownToolPresentationDetailCacheKey(presentation: MarkdownToolPresentation): string | null {
  if (!presentation.detailRef) {
    return null;
  }
  return [
    presentation.detailRef.sessionId,
    presentation.detailRef.recordId ?? "",
    presentation.detailRef.toolCallId ?? "",
    presentation.detailRef.seq ?? "",
    presentation.detailRef.messageIndex ?? "",
  ].join(":");
}

// SubagentOrchestrationPresentationCard renders a compact inline subagent dispatch record.
// SubagentOrchestrationPresentationCard 渲染紧凑的内联子代理调度记录。
function SubagentOrchestrationPresentationCard(props: {
  // Subagent orchestration presentation model.
  // 子代理调度展示模型。
  presentation: SubagentOrchestrationPresentation;
  // Whether this row is expanded.
  // 当前行是否展开。
  expanded: boolean;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for full subagent results.
  // 加载单条隐藏工具详情记录用于完整子代理结果。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    expanded,
    onExpandedChange,
    onLoadToolDetail,
    presentation,
  } = props;
  const detailCacheKey = subagentOrchestrationPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<SubagentOrchestrationPresentation>(
    () => detailCacheKey ? subagentOrchestrationPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? subagentOrchestrationPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);
  React.useEffect(() => {
    if (!expanded || !presentation.detailRef || !detailCacheKey || subagentOrchestrationPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateSubagentOrchestrationPresentation(presentation, payload);
        subagentOrchestrationPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          subagentOrchestrationPresentationDetailCache.set(detailCacheKey, presentation);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);
  const handleToggle = React.useCallback(() => {
    if (!subagentPresentationHasExpandableDetail(hydratedPresentation)) {
      return;
    }
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, hydratedPresentation, onExpandedChange, presentation.id]);
  // canExpand prevents empty status rows from opening an empty detail area.
  // canExpand 避免空状态列表行展开出空详情区域。
  const canExpand = subagentPresentationHasExpandableDetail(hydratedPresentation);
  // waitUsageSummary keeps consumed wait_agent child usage visible before expansion.
  // waitUsageSummary 让 wait_agent 已消费的子智能体消耗在展开前也可见。
  const waitUsageSummary = hydratedPresentation.operation === "wait"
    ? subagentPresentationUsageSummary(hydratedPresentation)
    : null;
  return (
    <div className={cn("tool-presentation-card subagent-orchestration", canExpand && expanded && "expanded")}>
      <button
        aria-disabled={!canExpand}
        aria-expanded={canExpand && expanded}
        className="tool-presentation-summary subagent-orchestration-summary"
        onClick={handleToggle}
        title={hydratedPresentation.title}
        type="button"
      >
        <Bot className="subagent-orchestration-icon" size={15} />
        <span className={cn("tool-presentation-state", hydratedPresentation.state === "running" && "shimmer")}>{subagentOperationLabel(hydratedPresentation, t)}</span>
        <span className="tool-presentation-command">{subagentSummaryText(hydratedPresentation, t)}</span>
        {canExpand ? (
          expanded ? (
            <ChevronDown className="tool-presentation-toggle" size={15} />
          ) : (
            <ChevronRight className="tool-presentation-toggle" size={15} />
          )
        ) : null}
      </button>
      {waitUsageSummary ? (
        <div className="subagent-orchestration-compact-usage">
          <SubagentUsageChip summary={waitUsageSummary} t={t} />
        </div>
      ) : null}
      <ConversationMeasuredCollapse collapseId={`subagent:${presentation.id}`} expanded={canExpand && expanded}>
        <SubagentOrchestrationDetail presentation={hydratedPresentation} t={t} />
      </ConversationMeasuredCollapse>
    </div>
  );
}

// subagentPresentationHasExpandableDetail returns whether a subagent row has useful expanded content.
// subagentPresentationHasExpandableDetail 返回智能体行是否存在有价值的展开内容。
function subagentPresentationHasExpandableDetail(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
): boolean {
  if (
    presentation.operation === "list_agents"
    && presentation.agentsKnown
    && presentation.agents.length === 0
    && !presentation.result
    && !presentation.note
  ) {
    return false;
  }
  return true;
}

// subagentOrchestrationPresentationDetailCacheKey returns a stable cache key for hidden subagent details.
// subagentOrchestrationPresentationDetailCacheKey 返回隐藏子代理详情的稳定缓存键。
function subagentOrchestrationPresentationDetailCacheKey(
  // Subagent presentation model.
  // 子代理展示模型。
  presentation: SubagentOrchestrationPresentation,
): string | null {
  const detailRef = presentation.detailRef;
  if (!detailRef) {
    return null;
  }
  return [
    detailRef.sessionId,
    detailRef.recordId ?? "",
    detailRef.toolCallId ?? "",
    detailRef.seq ?? "",
    detailRef.messageIndex ?? "",
  ].join(":");
}

// SubagentOrchestrationDetail renders the expanded inline body without generic JSON sections.
// SubagentOrchestrationDetail 渲染不包含通用 JSON 区块的内联展开主体。
function SubagentOrchestrationDetail(props: {
  // Presentation to render.
  // 需要渲染的展示模型。
  presentation: SubagentOrchestrationPresentation;
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string;
}) {
  const { presentation, t } = props;
  if (presentation.operation === "wait") {
    return (
      <div className="subagent-orchestration-detail">
        {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
        <div className="subagent-orchestration-meta">
          <span>{t("conversation.subagent.detail.status")}</span>
          <strong>{subagentWaitStatusText(presentation, t)}</strong>
        </div>
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : null}
        {presentation.agents.length ? (
          <div className="subagent-orchestration-agent-list">
            {presentation.agents.map((agent) => (
              <SubagentOrchestrationAgentRow agent={agent} key={agent.id} t={t} />
            ))}
          </div>
        ) : null}
      </div>
    );
  }
  if (presentation.operation === "feedback") {
    return (
      <div className="subagent-orchestration-detail">
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : (
          <div className="subagent-orchestration-empty">{t("conversation.subagent.detail.noResult")}</div>
        )}
      </div>
    );
  }
  if (presentation.operation === "agent_orchestration_info") {
    return (
      <div className="subagent-orchestration-detail">
        {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : null}
        {presentation.agents.length ? (
          <div className="subagent-orchestration-agent-list">
            {presentation.agents.map((agent) => (
              <SubagentOrchestrationAgentRow agent={agent} key={agent.id} t={t} />
            ))}
          </div>
        ) : null}
      </div>
    );
  }
  if (presentation.operation === "list_agents") {
    const emptyOpenAgentStatus = presentation.agentsKnown
      && presentation.agents.length === 0
      && !presentation.result
      && !presentation.note;
    return (
      <div className="subagent-orchestration-detail">
        {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : null}
        {presentation.agents.length ? (
          <div className="subagent-orchestration-agent-list">
            {presentation.agents.map((agent) => (
              <SubagentOrchestrationAgentRow agent={agent} key={agent.id} t={t} />
            ))}
          </div>
        ) : null}
        {emptyOpenAgentStatus ? (
          <div className="subagent-orchestration-empty">{t("conversation.subagent.detail.noOpenAgents")}</div>
        ) : null}
      </div>
    );
  }
  // resultText removes model-facing dispatch acknowledgements from create rows.
  // resultText 从创建行中移除面向模型的调度确认内容。
  const resultText = presentation.operation === "create" ? null : presentation.result ?? null;
  // showResult controls whether model-facing dispatch acknowledgements are visible.
  // showResult 控制是否展示面向模型的调度确认内容。
  const showResult = Boolean(resultText);
  // showStoppedCount records whether the stop count adds useful information to close or interrupt rows.
  // showStoppedCount 记录停止数量是否为关闭或停止行补充了有效信息。
  const showStoppedCount = (presentation.operation === "close" || presentation.operation === "interrupt")
    && presentation.stoppedCount > 0;
  // hasVisibleBody records whether the expanded area has user-facing content.
  // hasVisibleBody 记录展开区是否存在面向用户的内容。
  const hasVisibleBody = Boolean(
    presentation.prompt
    || presentation.effectivePrompt
    || presentation.debugSummary
    || showResult
    || presentation.operation === "close"
    || presentation.operation === "interrupt",
  );
  return (
    <div className="subagent-orchestration-detail">
      {presentation.queued !== null && presentation.queued !== undefined ? (
        <div className="subagent-orchestration-meta">
          <span>{t("conversation.subagent.detail.queued")}</span>
          <strong>{presentation.queued ? t("conversation.subagent.detail.yes") : t("conversation.subagent.detail.no")}</strong>
        </div>
      ) : null}
      {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
      {presentation.prompt ? (
        <SubagentInlineTextLine
          label={t(presentation.operation === "message" ? "conversation.subagent.detail.message" : "conversation.subagent.detail.prompt")}
          text={presentation.prompt}
        />
      ) : null}
      {presentation.effectivePrompt && presentation.effectivePrompt !== presentation.prompt ? (
        <SubagentInlineTextLine label={t("conversation.subagent.detail.effectivePrompt")} text={presentation.effectivePrompt} />
      ) : null}
      {presentation.debugSummary ? (
        <SubagentInlineTextLine label={t("conversation.subagent.detail.debug")} text={presentation.debugSummary} />
      ) : null}
      {resultText ? (
        <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={resultText} />
      ) : null}
      {presentation.operation === "close" || presentation.operation === "interrupt" ? (
        <>
          {presentation.operation === "close" && (presentation.closedCount > 0 || presentation.closedAgents.length > 0) ? (
            <div className="subagent-orchestration-meta">
              <span>{t("conversation.subagent.detail.closedCount")}</span>
              <strong>{presentation.closedCount}</strong>
            </div>
          ) : null}
          {showStoppedCount ? (
            <div className="subagent-orchestration-meta">
              <span>{t("conversation.subagent.detail.stoppedCount")}</span>
              <strong>{presentation.stoppedCount}</strong>
            </div>
          ) : null}
          {presentation.clearedMessageCount !== null && presentation.clearedMessageCount !== undefined ? (
            <div className="subagent-orchestration-meta">
              <span>{t("conversation.subagent.detail.clearedMessages")}</span>
              <strong>{presentation.clearedMessageCount}</strong>
            </div>
          ) : null}
        </>
      ) : null}
      {!hasVisibleBody ? (
        <div className="subagent-orchestration-empty">{t("conversation.subagent.detail.empty")}</div>
      ) : null}
    </div>
  );
}

// SubagentOrchestrationAgentRow renders one listed child-agent status row.
// SubagentOrchestrationAgentRow 渲染单条已列出的子代理状态行。
function SubagentOrchestrationAgentRow(props: {
  // Agent row model.
  // 代理行模型。
  agent: SubagentOrchestrationAgent;
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string;
}) {
  return (
    <div className="subagent-orchestration-agent">
      <div className="subagent-orchestration-agent-heading">
        <strong>{props.agent.name}</strong>
        <span>{props.agent.status}</span>
      </div>
      {props.agent.debugSummary ? (
        <SubagentInlineTextLine label={props.t("conversation.subagent.detail.debug")} text={props.agent.debugSummary} />
      ) : null}
      {props.agent.usageSummary ? (
        <SubagentUsageChip summary={props.agent.usageSummary} t={props.t} />
      ) : null}
      {props.agent.result ? (
        <pre className="subagent-orchestration-text">{compactSubagentText(props.agent.result)}</pre>
      ) : (
        <div className="subagent-orchestration-empty">{props.t("conversation.subagent.detail.noResult")}</div>
      )}
    </div>
  );
}

// SubagentUsageChip renders one compact child-agent usage row.
// SubagentUsageChip 渲染一条紧凑子智能体消耗统计行。
function SubagentUsageChip(props: {
  // Aggregated child-agent usage.
  // 聚合后的子智能体消耗。
  summary: SubagentUsageSummary;
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string;
}) {
  return (
    <div className="subagent-orchestration-usage">
      <Bot aria-hidden="true" size={13} />
      <span>{subagentUsageSummaryText(props.summary, props.t)}</span>
    </div>
  );
}

// SubagentInlineTextLine renders one unframed prompt-style line.
// SubagentInlineTextLine 渲染一条无边框的提示词样式文本。
function SubagentInlineTextLine(props: {
  // Line label.
  // 文本行标签。
  label: string;
  // Raw text.
  // 原始文本。
  text: string;
}) {
  return (
    <div className="subagent-orchestration-inline-text">
      <span>{props.label}：</span>
      <code>{compactSubagentText(props.text)}</code>
    </div>
  );
}

// SubagentTextBlock renders one compact prompt or result text block.
// SubagentTextBlock 渲染一段紧凑提示词或结果文本。
function SubagentTextBlock(props: {
  // Block label.
  // 文本块标签。
  label: string;
  // Raw text.
  // 原始文本。
  text: string;
}) {
  return (
    <div className="subagent-orchestration-block">
      <div className="subagent-orchestration-block-label">{props.label}</div>
      <pre className="subagent-orchestration-text">{compactSubagentText(props.text)}</pre>
    </div>
  );
}

// subagentOperationLabel returns the compact operation label.
// subagentOperationLabel 返回紧凑操作文案。
function subagentOperationLabel(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  if (presentation.operation === "wait") return t("conversation.subagent.detail.wait");
  if (presentation.operation === "message") return t("conversation.subagent.detail.message");
  if (presentation.operation === "agent_orchestration_info") return t("conversation.subagent.detail.templateCollection");
  if (presentation.operation === "list_agents") return t("conversation.subagent.detail.statusCollection");
  if (presentation.operation === "feedback") return t("conversation.subagent.detail.feedback");
  if (presentation.operation === "close") return t("conversation.subagent.detail.closeControl");
  if (presentation.operation === "interrupt") return t("conversation.subagent.detail.interruptControl");
  return t("conversation.subagent.call");
}

// subagentSummaryText returns a terse row summary.
// subagentSummaryText 返回简短行摘要。
function subagentSummaryText(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  if (presentation.operation === "wait") {
    return subagentWaitStatusText(presentation, t);
  }
  if (presentation.operation === "agent_orchestration_info") {
    if (!presentation.agentsKnown) {
      return t("conversation.subagent.detail.templateCollection");
    }
    if (presentation.agents.length === 0) {
      return t("conversation.subagent.detail.templateCollection");
    }
    return `${presentation.agents.length} ${t("conversation.subagent.detail.agent")}`;
  }
  if (presentation.operation === "list_agents") {
    if (!presentation.agentsKnown) {
      return t("conversation.subagent.detail.statusCollection");
    }
    if (presentation.agents.length === 0) {
      return t("conversation.subagent.detail.noOpenAgents");
    }
    return `${presentation.agents.length} ${t("conversation.subagent.detail.agent")}`;
  }
  if (presentation.operation === "feedback") {
    return t("conversation.subagent.detail.openFeedback");
  }
  if (presentation.operation === "message") {
    return presentation.target ?? presentation.title;
  }
  if (presentation.operation === "close") {
    if (presentation.closedCount > 0) {
      return `${presentation.closedCount} ${t("conversation.subagent.detail.closedAgents")}`;
    }
    return presentation.target ?? t("conversation.subagent.detail.closeControl");
  }
  if (presentation.operation === "interrupt") {
    if (presentation.stoppedCount > 0) {
      return presentation.target ?? `${presentation.stoppedCount}`;
    }
    return presentation.target ?? t("conversation.subagent.detail.interruptControl");
  }
  return presentation.title;
}

// subagentWaitStatusText maps wait_agent activity to a non-misleading compact label.
// subagentWaitStatusText 将 wait_agent 活动映射为不误导的紧凑文案。
function subagentWaitStatusText(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  if (presentation.timedOut) {
    return t("conversation.subagent.detail.waitTimedOut");
  }
  if (presentation.interrupted) {
    return t("conversation.subagent.detail.waitInterrupted");
  }
  if (presentation.terminalEventReceived) {
    return t("conversation.subagent.detail.waitCompleted");
  }
  return t("conversation.subagent.detail.waitPending");
}

// subagentUsageSummaryText formats child-agent usage without regular-turn group labels.
// subagentUsageSummaryText 格式化不带常规回合分组文案的子智能体消耗。
function subagentUsageSummaryText(
  // Aggregated child-agent usage.
  // 聚合后的子智能体消耗。
  summary: SubagentUsageSummary,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  const parts = [
    formatSubagentUsageTranslation(t("conversation.stats.providerCalls"), summary.providerCallCount),
    formatSubagentUsageTranslation(t("conversation.stats.toolCalls"), summary.toolCallCount),
  ];
  if (summary.totalTokens > 0) {
    parts.push(formatSubagentUsageTranslation(t("conversation.stats.totalTokens"), summary.totalTokens));
  }
  if (summary.cachedInputTokens > 0) {
    parts.push(formatSubagentUsageTranslation(t("conversation.stats.cachedTokensCompact"), summary.cachedInputTokens));
  }
  return parts.join(" · ");
}

// subagentPresentationUsageSummary aggregates visible child-agent usage rows.
// subagentPresentationUsageSummary 聚合可见子智能体行的消耗统计。
function subagentPresentationUsageSummary(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
): SubagentUsageSummary | null {
  const summary: SubagentUsageSummary = {
    providerCallCount: 0,
    toolCallCount: 0,
    totalTokens: 0,
    cachedInputTokens: 0,
  };
  for (const agent of presentation.agents) {
    if (!agent.usageSummary) {
      continue;
    }
    summary.providerCallCount += agent.usageSummary.providerCallCount;
    summary.toolCallCount += agent.usageSummary.toolCallCount;
    summary.totalTokens += agent.usageSummary.totalTokens;
    summary.cachedInputTokens += agent.usageSummary.cachedInputTokens;
  }
  return summary.providerCallCount > 0
    || summary.toolCallCount > 0
    || summary.totalTokens > 0
    || summary.cachedInputTokens > 0
    ? summary
    : null;
}

// formatSubagentUsageTranslation injects a count into a simple i18n template.
// formatSubagentUsageTranslation 向简单国际化模板注入数量。
function formatSubagentUsageTranslation(
  // Template containing the count placeholder.
  // 包含数量占位符的模板。
  template: string,
  // Count value to inject.
  // 需要注入的数量。
  count: number,
): string {
  return template.replaceAll("{count}", String(count));
}

// compactSubagentText limits inline subagent text to keep transcript rows light.
// compactSubagentText 限制内联子代理文本长度，让会话行保持轻量。
function compactSubagentText(
  // Source text.
  // 源文本。
  source: string,
): string {
  const normalized = source.trim();
  return normalized.length > 1600 ? `${normalized.slice(0, 1600)}\n...` : normalized;
}

// FILE_CHANGE_OPERATION_LABEL_KEYS maps file-change operations and phases to checked translation keys.
// FILE_CHANGE_OPERATION_LABEL_KEYS 将文件变更操作与阶段映射到受类型检查的翻译键。
const FILE_CHANGE_OPERATION_LABEL_KEYS: Record<FileChangeOperation, Record<"running" | "failed" | "preview" | "completed", TranslationKey>> = {
  create: {
    running: "toolPresentation.file.create.running",
    failed: "toolPresentation.file.create.failed",
    preview: "toolPresentation.file.create.preview",
    completed: "toolPresentation.file.create.completed",
  },
  modify: {
    running: "toolPresentation.file.modify.running",
    failed: "toolPresentation.file.modify.failed",
    preview: "toolPresentation.file.modify.preview",
    completed: "toolPresentation.file.modify.completed",
  },
  delete: {
    running: "toolPresentation.file.delete.running",
    failed: "toolPresentation.file.delete.failed",
    preview: "toolPresentation.file.delete.preview",
    completed: "toolPresentation.file.delete.completed",
  },
  mixed: {
    running: "toolPresentation.file.mixed.running",
    failed: "toolPresentation.file.mixed.failed",
    preview: "toolPresentation.file.mixed.preview",
    completed: "toolPresentation.file.mixed.completed",
  },
};

// CommandExecutionPresentationCard renders the command-line specific compact and expanded states.
// CommandExecutionPresentationCard 渲染命令行专用的紧凑与展开状态。
function CommandExecutionPresentationCard(props: {
  // Command execution presentation model.
  // 命令执行展示模型。
  presentation: CommandExecutionPresentation;
  // Whether this command card is expanded.
  // 当前命令卡片是否展开。
  expanded: boolean;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for command output.
  // 加载单条隐藏工具详情记录用于命令输出。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    expanded,
    onExpandedChange,
    onLoadToolDetail,
    presentation,
  } = props;
  const detailCacheKey = commandPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<CommandExecutionPresentation>(
    () => detailCacheKey ? commandPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  const [loadingDetail, setLoadingDetail] = React.useState(false);
  React.useEffect(() => {
    const cached = detailCacheKey ? commandPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);

  React.useEffect(() => {
    if (!expanded || !presentation.detailRef || !detailCacheKey || commandPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    setLoadingDetail(true);
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateCommandExecutionPresentation(presentation, payload);
        commandPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          setHydratedPresentation(presentation);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingDetail(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);

  const outputText = commandOutputText(hydratedPresentation);
  const exitCodeLabel = hydratedPresentation.exitCode === null ? "-" : String(hydratedPresentation.exitCode);
  const exitCodeState = commandExitCodeState(hydratedPresentation.exitCode);
  const terminalState = hydratedPresentation.terminalState;
  const statusLabel = hydratedPresentation.commandSessionAction
    ? commandSessionActionStatusLabel(hydratedPresentation.commandSessionAction, hydratedPresentation.state, t)
    : commandPresentationStatusLabel(hydratedPresentation.state, t);
  const sessionMetaLabel = commandSessionMetaLabel(hydratedPresentation, t);
  const detailHeading = commandDetailHeading(hydratedPresentation, t);
  const detailCommandText = commandDetailText(hydratedPresentation);
  // pluginContributions preserves every badge contributed by the ordered hook chain.
  // pluginContributions 保留有序 Hook 链贡献的全部徽标。
  const pluginContributions = hydratedPresentation.pluginContributions ?? [];
  // commandTitle exposes both the original command and generic contribution details without plugin-specific fields.
  // commandTitle 在不引入插件专用字段的前提下展示原始命令与通用贡献详情。
  const commandTitle = [
    hydratedPresentation.command,
    ...pluginContributions.map((contribution) => `${contribution.label}: ${contribution.value}`),
  ].join("\n");
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);

  return (
    <div className={cn("tool-presentation-card", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary"
        onClick={handleToggle}
        title={commandTitle}
        type="button"
      >
        <TerminalSquare className={cn("terminal-session-icon", terminalState && terminalStateClass(terminalState))} size={15} />
        <span className={cn("tool-presentation-state", hydratedPresentation.state === "running" && "shimmer")}>{statusLabel}</span>
        {pluginContributions.map((contribution, index) => contribution.badge ? (
          <span
            className="tool-presentation-route-badge"
            key={`${contribution.badge}:${contribution.label}:${index}`}
            title={`${contribution.label}: ${contribution.value}`}
          >
            {contribution.badge}
          </span>
        ) : null)}
        <span className="tool-presentation-command">{hydratedPresentation.command}</span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`command-detail:${presentation.id}`} expanded={expanded}>
        <div className="tool-presentation-detail">
          <pre className="tool-presentation-command-line"><span>{detailHeading}</span> {detailCommandText}</pre>
          {hydratedPresentation.cwd || hydratedPresentation.shellLabel ? (
            <div className="tool-presentation-terminal-meta">
              {hydratedPresentation.shellLabel ? <span>{hydratedPresentation.shellLabel}</span> : null}
              {hydratedPresentation.cwd ? <span>{hydratedPresentation.cwd}</span> : null}
            </div>
          ) : null}
          <div className="tool-presentation-output-scroll">
            <TerminalOutputView
              className="tool-presentation-output-text"
              emptyText={loadingDetail ? t("toolPresentation.output.loading") : t("toolPresentation.output.empty")}
              text={outputText}
            />
          </div>
          {hydratedPresentation.failureReason ? (
            <div className="tool-presentation-terminal-error">{hydratedPresentation.failureReason}</div>
          ) : null}
          <div className="tool-presentation-footer">
            <span>{sessionMetaLabel}</span>
            {hydratedPresentation.exitCode !== null ? (
              <span className={cn("tool-presentation-exit-code", exitCodeState)}>{t("toolPresentation.exitCode")} {exitCodeLabel}</span>
            ) : null}
          </div>
        </div>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// commandSessionActionStatusLabel returns an action result label instead of the session lifecycle label.
// commandSessionActionStatusLabel 返回动作结果文案，而不是会话生命周期文案。
function commandSessionActionStatusLabel(
  action: string,
  state: CommandExecutionPresentation["state"],
  t: (key: TranslationKey) => string,
): string {
  if (state === "failed") {
    if (action === "start") return t("toolPresentation.command.start.failed");
    if (action === "read") return t("toolPresentation.command.read.failed");
    if (action === "send") return t("toolPresentation.command.send.failed");
    if (action === "write") return t("toolPresentation.command.write.failed");
    if (action === "stop") return t("toolPresentation.command.stop.failed");
    return t("toolPresentation.command.failed");
  }
  if (state === "running") {
    if (action === "start") return t("toolPresentation.command.start.running");
    if (action === "read") return t("toolPresentation.command.read.running");
    if (action === "send") return t("toolPresentation.command.send.running");
    if (action === "write") return t("toolPresentation.command.write.running");
    if (action === "stop") return t("toolPresentation.command.stop.running");
    return t("toolPresentation.command.running");
  }
  if (action === "start") return t("toolPresentation.command.start.completed");
  if (action === "list") return t("toolPresentation.command.list.completed");
  if (action === "read") return t("toolPresentation.command.read.completed");
  if (action === "send") return t("toolPresentation.command.send.completed");
  if (action === "write") return t("toolPresentation.command.write.completed");
  if (action === "stop") return t("toolPresentation.command.stop.completed");
  return commandPresentationStatusLabel(state, t);
}

// commandDetailHeading names the expanded content by command-session action.
// commandDetailHeading 根据命令会话动作命名展开内容。
function commandDetailHeading(
  presentation: CommandExecutionPresentation,
  t: (key: TranslationKey) => string,
): string {
  if (presentation.commandSessionAction === "start") return t("toolPresentation.command.heading.start");
  if (presentation.commandSessionAction === "read") return t("toolPresentation.command.heading.read");
  if (presentation.commandSessionAction === "send") return t("toolPresentation.command.heading.send");
  if (presentation.commandSessionAction === "write") return t("toolPresentation.command.heading.write");
  if (presentation.commandSessionAction === "stop") return t("toolPresentation.command.heading.stop");
  return "$";
}

// commandDetailText returns the primary text shown on the first expanded line.
// commandDetailText 返回展开区首行展示的主要文本。
function commandDetailText(presentation: CommandExecutionPresentation): string {
  if (presentation.commandSessionAction === "write" || presentation.commandSessionAction === "send") {
    return presentation.commandSessionInput?.trim() || presentation.command;
  }
  return presentation.effectiveCommand?.trim() || presentation.command;
}

// commandSessionMetaLabel builds the footer text without making old actions look failed.
// commandSessionMetaLabel 构建页脚文本，避免让历史动作看起来失败。
function commandSessionMetaLabel(
  presentation: CommandExecutionPresentation,
  t: (key: TranslationKey) => string,
): string {
  if (!presentation.commandSessionId) {
    return "";
  }
  const id = presentation.commandSessionId.length > 8 ? presentation.commandSessionId.slice(0, 8) : presentation.commandSessionId;
  if (presentation.lost) {
    return `${t("toolPresentation.command.session")} ${id} · ${t("toolPresentation.command.lost")}`;
  }
  if (presentation.terminalState) {
    const currentState = formatTranslation(t, "toolPresentation.command.currentState", {
      state: localizedTerminalStateLabel(presentation.terminalState, t),
    });
    return `${t("toolPresentation.command.session")} ${id} · ${currentState}`;
  }
  return `${t("toolPresentation.command.session")} ${id}`;
}

// commandPresentationStatusLabel returns the localized command lifecycle label.
// commandPresentationStatusLabel 返回本地化后的命令生命周期文案。
function commandPresentationStatusLabel(
  state: ToolPresentationDisplayState,
  t: (key: TranslationKey) => string,
): string {
  if (state === "running") return t("toolPresentation.command.running");
  if (state === "failed") return t("toolPresentation.command.failed");
  if (state === "stopped") return t("toolPresentation.command.stopped");
  return t("toolPresentation.command.completed");
}

// localizedTerminalStateLabel returns the localized terminal lifecycle label.
// localizedTerminalStateLabel 返回本地化后的终端生命周期文案。
function localizedTerminalStateLabel(
  state: TerminalVisualState,
  t: (key: TranslationKey) => string,
): string {
  if (state === "running") return t("terminalState.running");
  if (state === "waiting") return t("terminalState.waiting");
  if (state === "success") return t("terminalState.success");
  if (state === "failed") return t("terminalState.failed");
  if (state === "stopped") return t("terminalState.stopped");
  if (state === "lost") return t("terminalState.lost");
  return t("terminalState.unknown");
}

// AskQuestionPresentationCard renders compact and expanded ask/question display.
// AskQuestionPresentationCard 渲染 ask/question 的紧凑与展开展示。
function AskQuestionPresentationCard(props: {
  // Ask/question presentation model.
  // ask/question 展示模型。
  presentation: AskQuestionPresentation;
  // Whether this ask card is expanded.
  // 当前提问卡片是否展开。
  expanded: boolean;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for completed answers.
  // 加载单条隐藏工具详情记录用于补全完成态答案。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    expanded,
    onExpandedChange,
    onLoadToolDetail,
    presentation,
  } = props;
  const detailCacheKey = askQuestionPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<AskQuestionPresentation>(
    () => detailCacheKey ? askQuestionPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? askQuestionPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);

  React.useEffect(() => {
    if (!expanded || !presentation.detailRef || !detailCacheKey || askQuestionPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateAskQuestionPresentation(presentation, payload);
        askQuestionPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          setHydratedPresentation(presentation);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);

  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);

  return (
    <div className={cn("tool-presentation-card ask-question", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary ask-question-summary"
        onClick={handleToggle}
        title={hydratedPresentation.questions.map((question) => question.question).join("\n")}
        type="button"
      >
        <MessageCircleQuestionMark className="ask-question-icon" size={15} />
        <span className={cn("tool-presentation-state", hydratedPresentation.state === "running" && "shimmer")}>{askQuestionStatusLabel(hydratedPresentation.state, t)}</span>
        <span className="tool-presentation-command">{hydratedPresentation.title}</span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`ask-question:${presentation.id}`} expanded={expanded}>
        <div className="ask-question-detail">
          {hydratedPresentation.questions.map((question) => (
            <AskQuestionAnswerItem
              ignoredAnswerText={hydratedPresentation.ignoredAnswerText}
              item={question}
              key={question.id}
              state={hydratedPresentation.state}
              t={t}
            />
          ))}
        </div>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// askQuestionPresentationDetailCacheKey returns a cache key for one ask detail reference.
// askQuestionPresentationDetailCacheKey 返回单条 ask 详情引用的缓存键。
function askQuestionPresentationDetailCacheKey(presentation: AskQuestionPresentation): string | null {
  if (!presentation.detailRef) {
    return null;
  }
  return `${presentation.detailRef.recordId ?? ""}:${presentation.detailRef.seq ?? ""}:${presentation.detailRef.messageIndex ?? ""}`;
}

// AskQuestionAnswerItem renders one expanded question and its user answer.
// AskQuestionAnswerItem 渲染展开区中的单个问题与用户回答。
function AskQuestionAnswerItem(props: {
  // Question-answer item.
  // 问答条目。
  item: AskQuestionPresentationItem;
  // Placeholder for ignored answers.
  // 忽略回答时的占位文案。
  ignoredAnswerText: string;
  // Parent presentation state.
  // 父级展示状态。
  state: AskQuestionPresentation["state"];
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  const answerText = askQuestionAnswerText(props.item, props.ignoredAnswerText, props.state, props.t);
  return (
    <div className="ask-question-item">
      <div className="ask-question-row">
        <span className="ask-question-label">{props.t("toolPresentation.ask.questionLabel")}</span>
        <span className="ask-question-text">{props.item.question}</span>
      </div>
      <div className={cn("ask-question-row", !props.item.answered && props.state !== "running" && "ignored")}>
        <span className="ask-question-label">{props.t("toolPresentation.ask.answerLabel")}</span>
        <span className="ask-question-text">{answerText}</span>
      </div>
    </div>
  );
}

// askQuestionAnswerText chooses the visible answer text for running, answered, and ignored states.
// askQuestionAnswerText 为等待中、已回答与忽略状态选择可见答案文本。
function askQuestionAnswerText(
  item: AskQuestionPresentationItem,
  ignoredAnswerText: string,
  state: AskQuestionPresentation["state"],
  t: (key: TranslationKey) => string,
): string {
  if (item.answered && item.answers.length) {
    return item.answers.join(", ");
  }
  if (state === "running") {
    return t("toolPresentation.ask.waitingAnswer");
  }
  return ignoredAnswerText;
}

// askQuestionStatusLabel returns the localized ask lifecycle label.
// askQuestionStatusLabel 返回本地化后的 ask 生命周期文案。
function askQuestionStatusLabel(
  state: ToolPresentationDisplayState,
  t: (key: TranslationKey) => string,
): string {
  if (state === "running") return t("toolPresentation.ask.running");
  if (state === "failed" || state === "stopped") return t("toolPresentation.ask.ignored");
  return t("toolPresentation.ask.completed");
}

// FileChangePresentationCard renders compact file operation rows and lazy diff bodies.
// FileChangePresentationCard 渲染紧凑文件操作行与懒加载 diff 内容。
function FileChangePresentationCard(props: {
  // File change presentation model.
  // 文件变更展示模型。
  presentation: FileChangePresentation;
  // Whether the top-level row is expanded.
  // 顶层行是否展开。
  expanded: boolean;
  // Expansion ids shared by the conversation surface.
  // 与会话界面共享的展开标识集合。
  expandedToolPresentationIds: Set<string>;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    expanded,
    expandedToolPresentationIds,
    onExpandedChange,
    presentation,
  } = props;
  const [errorExpanded, setErrorExpanded] = React.useState(() => presentation.state === "failed" && Boolean(presentation.errorDetail));
  React.useEffect(() => {
    setErrorExpanded(presentation.state === "failed" && Boolean(presentation.errorDetail));
  }, [presentation.errorDetail, presentation.id, presentation.state]);
  const effectiveExpanded = presentation.state === "failed" && presentation.errorDetail ? errorExpanded : expanded;
  const singleFile = presentation.files.length === 1 ? presentation.files[0] : null;
  const handleTopToggle = React.useCallback(() => {
    if (!presentation.interactive) {
      return;
    }
    dispatchConversationManualViewportInteraction();
    if (presentation.state === "failed" && presentation.errorDetail) {
      setErrorExpanded((value) => !value);
      return;
    }
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation]);

  if (singleFile) {
    return (
      <div className={cn("tool-presentation-card file-change", effectiveExpanded && "expanded", !presentation.interactive && "disabled")}>
        <FileChangeSummaryButton
          expanded={effectiveExpanded}
          file={singleFile}
          interactive={presentation.interactive}
          presentation={presentation}
          t={t}
          onToggle={handleTopToggle}
        />
        <ConversationMeasuredCollapse collapseId={`file-change:${presentation.id}:single`} expanded={effectiveExpanded}>
          <FileChangeExpandableBody
            errorDetail={presentation.state === "failed" ? presentation.errorDetail : null}
            file={singleFile}
            t={t}
          />
        </ConversationMeasuredCollapse>
      </div>
    );
  }

  return (
    <div className={cn("tool-presentation-card file-change", effectiveExpanded && "expanded", !presentation.interactive && "disabled")}>
      <button
        aria-expanded={effectiveExpanded}
        className="tool-presentation-summary file-change-summary"
        disabled={!presentation.interactive}
        onClick={handleTopToggle}
        title={presentation.files.map((file) => file.displayPath).join("\n")}
        type="button"
      >
        <FileChangeOperationIcon operation={presentation.operation} />
        <span className={cn("tool-presentation-state", presentation.state === "running" && "shimmer")}>{fileChangeStatusLabel(presentation, presentation.operation, t)} {formatTranslation(t, "toolPresentation.file.count", { count: String(presentation.files.length) })}</span>
        <span className="file-change-path multi">
          <span className="file-change-path-text">{t("toolPresentation.file.multiChange")}</span>
          <FileChangeCounts added={presentation.totalAdded} failed={presentation.state === "failed"} removed={presentation.totalRemoved} />
        </span>
        {presentation.interactive ? (effectiveExpanded ? <ChevronDown className="tool-presentation-toggle file-change-toggle" size={15} /> : <ChevronRight className="tool-presentation-toggle file-change-toggle" size={15} />) : <span />}
      </button>
      <ConversationMeasuredCollapse collapseId={`file-change:${presentation.id}:tree`} expanded={effectiveExpanded}>
        <div className="file-change-tree">
          {presentation.state === "failed" && presentation.errorDetail ? (
            <FileChangeErrorDetail detail={presentation.errorDetail} />
          ) : presentation.files.map((file) => {
            const fileExpanded = expandedToolPresentationIds.has(fileExpansionId(presentation, file));
            return (
              <div className="file-change-tree-item" key={file.id}>
                <FileChangeSummaryButton
                  expanded={fileExpanded}
                  file={file}
                  interactive={file.expandable}
                  presentation={presentation}
                  t={t}
                  showOperationIcon={false}
                  showStatusLabel={false}
                  onToggle={() => {
                    if (!file.expandable) {
                      return;
                    }
                    dispatchConversationManualViewportInteraction();
                    onExpandedChange(fileExpansionId(presentation, file), !fileExpanded);
                  }}
                />
                <ConversationMeasuredCollapse collapseId={fileExpansionId(presentation, file)} expanded={fileExpanded}>
                  <FileChangeExpandableBody file={file} t={t} />
                </ConversationMeasuredCollapse>
              </div>
            );
          })}
        </div>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// FileChangeSummaryButton renders one clickable or disabled file summary row.
// FileChangeSummaryButton 渲染单条可点击或禁用的文件摘要行。
function FileChangeSummaryButton(props: {
  // Whether this row is expanded.
  // 当前行是否展开。
  expanded: boolean;
  // File row model.
  // 文件行模型。
  file: FileChangeFilePresentation;
  // Whether the row may be toggled.
  // 当前行是否允许切换。
  interactive: boolean;
  // Parent file change presentation.
  // 父级文件变更展示。
  presentation: FileChangePresentation;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
  // Whether to show the left operation icon before the status label.
  // 是否在状态文案前显示左侧操作图标。
  showOperationIcon?: boolean;
  // Whether to show the operation status label before the file name.
  // 是否在文件名前显示操作状态文案。
  showStatusLabel?: boolean;
  // Toggle handler for interactive rows.
  // 可交互行的切换处理器。
  onToggle: () => void;
}) {
  const showOperationIcon = props.showOperationIcon ?? true;
  const showStatusLabel = props.showStatusLabel ?? true;
  return (
    <button
      aria-expanded={props.expanded}
      className={cn(
        "tool-presentation-summary file-change-summary",
        !showOperationIcon && "without-operation-icon",
        !showOperationIcon && !showStatusLabel && "file-only",
      )}
      disabled={!props.interactive}
      onClick={props.onToggle}
      title={props.file.path}
      type="button"
    >
      {showOperationIcon ? <FileChangeOperationIcon operation={props.file.operation} /> : null}
      {showStatusLabel ? <span className={cn("tool-presentation-state", props.presentation.state === "running" && "shimmer")}>{fileChangeStatusLabel(props.presentation, props.file.operation, props.t)}</span> : null}
      <span className="file-change-path">
        <span className="file-change-icon-stack"><FileTypeIcon icon={props.file.icon} /></span>
        <span className="file-change-path-text">{props.file.displayPath}</span>
        <FileChangeCounts added={props.file.added} failed={props.presentation.state === "failed"} removed={props.file.removed} />
      </span>
      {props.interactive ? (props.expanded ? <ChevronDown className="tool-presentation-toggle file-change-toggle" size={15} /> : <ChevronRight className="tool-presentation-toggle file-change-toggle" size={15} />) : <span />}
    </button>
  );
}

// FileChangeOperationIcon renders the leftmost operation icon for file writes and edits.
// FileChangeOperationIcon 渲染文件写入与编辑行最左侧的操作图标。
function FileChangeOperationIcon(props: {
  // Operation represented by the row.
  // 当前行代表的操作类型。
  operation: FileChangeOperation;
}) {
  if (props.operation === "delete") {
    return <Trash2 className="file-change-operation-icon" size={15} />;
  }
  return <Pencil className="file-change-operation-icon" size={15} />;
}

// FileChangeCounts renders added, removed, and error status tokens.
// FileChangeCounts 渲染新增、删除与错误状态标记。
function FileChangeCounts(props: {
  // Added line count.
  // 新增行数。
  added: number | null;
  // Whether an error badge should be displayed.
  // 是否展示错误徽标。
  failed: boolean;
  // Removed line count.
  // 删除行数。
  removed: number | null;
}) {
  return (
    <span className="file-change-counts">
      {typeof props.added === "number" && props.added > 0 ? <span className="file-change-added">+{props.added}</span> : null}
      {typeof props.removed === "number" && props.removed > 0 ? <span className="file-change-removed">-{props.removed}</span> : null}
      {props.failed ? <span className="file-change-error-badge">ERR</span> : null}
    </span>
  );
}

// fileChangeStatusLabel returns the localized file-change lifecycle label.
// fileChangeStatusLabel 返回本地化后的文件变更生命周期文案。
function fileChangeStatusLabel(
  presentation: FileChangePresentation,
  operation: FileChangeOperation,
  t: (key: TranslationKey) => string,
): string {
  if (presentation.state === "running") {
    return fileChangeOperationLabel(operation, "running", t);
  }
  if (presentation.state === "failed") {
    return fileChangeOperationLabel(operation, "failed", t);
  }
  if (presentation.mode === "preview") {
    return fileChangeOperationLabel(operation, "preview", t);
  }
  return fileChangeOperationLabel(operation, "completed", t);
}

// fileChangeOperationLabel maps one operation and phase to a stable translation key.
// fileChangeOperationLabel 将操作与阶段映射为稳定翻译键。
function fileChangeOperationLabel(
  operation: FileChangeOperation,
  phase: "running" | "failed" | "preview" | "completed",
  t: (key: TranslationKey) => string,
): string {
  return t(FILE_CHANGE_OPERATION_LABEL_KEYS[operation][phase]);
}

// FileChangeExpandableBody renders either an error detail or file diff rows.
// FileChangeExpandableBody 渲染错误详情或文件 diff 行。
function FileChangeExpandableBody(props: {
  // Optional error detail replacing normal diff rows.
  // 替代普通 diff 行的可选错误详情。
  errorDetail?: string | null;
  // File row model.
  // 文件行模型。
  file: FileChangeFilePresentation;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  if (props.errorDetail) {
    return <FileChangeErrorDetail detail={props.errorDetail} />;
  }
  if (props.file.diffFile) {
    return (
      <div className="file-change-detail review">
        <ChangeSetDiffPreview file={props.file.diffFile} />
      </div>
    );
  }
  return (
    <div className="file-change-detail">
      <div className="file-change-code-scroll">
        <div className="file-change-empty">{props.t("toolPresentation.file.empty")}</div>
      </div>
    </div>
  );
}

// FileChangeErrorDetail renders failed native vulcan-file details without opening the generic modal.
// FileChangeErrorDetail 渲染失败原生 vulcan-file 详情，避免打开通用弹窗。
function FileChangeErrorDetail(props: {
  // Error detail text.
  // 错误详情文本。
  detail: string;
}) {
  return (
    <div className="file-change-detail error">
      <pre className="file-change-error-text">{props.detail}</pre>
    </div>
  );
}

// fileExpansionId builds the child expansion id used by multi-file rows.
// fileExpansionId 构建多文件行使用的子级展开标识。
function fileExpansionId(presentation: FileChangePresentation, file: FileChangeFilePresentation): string {
  return `${presentation.id}:file:${file.id}`;
}

// formatTranslation replaces simple named placeholders in localized UI templates.
// formatTranslation 替换本地化 UI 模板中的简单具名占位符。
function formatTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

// commandOutputText combines stdout and stderr without exposing tool metadata labels.
// commandOutputText 合并 stdout 与 stderr，同时不暴露工具 metadata 标签。
function commandOutputText(presentation: CommandExecutionPresentation): string {
  return [presentation.stdout, presentation.stderr]
    .map((value) => value.trim())
    .filter(Boolean)
    .join("\n");
}

// commandExitCodeState maps exit codes to visual status classes.
// commandExitCodeState 将退出码映射为视觉状态类名。
function commandExitCodeState(exitCode: number | null): "unknown" | "success" | "failed" {
  if (exitCode === null) {
    return "unknown";
  }
  return exitCode === 0 ? "success" : "failed";
}

// commandPresentationDetailCacheKey creates a stable cache key for hidden command details.
// commandPresentationDetailCacheKey 为隐藏命令详情创建稳定缓存键。
function commandPresentationDetailCacheKey(presentation: CommandExecutionPresentation): string | null {
  const detailRef = presentation.detailRef;
  if (!detailRef) {
    return null;
  }
  return [
    detailRef.sessionId,
    detailRef.recordId ?? "",
    detailRef.toolCallId ?? "",
    detailRef.seq ?? "",
    detailRef.messageIndex ?? "",
  ].join(":");
}
