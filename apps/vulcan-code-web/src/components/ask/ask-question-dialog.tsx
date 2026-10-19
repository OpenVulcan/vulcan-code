import React from "react";
import {
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  CornerDownLeft,
  Info,
} from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Textarea } from "@/ui-kit/primitives/textarea";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import type {
  PendingQuestionOptionSummary,
  PendingQuestionPromptSummary,
  PendingQuestionSnapshot,
} from "@/types";
import { ToolApprovalRequest } from "./tool-approval-bar";
import { useAskAnchorStyle } from "./use-ask-anchor-style";

// AskQuestionStage describes the lightweight question bar lifecycle.
// AskQuestionStage 描述轻量 question bar 的生命周期阶段。
type AskQuestionStage = "answering" | "submitConfirm" | "rejectConfirm";

// AskQuestionDialogProps describes callbacks and request data for the ask question bar.
// AskQuestionDialogProps 描述 ask question bar 使用的请求数据与回调。
interface AskQuestionDialogProps {
  // Pending request shown by the question bar.
  // question bar 展示的待处理请求。
  request: PendingQuestionSnapshot | null;
  // Called when the user confirms abandoning the request.
  // 用户确认放弃请求时调用。
  onReject: (requestId: string) => Promise<void>;
  // Called when the user submits all validated answers.
  // 用户提交全部校验后的答案时调用。
  onSubmit: (requestId: string, answers: string[][]) => Promise<void>;
}

// AskQuestionDraft stores one editable answer draft.
// AskQuestionDraft 保存单个可编辑答案草稿。
interface AskQuestionDraft {
  // Selected structured option labels.
  // 已选择的结构化选项标签。
  selectedLabels: string[];
  // Whether the custom answer lane is enabled.
  // 是否启用自定义回答通道。
  customSelected: boolean;
  // Custom free-text answer.
  // 自定义自由文本回答。
  customText: string;
}

// AskQuestionValidationResult stores collected answers or the first validation error.
// AskQuestionValidationResult 保存收集到的答案或首个校验错误。
interface AskQuestionValidationResult {
  // Answers aligned with the request question order.
  // 与请求问题顺序对齐的答案。
  answers: string[][];
  // First validation error, when present.
  // 存在时表示首个校验错误。
  error: string | null;
  // Question index that owns the first validation error.
  // 首个校验错误所属的问题下标。
  errorIndex: number | null;
}

// AskQuestionOptionDescriptor describes one rendered option row, including the synthetic custom row.
// AskQuestionOptionDescriptor 描述一条渲染选项行，包括合成的自定义回答行。
interface AskQuestionOptionDescriptor {
  // Stable row label shown in the question bar.
  // question bar 中显示的稳定行标签。
  label: string;
  // Optional helper text shown in the information tooltip.
  // 信息 tooltip 中显示的可选辅助说明。
  description: string;
  // Whether this row represents the synthetic custom-answer lane.
  // 该行是否表示合成的自定义回答通道。
  custom: boolean;
  // Original structured option when the row maps to one.
  // 该行映射到结构化选项时的原始选项。
  option: PendingQuestionOptionSummary | null;
}

// AskQuestionDialog renders a blocking ask request or tool-call approval as a compact session bar.
// AskQuestionDialog 将阻塞式 ask 请求或工具调用审批渲染为紧凑会话面板。
export function AskQuestionDialog(props: AskQuestionDialogProps) {
  if (!props.request) {
    return null;
  }
  return props.request.approval
    ? <ToolApprovalRequest key={props.request.id} {...props} approval={props.request.approval} request={props.request} />
    : <AskQuestionRequest key={props.request.id} {...props} />;
}

// AskQuestionRequest isolates drafts and in-flight acknowledgements to one immutable request identity.
// AskQuestionRequest 将草稿与进行中的确认隔离到一个不可变请求标识。
// props supplies the current request and acknowledgement callbacks; returns the scoped question surface.
// props 提供当前请求及确认回调；返回具有独立状态的问题面板。
function AskQuestionRequest(props: AskQuestionDialogProps) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // request is the latest pending ask request projected by the service layer.
  // request 是服务层投影出的最新待处理 ask 请求。
  const request = props.request;
  // activeIndex stores the currently visible question index.
  // activeIndex 保存当前可见的问题下标。
  const [activeIndex, setActiveIndex] = React.useState(0);
  // activeOptionIndex stores the keyboard-highlighted rendered option row.
  // activeOptionIndex 保存键盘当前高亮的渲染选项行。
  const [activeOptionIndex, setActiveOptionIndex] = React.useState(0);
  // stage stores whether the user is answering, confirming submit, or confirming rejection.
  // stage 保存用户当前处于答题、提交确认还是终止确认阶段。
  const [stage, setStage] = React.useState<AskQuestionStage>("answering");
  // drafts stores all editable answers for the active request.
  // drafts 保存当前请求的全部可编辑答案。
  const [drafts, setDrafts] = React.useState<AskQuestionDraft[]>([]);
  // validationError stores the latest user-facing validation message.
  // validationError 保存最近一次面向用户的校验消息。
  const [validationError, setValidationError] = React.useState<string | null>(null);
  // busy prevents double submit or double reject while the backend call is pending.
  // busy 在后端调用未完成时阻止重复提交或重复拒绝。
  const [busy, setBusy] = React.useState(false);
  // mutationPending blocks repeated events before React commits the disabled state.
  // mutationPending 在 React 提交禁用状态前阻止重复事件。
  const mutationPending = React.useRef(false);
  // mounted prevents a retired request from publishing late errors into its replacement.
  // mounted 阻止已离开的请求把迟到错误发布到替代请求中。
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  // questionBarRef owns focus when the active question uses structured choices.
  // questionBarRef 在当前问题使用结构化选项时承载键盘焦点。
  const questionBarRef = React.useRef<HTMLElement | null>(null);
  // titleId links the custom dialog title to assistive technology.
  // titleId 将自定义对话框标题关联给辅助技术。
  const titleId = React.useId();
  // descriptionId links the custom dialog description to assistive technology.
  // descriptionId 将自定义对话框说明关联给辅助技术。
  const descriptionId = React.useId();
  // overlayStyle keeps the Ask bar horizontally aligned with the active conversation panel.
  // overlayStyle 保持 Ask 面板与当前会话面板横向对齐。
  const overlayStyle = useAskAnchorStyle(request?.id ?? null);

  React.useEffect(() => {
    setActiveIndex(0);
    setActiveOptionIndex(0);
    setStage("answering");
    setDrafts(request ? buildAskQuestionDrafts(request.questions) : []);
    setValidationError(null);
    setBusy(false);
  }, [request?.id]);

  // questionCount is the total prompt count inside the pending request.
  // questionCount 是待处理请求中的问题总数。
  const questionCount = request?.questions.length ?? 0;
  // safeActiveIndex clamps state against a request shape update.
  // safeActiveIndex 将状态限制在最新请求结构范围内。
  const safeActiveIndex = Math.min(activeIndex, Math.max(questionCount - 1, 0));
  // activeQuestion is the prompt currently shown to the user.
  // activeQuestion 是当前展示给用户的问题。
  const activeQuestion = request?.questions[safeActiveIndex] ?? null;
  // activeDraft is the answer draft paired with the visible question.
  // activeDraft 是与当前问题对应的答案草稿。
  const activeDraft = drafts[safeActiveIndex] ?? null;
  // renderedOptions describes visible option rows including the custom lane.
  // renderedOptions 描述包含自定义通道在内的可见选项行。
  const renderedOptions = React.useMemo(
    () => activeQuestion ? buildAskQuestionOptionDescriptors(activeQuestion, t) : [],
    [activeQuestion, t],
  );
  // focusStructuredQuestion depends on semantic mode, not freshly projected prompt object identity.
  // focusStructuredQuestion 依赖语义模式，不依赖重新投影的问题对象引用。
  const focusStructuredQuestion = Boolean(activeQuestion && activeQuestion.options.length > 0 && !activeDraft?.customSelected);

  React.useEffect(() => {
    if (!activeQuestion) {
      setActiveOptionIndex(0);
      return;
    }
    const nextDraft = drafts[safeActiveIndex] ?? buildAskQuestionDraft(activeQuestion);
    setActiveOptionIndex(defaultAskQuestionOptionIndex(activeQuestion, nextDraft));
  }, [activeQuestion, safeActiveIndex]);

  React.useEffect(() => {
    if (!request || !focusStructuredQuestion) {
      return;
    }
    const focusFrame = window.requestAnimationFrame(() => {
      questionBarRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(focusFrame);
  }, [focusStructuredQuestion, safeActiveIndex, request?.id]);

  // updateDraft applies one immutable change to a draft row.
  // updateDraft 对单个草稿行应用一次不可变更新。
  const updateDraft = React.useCallback((questionIndex: number, nextDraft: AskQuestionDraft) => {
    if (mutationPending.current) return;
    setDrafts((current) => current.map((draft, index) => index === questionIndex ? nextDraft : draft));
    setValidationError(null);
  }, []);

  // moveQuestion changes the visible prompt and returns from lightweight confirmation stages.
  // moveQuestion 切换可见问题，并从轻量确认阶段返回答题阶段。
  const moveQuestion = React.useCallback((direction: -1 | 1) => {
    if (mutationPending.current || !request || questionCount === 0) {
      return;
    }
    setStage("answering");
    setValidationError(null);
    setActiveIndex((current) => Math.min(Math.max(current + direction, 0), questionCount - 1));
  }, [questionCount, request]);

  // toggleRenderedOption toggles one structured option row or the custom-answer lane.
  // toggleRenderedOption 切换一条结构化选项行或自定义回答通道。
  const toggleRenderedOption = React.useCallback((optionIndex: number) => {
    if (mutationPending.current || !activeQuestion || !activeDraft) {
      return;
    }
    const descriptor = renderedOptions[optionIndex];
    if (!descriptor) {
      return;
    }
    setStage("answering");
    setActiveOptionIndex(optionIndex);
    if (descriptor.custom) {
      const nextDraft = activeQuestion.options.length === 0
        ? { ...activeDraft, customSelected: true }
        : activeQuestion.multiple
          ? { ...activeDraft, customSelected: !activeDraft.customSelected }
          : { ...activeDraft, customSelected: true, selectedLabels: [] };
      updateDraft(safeActiveIndex, nextDraft);
      return;
    }
    if (!descriptor.option) {
      return;
    }
    if (activeQuestion.multiple) {
      const selectedLabels = activeDraft.selectedLabels.includes(descriptor.option.label)
        ? activeDraft.selectedLabels.filter((label) => label !== descriptor.option?.label)
        : [...activeDraft.selectedLabels, descriptor.option.label];
      updateDraft(safeActiveIndex, { ...activeDraft, selectedLabels });
      return;
    }
    updateDraft(safeActiveIndex, {
      ...activeDraft,
      customSelected: false,
      selectedLabels: [descriptor.option.label],
    });
  }, [activeDraft, activeQuestion, renderedOptions, safeActiveIndex, updateDraft]);

  // moveOption changes the keyboard-highlighted option row without wrapping around.
  // moveOption 以不循环方式移动键盘高亮选项行。
  const moveOption = React.useCallback((direction: -1 | 1) => {
    if (mutationPending.current) return;
    setStage("answering");
    setValidationError(null);
    setActiveOptionIndex((current) => Math.min(Math.max(current + direction, 0), Math.max(renderedOptions.length - 1, 0)));
  }, [renderedOptions.length]);

  // openSubmitConfirmation validates current and previous answers before showing the final confirmation prompt.
  // openSubmitConfirmation 在显示最终确认提示前校验当前与前序答案。
  const openSubmitConfirmation = React.useCallback(() => {
    if (!request) {
      return;
    }
    const nextValidation = collectAskQuestionAnswers(request.questions, drafts, t);
    if (nextValidation.error) {
      setActiveIndex(nextValidation.errorIndex ?? 0);
      setStage("answering");
      setValidationError(nextValidation.error);
      return;
    }
    setStage("submitConfirm");
    setValidationError(null);
  }, [drafts, request, t]);

  // advanceQuestion validates the current page before moving forward or opening submit confirmation.
  // advanceQuestion 校验当前页面，然后前进或打开提交确认。
  const advanceQuestion = React.useCallback(() => {
    if (!activeQuestion || !activeDraft || !request) {
      return;
    }
    const currentResult = collectAskQuestionPromptAnswers(activeQuestion, activeDraft, t);
    if (currentResult.error) {
      setValidationError(currentResult.error);
      setStage("answering");
      return;
    }
    if (safeActiveIndex >= questionCount - 1) {
      openSubmitConfirmation();
      return;
    }
    setValidationError(null);
    setStage("answering");
    setActiveIndex((current) => Math.min(current + 1, questionCount - 1));
  }, [activeDraft, activeQuestion, openSubmitConfirmation, questionCount, request, safeActiveIndex, t]);

  // handleSubmit validates answers, then resumes the backend ask runtime.
  // handleSubmit 校验答案并恢复后端 ask 运行时。
  const handleSubmit = React.useCallback(async () => {
    if (mutationPending.current || !request) {
      return;
    }
    const nextValidation = collectAskQuestionAnswers(request.questions, drafts, t);
    if (nextValidation.error) {
      setValidationError(nextValidation.error);
      setStage("answering");
      setActiveIndex(nextValidation.errorIndex ?? 0);
      return;
    }
    mutationPending.current = true;
    setBusy(true);
    setValidationError(null);
    try {
      await props.onSubmit(request.id, nextValidation.answers);
      // Successful acknowledgements stay locked until the authoritative request leaves the surface.
      // 成功确认后保持锁定，直到权威请求从面板移除。
    } catch (error) {
      if (!mounted.current) return;
      mutationPending.current = false;
      setBusy(false);
      setValidationError(error instanceof Error ? error.message : String(error));
    }
  }, [drafts, props, request, t]);

  // handleReject confirms cancellation and rejects the backend ask runtime.
  // handleReject 确认取消并拒绝后端 ask 运行时。
  const handleReject = React.useCallback(async () => {
    if (mutationPending.current || !request) {
      return;
    }
    mutationPending.current = true;
    setBusy(true);
    setValidationError(null);
    try {
      await props.onReject(request.id);
    } catch (error) {
      if (!mounted.current) return;
      mutationPending.current = false;
      setBusy(false);
      setValidationError(error instanceof Error ? error.message : String(error));
    }
  }, [props, request]);

  // handleDialogKeyDown maps keyboard shortcuts to the compact question bar state machine.
  // handleDialogKeyDown 将键盘快捷键映射到紧凑 question bar 状态机。
  const handleDialogKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (mutationPending.current || busy || event.nativeEvent.isComposing) {
      return;
    }
    const targetIsTextarea = event.target instanceof HTMLTextAreaElement;
    if (event.key === "Escape") {
      event.preventDefault();
      if (stage === "answering") {
        setStage("rejectConfirm");
      } else {
        setStage("answering");
      }
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (stage === "submitConfirm") {
        void handleSubmit();
      } else if (stage === "rejectConfirm") {
        void handleReject();
      } else {
        advanceQuestion();
      }
      return;
    }
    if (stage !== "answering" || targetIsTextarea) {
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      moveQuestion(-1);
      return;
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      moveQuestion(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      moveOption(-1);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveOption(1);
      return;
    }
    if (event.key === " ") {
      event.preventDefault();
      toggleRenderedOption(activeOptionIndex);
    }
  }, [activeOptionIndex, advanceQuestion, busy, handleReject, handleSubmit, moveOption, moveQuestion, stage, toggleRenderedOption]);

  if (!request || questionCount === 0 || !activeQuestion || !activeDraft) {
    return null;
  }

  // isLastQuestion indicates whether the primary answering action should read as submit.
  // isLastQuestion 表示答题态主按钮是否应显示为提交。
  const isLastQuestion = safeActiveIndex >= questionCount - 1;
  // primaryLabel is the current primary action label.
  // primaryLabel 是当前主操作按钮文案。
  const primaryLabel = stage === "submitConfirm"
    ? t("ask.action.confirmSubmit")
    : stage === "rejectConfirm"
      ? t("ask.action.confirmReject")
      : isLastQuestion
        ? t("ask.action.submit")
        : t("ask.action.continue");
  // primaryAction is the current primary action callback.
  // primaryAction 是当前主操作回调。
  const primaryAction = stage === "submitConfirm"
    ? handleSubmit
    : stage === "rejectConfirm"
      ? handleReject
      : advanceQuestion;

  return (
    <TooltipProvider>
      <div className="vk-ask-layer" style={overlayStyle}>
        <section
          aria-describedby={descriptionId}
          aria-labelledby={titleId}
          aria-modal="false"
          className="vk-ask"
          data-kind="question"
          onKeyDown={handleDialogKeyDown}
          ref={questionBarRef}
          role="dialog"
          tabIndex={-1}
        >
          <h2 className="vk-sr-only" id={titleId}>{t("ask.title")}</h2>
          <p className="vk-sr-only" id={descriptionId}>
            {t("ask.description")}
          </p>
          <div className="vk-ask-head">
            <div className="vk-ask-title">
              {activeQuestion.question}
            </div>
            <AskQuestionProgress
              activeIndex={safeActiveIndex}
              disabled={busy}
              onNext={() => moveQuestion(1)}
              onPrevious={() => moveQuestion(-1)}
              t={t}
              total={questionCount}
            />
          </div>
          <div className="vk-ask-options">
            {renderedOptions.map((descriptor, optionIndex) => (
              <AskQuestionOptionRow
                active={stage === "answering" && optionIndex === activeOptionIndex}
                descriptor={descriptor}
                disabled={busy || stage !== "answering"}
                draft={activeDraft}
                key={`${descriptor.custom ? "custom" : descriptor.label}-${optionIndex}`}
                onCustomTextChange={(value) => updateDraft(safeActiveIndex, {
                  ...activeDraft,
                  customSelected: true,
                  customText: value,
                  selectedLabels: activeQuestion.options.length === 0 || activeQuestion.multiple
                    ? activeDraft.selectedLabels
                    : [],
                })}
                onSelect={() => toggleRenderedOption(optionIndex)}
                onFocus={() => setActiveOptionIndex(optionIndex)}
                optionIndex={optionIndex}
                question={activeQuestion}
                selected={askQuestionOptionIsSelected(activeQuestion, activeDraft, descriptor)}
                t={t}
              />
            ))}
          </div>
          {validationError ? (
            <div role="alert" className="vk-ask-error">
              {validationError}
            </div>
          ) : null}
          <AskQuestionConfirmPanel stage={stage} t={t} />
          <div className="vk-ask-actions">
            <Button
              disabled={busy}
              onClick={() => {
                if (mutationPending.current) return;
                if (stage === "answering") {
                  setStage("rejectConfirm");
                } else {
                  setStage("answering");
                }
              }}
              size="sm"
              type="button"
              variant="ghost"
            >
              <span>{stage === "answering" ? t("ask.action.ignore") : t("ask.action.backToEdit")}</span>
              <kbd className="vk-kbd">ESC</kbd>
            </Button>
            <Button
              disabled={busy}
              onClick={() => void primaryAction()}
              size="sm"
              type="button"
            >
              <span>{busy ? t("ask.action.processing") : primaryLabel}</span>
              <CornerDownLeft size={14} />
            </Button>
          </div>
        </section>
      </div>
    </TooltipProvider>
  );
}

// AskQuestionProgress renders the clickable top-right question position control.
// AskQuestionProgress 渲染右上角可点击的问题位置控件。
function AskQuestionProgress(props: {
  // Current zero-based question index.
  // 当前从零开始的问题下标。
  activeIndex: number;
  // Whether navigation is disabled.
  // 是否禁用导航。
  disabled: boolean;
  // Moves to the next question.
  // 移动到下一题。
  onNext: () => void;
  // Moves to the previous question.
  // 移动到上一题。
  onPrevious: () => void;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
  // Total question count.
  // 问题总数。
  total: number;
}) {
  return (
    <div className="vk-ask-progress">
      <Button
        aria-label={props.t("ask.progress.previous")}
        disabled={props.disabled || props.activeIndex <= 0}
        onClick={props.onPrevious}
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <ChevronLeft size={12} />
      </Button>
      <span>{props.activeIndex + 1} / {props.total}</span>
      <Button
        aria-label={props.t("ask.progress.next")}
        disabled={props.disabled || props.activeIndex >= props.total - 1}
        onClick={props.onNext}
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <ChevronRight size={12} />
      </Button>
    </div>
  );
}

// AskQuestionOptionRow renders one compact numbered option or inline custom-answer editor.
// AskQuestionOptionRow 渲染一条紧凑编号选项或行内自定义回答编辑器。
function AskQuestionOptionRow(props: {
  // Whether this row is keyboard-highlighted.
  // 该行是否被键盘高亮。
  active: boolean;
  // Rendered option descriptor.
  // 渲染选项描述。
  descriptor: AskQuestionOptionDescriptor;
  // Whether row interactions are disabled.
  // 是否禁用行交互。
  disabled: boolean;
  // Draft that owns current selections and custom text.
  // 持有当前选择与自定义文本的草稿。
  draft: AskQuestionDraft;
  // Emits custom text changes.
  // 发出自定义文本变化。
  onCustomTextChange: (value: string) => void;
  // Selects or toggles this row.
  // 选择或切换该行。
  onSelect: () => void;
  // Synchronizes the option highlighted by keyboard navigation with actual focus.
  // 将键盘导航高亮项与实际焦点保持同步。
  onFocus: () => void;
  // Zero-based rendered row index.
  // 从零开始的渲染行下标。
  optionIndex: number;
  // Question that owns this option row.
  // 拥有该选项行的问题。
  question: PendingQuestionPromptSummary;
  // Whether this row is selected.
  // 该行是否已选中。
  selected: boolean;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  // showsInlineEditor indicates whether this row should render an embedded textarea.
  // showsInlineEditor 表示该行是否应渲染内嵌多行输入框。
  const showsInlineEditor = props.descriptor.custom && props.selected;
  // activeMark exposes the keyboard highlight to the kit row style.
  // activeMark 将键盘高亮状态交给 kit 行样式。
  const activeMark = props.active ? "true" : undefined;

  if (showsInlineEditor) {
    return (
      <div className="vk-ask-option" data-active={activeMark} data-editor="true">
        <span className="vk-ask-index">{props.optionIndex + 1}.</span>
        <Textarea
          aria-label={props.question.question}
          autoFocus
          className="vk-ask-editor"
          disabled={props.disabled}
          onChange={(event) => props.onCustomTextChange(event.target.value)}
          placeholder={props.descriptor.label}
          rows={askQuestionTextareaRows(props.draft.customText)}
          value={props.draft.customText}
        />
        <AskQuestionRowTail active={props.active} selected={props.selected} />
      </div>
    );
  }

  return (
    <button
      aria-pressed={props.selected}
      className="vk-ask-option"
      data-active={activeMark}
      disabled={props.disabled}
      onClick={props.onSelect}
      onFocus={props.onFocus}
      type="button"
    >
      <span className="vk-ask-index">{props.optionIndex + 1}.</span>
      <span className="vk-ask-label-row">
        <span className="vk-ask-label">{props.descriptor.label}</span>
        {props.descriptor.description ? <span className="vk-sr-only">{props.descriptor.description}</span> : null}
        <AskQuestionInfoTooltip descriptor={props.descriptor} />
        {props.selected ? (
          <span className="vk-badge">
            {props.t("ask.option.selected")}
          </span>
        ) : null}
      </span>
      <AskQuestionRowTail active={props.active} selected={props.selected} />
    </button>
  );
}

// AskQuestionInfoTooltip renders a bounded tooltip containing full option text.
// AskQuestionInfoTooltip 渲染包含完整选项文本的有界 tooltip。
function AskQuestionInfoTooltip(props: {
  // Rendered option descriptor.
  // 渲染选项描述。
  descriptor: AskQuestionOptionDescriptor;
}) {
  // tooltipDescription is the normalized helper text shown below the label.
  // tooltipDescription 是显示在标签下方的规范化辅助文本。
  const tooltipDescription = props.descriptor.description.trim();
  if (!props.descriptor.label.trim() && !tooltipDescription) {
    return null;
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="vk-ask-info">
          <Info size={10} />
        </span>
      </TooltipTrigger>
      <TooltipContent className="vk-ask-tip" side="top" sideOffset={6}>
        <span>{props.descriptor.label}</span>
        {tooltipDescription ? <span>{tooltipDescription}</span> : null}
      </TooltipContent>
    </Tooltip>
  );
}

// AskQuestionRowTail renders selected badges or keyboard movement hints on the row edge.
// AskQuestionRowTail 渲染行尾的选中标记或键盘移动提示。
function AskQuestionRowTail(props: {
  // Whether the row is keyboard-highlighted.
  // 该行是否被键盘高亮。
  active: boolean;
  // Whether the row is selected.
  // 该行是否已选中。
  selected: boolean;
}) {
  if (!props.active) {
    return <span className="vk-ask-tail" />;
  }
  return (
    <span className="vk-ask-tail">
      {props.selected ? <CheckCircle2 data-selected="true" size={14} /> : null}
      <ArrowUp size={12} />
      <ArrowDown size={12} />
    </span>
  );
}

// AskQuestionConfirmPanel renders the inline submit or reject confirmation prompt.
// AskQuestionConfirmPanel 渲染行内提交或终止确认提示。
function AskQuestionConfirmPanel(props: {
  // Current question bar lifecycle stage.
  // 当前 question bar 生命周期阶段。
  stage: AskQuestionStage;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  if (props.stage === "answering") {
    return null;
  }
  const confirmTitle = props.stage === "submitConfirm" ? props.t("ask.confirm.submitTitle") : props.t("ask.confirm.rejectTitle");
  const confirmBody = props.stage === "submitConfirm"
    ? props.t("ask.confirm.submitBody")
    : props.t("ask.confirm.rejectBody");
  return (
    <div className="vk-ask-confirm" data-tone={props.stage === "rejectConfirm" ? "bad" : undefined}>
      <CircleAlert size={16} />
      <div>
        <strong>{confirmTitle}</strong>
        <small>{confirmBody}</small>
      </div>
    </div>
  );
}

// buildAskQuestionDrafts creates fresh drafts for a pending ask request.
// buildAskQuestionDrafts 为待处理 ask 请求创建全新草稿。
function buildAskQuestionDrafts(questions: PendingQuestionPromptSummary[]): AskQuestionDraft[] {
  return questions.map(buildAskQuestionDraft);
}

// buildAskQuestionDraft creates one default draft for a prompt.
// buildAskQuestionDraft 为单条问题提示创建默认草稿。
function buildAskQuestionDraft(question: PendingQuestionPromptSummary): AskQuestionDraft {
  return {
    selectedLabels: [],
    customSelected: question.options.length === 0,
    customText: "",
  };
}

// buildAskQuestionOptionDescriptors creates visible rows for structured choices plus the custom lane.
// buildAskQuestionOptionDescriptors 为结构化选项和自定义通道创建可见行。
function buildAskQuestionOptionDescriptors(
  question: PendingQuestionPromptSummary,
  t: (key: TranslationKey) => string,
): AskQuestionOptionDescriptor[] {
  const optionRows = question.options.map((option) => ({
    label: option.label,
    description: option.description,
    custom: false,
    option,
  }));
  return [
    ...optionRows,
    {
      label: question.options.length === 0 ? t("ask.custom.textPlaceholder") : t("ask.custom.adjustCodex"),
      description: t("ask.custom.description"),
      custom: true,
      option: null,
    },
  ];
}

// defaultAskQuestionOptionIndex returns the best keyboard highlight for one draft.
// defaultAskQuestionOptionIndex 返回单个草稿最合适的键盘高亮行。
function defaultAskQuestionOptionIndex(
  question: PendingQuestionPromptSummary,
  draft: AskQuestionDraft,
): number {
  if (question.options.length === 0 || draft.customSelected) {
    return question.options.length;
  }
  const selectedIndex = question.options.findIndex((option) => draft.selectedLabels.includes(option.label));
  return selectedIndex >= 0 ? selectedIndex : 0;
}

// askQuestionOptionIsSelected returns whether one rendered row is selected.
// askQuestionOptionIsSelected 返回某条渲染行是否已选中。
function askQuestionOptionIsSelected(
  question: PendingQuestionPromptSummary,
  draft: AskQuestionDraft,
  descriptor: AskQuestionOptionDescriptor,
): boolean {
  if (descriptor.custom) {
    return question.options.length === 0 || draft.customSelected;
  }
  return descriptor.option ? draft.selectedLabels.includes(descriptor.option.label) : false;
}

// askQuestionTextareaRows estimates inline textarea height from current content.
// askQuestionTextareaRows 根据当前内容估算行内输入框高度。
function askQuestionTextareaRows(value: string): number {
  const manualLines = Math.max(value.split(/\r?\n/).length, 1);
  const wrappedLines = Math.floor(value.length / 72) + 1;
  return Math.min(Math.max(manualLines, wrappedLines, 1), 8);
}

// collectAskQuestionAnswers validates every prompt and returns ordered answers.
// collectAskQuestionAnswers 校验全部问题并返回有序答案。
function collectAskQuestionAnswers(
  questions: PendingQuestionPromptSummary[],
  drafts: AskQuestionDraft[],
  t: (key: TranslationKey) => string,
): AskQuestionValidationResult {
  const answers: string[][] = [];
  for (let index = 0; index < questions.length; index += 1) {
    const question = questions[index];
    const draft = drafts[index] ?? buildAskQuestionDraft(question);
    const result = collectAskQuestionPromptAnswers(question, draft, t);
    if (result.error) {
      return { answers, error: result.error, errorIndex: index };
    }
    answers.push(result.answers);
  }
  return { answers, error: null, errorIndex: null };
}

// collectAskQuestionPromptAnswers validates one prompt draft.
// collectAskQuestionPromptAnswers 校验单条问题草稿。
function collectAskQuestionPromptAnswers(
  question: PendingQuestionPromptSummary,
  draft: AskQuestionDraft,
  t: (key: TranslationKey) => string,
): { answers: string[]; error: string | null } {
  const trimmedCustom = draft.customText.trim();
  if (question.options.length === 0) {
    return trimmedCustom
      ? { answers: [trimmedCustom], error: null }
      : { answers: [], error: formatAskQuestionMessage(t, "ask.validation.requiredAnswer", question.header) };
  }
  const answers = [...draft.selectedLabels];
  if (draft.customSelected) {
    if (!trimmedCustom) {
      return { answers: [], error: formatAskQuestionMessage(t, "ask.validation.requiredOther", question.header) };
    }
    answers.push(trimmedCustom);
  }
  if (answers.length === 0) {
    return { answers: [], error: formatAskQuestionMessage(t, "ask.validation.requiredSelectionOrOther", question.header) };
  }
  return { answers, error: null };
}

// formatAskQuestionMessage fills the one supported ask validation placeholder.
// formatAskQuestionMessage 填充 ask 校验消息当前支持的单个占位符。
function formatAskQuestionMessage(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  header: string,
): string {
  return t(key).replace("{header}", header);
}
