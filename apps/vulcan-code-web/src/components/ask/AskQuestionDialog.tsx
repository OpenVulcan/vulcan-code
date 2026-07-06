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
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type {
  PendingQuestionOptionSummary,
  PendingQuestionPromptSummary,
  PendingQuestionSnapshot,
} from "@/types";

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

// AskQuestionDialogAnchorBounds stores viewport offsets for the active conversation panel.
// AskQuestionDialogAnchorBounds 保存当前会话面板相对视口的左右偏移。
interface AskQuestionDialogAnchorBounds {
  // Left viewport offset in CSS pixels.
  // 视口左侧偏移，单位为 CSS 像素。
  left: number;
  // Right viewport offset in CSS pixels.
  // 视口右侧偏移，单位为 CSS 像素。
  right: number;
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

// AskQuestionDialog renders the blocking WebUI ask tool as a compact question bar.
// AskQuestionDialog 将阻塞式 WebUI ask 工具渲染为紧凑 question bar。
export function AskQuestionDialog(props: AskQuestionDialogProps) {
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
  // questionBarRef owns focus when the active question uses structured choices.
  // questionBarRef 在当前问题使用结构化选项时承载键盘焦点。
  const questionBarRef = React.useRef<HTMLElement | null>(null);
  // titleId links the custom dialog title to assistive technology.
  // titleId 将自定义对话框标题关联给辅助技术。
  const titleId = React.useId();
  // descriptionId links the custom dialog description to assistive technology.
  // descriptionId 将自定义对话框说明关联给辅助技术。
  const descriptionId = React.useId();
  // anchorBounds keeps the Ask bar horizontally aligned with the active conversation panel.
  // anchorBounds 保持 Ask 面板与当前会话面板横向对齐。
  const [anchorBounds, setAnchorBounds] = React.useState<AskQuestionDialogAnchorBounds | null>(null);

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

  React.useEffect(() => {
    if (!activeQuestion) {
      setActiveOptionIndex(0);
      return;
    }
    const nextDraft = drafts[safeActiveIndex] ?? buildAskQuestionDraft(activeQuestion);
    setActiveOptionIndex(defaultAskQuestionOptionIndex(activeQuestion, nextDraft));
  }, [activeQuestion, safeActiveIndex]);

  React.useEffect(() => {
    if (!request || !activeQuestion || activeQuestion.options.length === 0) {
      return;
    }
    const focusFrame = window.requestAnimationFrame(() => {
      questionBarRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(focusFrame);
  }, [activeQuestion, request?.id]);

  React.useLayoutEffect(() => {
    if (!request) {
      setAnchorBounds(null);
      return;
    }
    let frameId: number | null = null;
    let resizeObserver: ResizeObserver | null = null;
    // scheduleMeasure batches layout reads behind the browser frame boundary.
    // scheduleMeasure 将布局读取合并到浏览器帧边界之后执行。
    const scheduleMeasure = () => {
      if (frameId !== null) {
        return;
      }
      frameId = window.requestAnimationFrame(() => {
        frameId = null;
        const nextBounds = readAskQuestionDialogAnchorBounds();
        setAnchorBounds((current) => (
          askQuestionDialogAnchorBoundsEqual(current, nextBounds) ? current : nextBounds
        ));
      });
    };
    const anchorElement = askQuestionDialogAnchorElement();
    scheduleMeasure();
    if (anchorElement && typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(scheduleMeasure);
      resizeObserver.observe(anchorElement);
      const shellElement = anchorElement.closest(".workspace-resizable-body-shell");
      if (shellElement instanceof HTMLElement) {
        resizeObserver.observe(shellElement);
      }
    }
    window.addEventListener("resize", scheduleMeasure);
    return () => {
      if (frameId !== null) {
        window.cancelAnimationFrame(frameId);
      }
      resizeObserver?.disconnect();
      window.removeEventListener("resize", scheduleMeasure);
    };
  }, [request?.id]);

  // updateDraft applies one immutable change to a draft row.
  // updateDraft 对单个草稿行应用一次不可变更新。
  const updateDraft = React.useCallback((questionIndex: number, nextDraft: AskQuestionDraft) => {
    setDrafts((current) => current.map((draft, index) => index === questionIndex ? nextDraft : draft));
    setValidationError(null);
  }, []);

  // moveQuestion changes the visible prompt and returns from lightweight confirmation stages.
  // moveQuestion 切换可见问题，并从轻量确认阶段返回答题阶段。
  const moveQuestion = React.useCallback((direction: -1 | 1) => {
    if (!request || questionCount === 0) {
      return;
    }
    setStage("answering");
    setValidationError(null);
    setActiveIndex((current) => Math.min(Math.max(current + direction, 0), questionCount - 1));
  }, [questionCount, request]);

  // toggleRenderedOption toggles one structured option row or the custom-answer lane.
  // toggleRenderedOption 切换一条结构化选项行或自定义回答通道。
  const toggleRenderedOption = React.useCallback((optionIndex: number) => {
    if (!activeQuestion || !activeDraft) {
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
    if (!request) {
      return;
    }
    const nextValidation = collectAskQuestionAnswers(request.questions, drafts, t);
    if (nextValidation.error) {
      setValidationError(nextValidation.error);
      setStage("answering");
      setActiveIndex(nextValidation.errorIndex ?? 0);
      return;
    }
    setBusy(true);
    try {
      await props.onSubmit(request.id, nextValidation.answers);
    } finally {
      setBusy(false);
    }
  }, [drafts, props, request, t]);

  // handleReject confirms cancellation and rejects the backend ask runtime.
  // handleReject 确认取消并拒绝后端 ask 运行时。
  const handleReject = React.useCallback(async () => {
    if (!request) {
      return;
    }
    setBusy(true);
    try {
      await props.onReject(request.id);
    } finally {
      setBusy(false);
    }
  }, [props, request]);

  // handleDialogKeyDown maps keyboard shortcuts to the compact question bar state machine.
  // handleDialogKeyDown 将键盘快捷键映射到紧凑 question bar 状态机。
  const handleDialogKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (busy) {
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

  // overlayStyle constrains the fixed Ask bar to the active conversation panel.
  // overlayStyle 将固定定位的 Ask 面板限制在当前会话面板范围内。
  const overlayStyle: React.CSSProperties = anchorBounds
    ? { left: anchorBounds.left, right: anchorBounds.right }
    : { left: 0, right: 0 };

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
      <div
        className="pointer-events-none fixed bottom-4 z-50 flex justify-center px-4"
        style={overlayStyle}
      >
        <section
          aria-describedby={descriptionId}
          aria-labelledby={titleId}
          aria-modal="true"
          className="pointer-events-auto max-h-[min(420px,calc(100vh-2rem))] w-full max-w-[760px] overflow-hidden rounded-2xl border border-border/80 bg-popover p-0 text-[13px] text-popover-foreground shadow-2xl outline-none"
          onKeyDown={handleDialogKeyDown}
          ref={questionBarRef}
          role="dialog"
          tabIndex={-1}
        >
          <div className="flex max-h-[min(420px,calc(100vh-2rem))] min-h-0 flex-col gap-2.5 p-3.5">
            <h2 className="sr-only" id={titleId}>{t("ask.title")}</h2>
            <p className="sr-only" id={descriptionId}>
              {t("ask.description")}
            </p>
            <div className="flex items-start gap-3">
              <div className="min-w-0 flex-1 text-sm leading-5 font-semibold text-popover-foreground">
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
            <div className="min-h-0 flex-1 overflow-y-auto pr-1">
              <div className="grid gap-0.5">
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
                    optionIndex={optionIndex}
                    question={activeQuestion}
                    selected={askQuestionOptionIsSelected(activeQuestion, activeDraft, descriptor)}
                    t={t}
                  />
                ))}
              </div>
            </div>
            {validationError ? (
              <div className="rounded-lg border border-destructive/25 bg-destructive/10 px-2.5 py-1.5 text-xs leading-5 text-destructive">
                {validationError}
              </div>
            ) : null}
            <AskQuestionConfirmPanel stage={stage} t={t} />
            <div className="flex items-center justify-end gap-1.5">
              <Button
                className="h-8 gap-1.5 px-2 text-xs text-muted-foreground"
                disabled={busy}
                onClick={() => {
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
                <span className="rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">ESC</span>
              </Button>
              <Button
                className="h-8 gap-1.5 rounded-xl px-3 text-xs"
                disabled={busy}
                onClick={() => void primaryAction()}
                size="sm"
                type="button"
              >
                <span>{busy ? t("ask.action.processing") : primaryLabel}</span>
                <CornerDownLeft className="size-3.5" />
              </Button>
            </div>
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
    <div className="flex shrink-0 items-center gap-0.5 text-[11px] leading-4 text-muted-foreground">
      <Button
        aria-label={props.t("ask.progress.previous")}
        disabled={props.disabled || props.activeIndex <= 0}
        onClick={props.onPrevious}
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <ChevronLeft className="size-3" />
      </Button>
      <span className="tabular-nums">{props.activeIndex + 1} of {props.total}</span>
      <Button
        aria-label={props.t("ask.progress.next")}
        disabled={props.disabled || props.activeIndex >= props.total - 1}
        onClick={props.onNext}
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <ChevronRight className="size-3" />
      </Button>
    </div>
  );
}

// askQuestionDialogAnchorElement returns the current main conversation panel used as the positioning anchor.
// askQuestionDialogAnchorElement 返回用于定位的当前主会话面板。
function askQuestionDialogAnchorElement(): HTMLElement | null {
  if (typeof document === "undefined") {
    return null;
  }
  return document.querySelector<HTMLElement>(".workspace-main-panel");
}

// readAskQuestionDialogAnchorBounds reads viewport offsets for the current conversation panel.
// readAskQuestionDialogAnchorBounds 读取当前会话面板相对视口的左右偏移。
function readAskQuestionDialogAnchorBounds(): AskQuestionDialogAnchorBounds | null {
  const anchorElement = askQuestionDialogAnchorElement();
  if (!anchorElement || typeof window === "undefined") {
    return null;
  }
  const rect = anchorElement.getBoundingClientRect();
  if (rect.width <= 0) {
    return null;
  }
  return {
    left: Math.max(0, Math.round(rect.left)),
    right: Math.max(0, Math.round(window.innerWidth - rect.right)),
  };
}

// askQuestionDialogAnchorBoundsEqual compares nullable panel bounds without triggering redundant renders.
// askQuestionDialogAnchorBoundsEqual 比较可空面板边界，避免触发冗余渲染。
function askQuestionDialogAnchorBoundsEqual(
  left: AskQuestionDialogAnchorBounds | null,
  right: AskQuestionDialogAnchorBounds | null,
): boolean {
  if (left === right) {
    return true;
  }
  if (!left || !right) {
    return false;
  }
  return left.left === right.left && left.right === right.right;
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
  // rowClassName centralizes selected and active visual states.
  // rowClassName 集中管理选中与高亮视觉状态。
  const rowClassName = cn(
    "grid grid-cols-[1.75rem_minmax(0,1fr)_auto] gap-2 rounded-xl px-2 text-left transition-colors",
    showsInlineEditor ? "items-start py-1.5" : "min-h-8 items-center py-1.5",
    props.selected || props.active ? "bg-muted/80" : "hover:bg-muted/45",
  );

  if (showsInlineEditor) {
    return (
      <div className={rowClassName}>
        <span className="pt-px text-[13px] leading-5 tabular-nums text-muted-foreground/70">{props.optionIndex + 1}.</span>
        <Textarea
          autoFocus
          className="min-h-5 resize-none rounded-none border-0 bg-transparent px-0 py-0 text-[13px] leading-5 shadow-none focus-visible:ring-0"
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
      className={rowClassName}
      disabled={props.disabled}
      onClick={props.onSelect}
      type="button"
    >
      <span className="text-[13px] leading-5 tabular-nums text-muted-foreground/70">{props.optionIndex + 1}.</span>
      <span className="flex min-w-0 items-center gap-2">
        <span className="truncate text-[13px] leading-5 font-medium">{props.descriptor.label}</span>
        <AskQuestionInfoTooltip descriptor={props.descriptor} />
        {props.selected ? (
          <span className="rounded-md bg-background px-1.5 py-0.5 text-[11px] leading-4 text-muted-foreground">
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
        <span className="inline-flex size-3.5 shrink-0 items-center justify-center rounded-full border border-border text-[10px] text-muted-foreground">
          <Info className="size-3" />
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-80 flex-col items-start whitespace-normal break-words text-left text-xs leading-5" side="top" sideOffset={6}>
        <span className="font-medium">{props.descriptor.label}</span>
        {tooltipDescription ? <span className="text-background/80">{tooltipDescription}</span> : null}
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
    return <span className="w-7" />;
  }
  return (
    <span className="flex items-center gap-0.5 pt-1 text-muted-foreground/60">
      {props.selected ? <CheckCircle2 className="mr-1 size-3.5 text-primary" /> : null}
      <ArrowUp className="size-3" />
      <ArrowDown className="size-3" />
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
    <div className="flex items-center gap-2.5 rounded-xl border border-border/70 bg-muted/45 px-2.5 py-1.5">
      <CircleAlert className={cn("size-4 shrink-0", props.stage === "rejectConfirm" ? "text-destructive" : "text-muted-foreground")} />
      <div className="min-w-0">
        <div className="text-[13px] leading-5 font-medium">{confirmTitle}</div>
        <div className="text-[11px] leading-4 text-muted-foreground">{confirmBody}</div>
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
