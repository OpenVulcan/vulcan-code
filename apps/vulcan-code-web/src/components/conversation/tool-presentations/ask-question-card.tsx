// Ask tool card showing questions and the submitted answers.
// 提问工具卡，展示问题与已提交答案。
import * as React from "react";
import { ChevronDown, ChevronRight, MessageCircleQuestionMark } from "lucide-react";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { hydrateAskQuestionPresentation, type AskQuestionPresentation, type AskQuestionPresentationItem } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { type ToolRequestCardProgress, type ToolDetailLoader, activeToolStatusLabel } from "./tool-card-common";

// askQuestionPresentationDetailCache avoids refetching completed ask answers after ordinary remounts.
// askQuestionPresentationDetailCache 避免普通重新挂载后重复请求已完成 ask 答案。
const askQuestionPresentationDetailCache = new Map<string, AskQuestionPresentation>();

// AskQuestionPresentationCard renders compact and expanded ask/question display.
// AskQuestionPresentationCard 渲染 ask/question 的紧凑与展开展示。
export function AskQuestionPresentationCard(props: {
  // Ask/question presentation model.
  // ask/question 展示模型。
  presentation: AskQuestionPresentation;
  // Whether this ask card is expanded.
  // 当前提问卡片是否展开。
  expanded: boolean;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for completed answers.
  // 加载单条隐藏工具详情记录用于补全完成态答案。
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
  const detailCacheKey = askQuestionPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<AskQuestionPresentation>(
    () => detailCacheKey ? askQuestionPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? askQuestionPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);

  React.useEffect(() => {
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || askQuestionPresentationDetailCache.has(detailCacheKey)) {
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
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );

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
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command"><span className="tool-presentation-command-text">{hydratedPresentation.title}</span></span>
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
