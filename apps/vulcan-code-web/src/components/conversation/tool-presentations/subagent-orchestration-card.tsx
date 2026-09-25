// Subagent orchestration tool card for spawning, waiting on and messaging agents.
// 子代理编排工具卡，覆盖创建、等待与消息往来。
import * as React from "react";
import { ConversationCollaborationStatus } from "../conversation-collaboration-card";
import { ConversationCollaborationStateContext } from "../conversation-collaboration-state";
import { Bot, ChevronDown, ChevronRight } from "lucide-react";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { hydrateSubagentOrchestrationPresentation, type SubagentOrchestrationAgent, type SubagentOrchestrationPresentation } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { type ToolRequestCardProgress, type ToolDetailLoader, activeToolStatusLabel } from "./tool-card-common";

// subagentOrchestrationPresentationDetailCache avoids refetching hidden subagent status details.
// subagentOrchestrationPresentationDetailCache 避免重复请求隐藏子代理状态详情。
const subagentOrchestrationPresentationDetailCache = new Map<string, SubagentOrchestrationPresentation>();

// SubagentOrchestrationPresentationCard renders a compact inline subagent dispatch record.
// SubagentOrchestrationPresentationCard 渲染紧凑的内联子代理调度记录。
export function SubagentOrchestrationPresentationCard(props: {
  // Subagent orchestration presentation model.
  // 子代理调度展示模型。
  presentation: SubagentOrchestrationPresentation;
  // Whether this row is expanded.
  // 当前行是否展开。
  expanded: boolean;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for full subagent results.
  // 加载单条隐藏工具详情记录用于完整子代理结果。
  onLoadToolDetail?: ToolDetailLoader;
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
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || subagentOrchestrationPresentationDetailCache.has(detailCacheKey)) {
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
          // A failed read must not populate the successful-detail cache or prevent explicit retries.
          // 失败读取不能填充成功详情缓存，也不能阻止显式重读。
          setHydratedPresentation(presentation);
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
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );
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
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command"><span className="tool-presentation-command-text">{subagentCardSummaryText(hydratedPresentation, t)}</span></span>
        <ConversationCollaborationStatus questionId={hydratedPresentation.questionId} executionId={hydratedPresentation.executionId} />
        {canExpand ? (
          expanded ? (
            <ChevronDown className="tool-presentation-toggle" size={15} />
          ) : (
            <ChevronRight className="tool-presentation-toggle" size={15} />
          )
        ) : null}
      </button>
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
  if (presentation.operation.startsWith("collaboration_")) {
    return <CollaborationToolDetail presentation={presentation} t={t} />;
  }
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
          <span>{t(presentation.deliveryMode === "interrupt" ? "conversation.subagent.detail.steering" : "conversation.subagent.detail.queued")}</span>
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

// CollaborationToolDetail keeps question, answer, direction and waiting cards distinct.
// CollaborationToolDetail 保持问题、回答、方向与等待卡片彼此区分。
function CollaborationToolDetail(props: {
  // Exact collaboration presentation model.
  // 精确的协作展示模型。
  presentation: SubagentOrchestrationPresentation;
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string;
}) {
  const { presentation, t } = props;
  // Question tools expose conversation content, never the internal request/receipt JSON.
  // 问题工具仅展示对话内容，不展示内部请求或回执 JSON。
  const isQuestion = presentation.operation === "collaboration_question"
    || presentation.operation === "collaboration_answer" || presentation.operation === "collaboration_question_wait"
    || presentation.operation === "collaboration_cancel";
  const collaborationState = React.useContext(ConversationCollaborationStateContext);
  const question = presentation.questionId ? collaborationState.questions.get(presentation.questionId) : undefined;
  const answer = question?.question.answer;
  // A newer failed delivery revokes historical answer disclosure as well.
  // 较新的失败交付也撤销历史回答的展示。
  const answerText = answer ? (answer.delivery.state === "failed" ? null : answer.body) : presentation.answerText;
  const argumentText = presentation.collaborationArguments
    ? JSON.stringify(presentation.collaborationArguments, null, 2)
    : null;
  const promptLabel = presentation.operation === "collaboration_answer"
    ? t("conversation.collaboration.detail.answer")
    : presentation.operation === "collaboration_notification"
      ? t("conversation.collaboration.detail.notification")
      : presentation.operation === "collaboration_steer"
      ? t("conversation.subagent.detail.message")
      : t("conversation.collaboration.detail.question");
  return (
    <div className="subagent-orchestration-detail collaboration-tool-detail">
      {presentation.prompt ? <SubagentInlineTextLine label={promptLabel} text={presentation.prompt} /> : null}
      {presentation.deliveryMode ? (
        <div className="subagent-orchestration-meta">
          <span>{t("conversation.subagent.detail.deliveryMode")}</span>
          <strong>{presentation.deliveryMode === "queue" ? t("conversation.subagent.queue.followup") : t("conversation.subagent.queue.steering")}</strong>
        </div>
      ) : null}
      {!isQuestion && argumentText ? <SubagentTextBlock label={t("conversation.collaboration.detail.arguments")} text={argumentText} /> : null}
      {!isQuestion && presentation.result ? <SubagentTextBlock label={t("conversation.collaboration.detail.output")} text={presentation.result} /> : null}
      {isQuestion && presentation.operation !== "collaboration_answer" && answerText ? <SubagentTextBlock label={t("conversation.collaboration.detail.answer")} text={answerText} /> : null}
      {!presentation.prompt && (isQuestion ? !answerText : !argumentText && !presentation.result) ? (
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
      {props.agent.result ? (
        <pre className="subagent-orchestration-text">{compactSubagentText(props.agent.result)}</pre>
      ) : (
        <div className="subagent-orchestration-empty">{props.t("conversation.subagent.detail.noResult")}</div>
      )}
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
  if (presentation.operation === "collaboration_question") return t("conversation.collaboration.question");
  if (presentation.operation === "collaboration_answer") return t("conversation.collaboration.answer");
  if (presentation.operation === "collaboration_steer") return t("conversation.collaboration.steer");
  if (presentation.operation === "collaboration_question_wait") return t("conversation.collaboration.questionWait");
  if (presentation.operation === "collaboration_notification") return t("conversation.collaboration.notification");
  if (presentation.operation === "collaboration_session_list") return t("conversation.collaboration.sessionList");
  if (presentation.operation === "collaboration_session_read") return t("conversation.collaboration.sessionRead");
  if (presentation.operation === "collaboration_session_wait") return t("conversation.collaboration.sessionWait");
  if (presentation.operation === "collaboration_cancel") return t("conversation.collaboration.cancel");
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
  if (presentation.operation === "collaboration_steer") {
    return presentation.deliveryMode === "queue"
      ? t("conversation.subagent.queue.followup")
      : t("conversation.subagent.queue.steering");
  }
  if (presentation.operation.startsWith("collaboration_")) {
    return subagentOperationLabel(presentation, t);
  }
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

// subagentCardSummaryText keeps the orchestration operation visible beside its specific target or outcome.
// subagentCardSummaryText 在具体目标或结果旁始终保留调度操作名称。
function subagentCardSummaryText(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  // operationLabel is the stable user-facing action that must never be replaced by an agent name.
  // operationLabel 是稳定的用户可见操作，不得被智能体名称替代。
  const operationLabel = subagentOperationLabel(presentation, t);
  // detailText retains the concrete agent, count, or wait result for this invocation.
  // detailText 保留本次调用的具体智能体、数量或等待结果。
  const detailText = subagentSummaryText(presentation, t);
  return detailText === operationLabel ? operationLabel : `${operationLabel} · ${detailText}`;
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
  if (presentation.waitNeedsReply) {
    return t("conversation.subagent.detail.waitNeedsReply");
  }
  if (presentation.waitCheckFailed) {
    return t("conversation.subagent.detail.waitCheckFailed");
  }
  if (presentation.noActiveAgents) {
    return t("conversation.subagent.detail.noPendingWork");
  }
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
