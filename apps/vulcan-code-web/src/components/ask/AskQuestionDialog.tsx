import React from "react";
import { CheckCircle2, CircleAlert, MessageSquareText, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import type {
  PendingQuestionOptionSummary,
  PendingQuestionPromptSummary,
  PendingQuestionSnapshot,
} from "@/types";

// AskQuestionDialogProps describes callbacks and request data for the ask modal.
// AskQuestionDialogProps 描述 ask 弹窗使用的请求数据与回调。
interface AskQuestionDialogProps {
  // Pending request shown by the dialog.
  // 弹窗展示的待处理请求。
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

// AskQuestionDialog renders the blocking ask-tool UI with explicit cancel confirmation.
// AskQuestionDialog 渲染带显式取消确认的阻塞式 ask 工具界面。
export function AskQuestionDialog(props: AskQuestionDialogProps) {
  const request = props.request;
  const [activeIndex, setActiveIndex] = React.useState(0);
  const [stage, setStage] = React.useState<"answering" | "confirm">("answering");
  const [drafts, setDrafts] = React.useState<AskQuestionDraft[]>([]);
  const [validationError, setValidationError] = React.useState<string | null>(null);
  const [cancelConfirmOpen, setCancelConfirmOpen] = React.useState(false);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    setActiveIndex(0);
    setStage("answering");
    setDrafts(request ? buildAskQuestionDrafts(request.questions) : []);
    setValidationError(null);
    setCancelConfirmOpen(false);
    setBusy(false);
  }, [request?.id]);

  const questionCount = request?.questions.length ?? 0;
  const activeQuestion = request?.questions[Math.min(activeIndex, Math.max(questionCount - 1, 0))] ?? null;
  const activeDraft = drafts[Math.min(activeIndex, Math.max(drafts.length - 1, 0))] ?? null;
  const validation = React.useMemo(
    () => request ? collectAskQuestionAnswers(request.questions, drafts) : { answers: [], error: null, errorIndex: null },
    [drafts, request],
  );

  // updateDraft applies one immutable change to a draft row.
  // updateDraft 对单个草稿行应用一次不可变更新。
  const updateDraft = React.useCallback((questionIndex: number, update: (draft: AskQuestionDraft) => AskQuestionDraft) => {
    setDrafts((current) => current.map((draft, index) => index === questionIndex ? update(draft) : draft));
    setValidationError(null);
  }, []);

  // handleSubmit validates answers, then resumes the backend ask runtime.
  // handleSubmit 校验答案并恢复后端 ask 运行时。
  const handleSubmit = React.useCallback(async () => {
    if (!request) {
      return;
    }
    const nextValidation = collectAskQuestionAnswers(request.questions, drafts);
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
  }, [drafts, props, request]);

  // handleReject confirms cancellation and rejects the backend ask runtime.
  // handleReject 确认取消并拒绝后端 ask 运行时。
  const handleReject = React.useCallback(async () => {
    if (!request) {
      return;
    }
    setBusy(true);
    try {
      await props.onReject(request.id);
      setCancelConfirmOpen(false);
    } finally {
      setBusy(false);
    }
  }, [props, request]);

  if (!request || questionCount === 0) {
    return null;
  }

  const allAnswered = validation.error === null;
  const isLastQuestion = activeIndex >= questionCount - 1;

  return (
    <>
      <Dialog open>
        <DialogContent
          className="max-h-[min(760px,calc(100vh-2rem))] w-[min(720px,calc(100vw-2rem))] overflow-hidden p-0 sm:max-w-[720px]"
          onEscapeKeyDown={(event) => {
            event.preventDefault();
            setCancelConfirmOpen(true);
          }}
          onInteractOutside={(event) => event.preventDefault()}
          showCloseButton={false}
        >
          <DialogHeader className="border-b px-5 py-4">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <DialogTitle className="flex items-center gap-2 text-base">
                  <MessageSquareText className="size-4" />
                  ask 需要你的确认
                </DialogTitle>
                <DialogDescription className="mt-1">
                  当前 AI 调用正在等待这些回答，提交后会继续执行。
                </DialogDescription>
              </div>
              <Button
                aria-label="关闭 ask 弹窗"
                disabled={busy}
                onClick={() => setCancelConfirmOpen(true)}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <X />
              </Button>
            </div>
          </DialogHeader>
          <div className="grid min-h-0 grid-cols-[180px_minmax(0,1fr)]">
            <aside className="border-r bg-muted/30 p-3">
              <div className="flex flex-col gap-1">
                {request.questions.map((question, questionIndex) => {
                  const draftValidation = collectAskQuestionPromptAnswers(question, drafts[questionIndex] ?? buildAskQuestionDraft(question));
                  const isActive = stage === "answering" && activeIndex === questionIndex;
                  return (
                    <Button
                      className={cn(
                        "h-auto justify-start gap-2 px-2 py-2 text-left",
                        isActive ? "bg-background shadow-xs" : "bg-transparent",
                      )}
                      key={`${request.id}-${question.header}-${questionIndex}`}
                      onClick={() => {
                        setStage("answering");
                        setActiveIndex(questionIndex);
                      }}
                      type="button"
                      variant="ghost"
                    >
                      <span className={cn("size-2 rounded-full", draftValidation.error ? "bg-muted-foreground/35" : "bg-primary")} />
                      <span className="min-w-0 truncate">{question.header || `问题 ${questionIndex + 1}`}</span>
                    </Button>
                  );
                })}
                <Button
                  className={cn("mt-2 h-auto justify-start gap-2 px-2 py-2 text-left", stage === "confirm" ? "bg-background shadow-xs" : "bg-transparent")}
                  onClick={() => setStage("confirm")}
                  type="button"
                  variant="ghost"
                >
                  <CheckCircle2 className={cn("size-4", allAnswered ? "text-primary" : "text-muted-foreground")} />
                  <span>确认提交</span>
                </Button>
              </div>
            </aside>
            <section className="min-h-[420px] overflow-y-auto p-5">
              {stage === "answering" && activeQuestion && activeDraft ? (
                <AskQuestionEditor
                  draft={activeDraft}
                  error={validationError}
                  onChange={(nextDraft) => updateDraft(activeIndex, () => nextDraft)}
                  question={activeQuestion}
                  questionIndex={activeIndex}
                  questionTotal={questionCount}
                />
              ) : (
                <AskQuestionConfirm
                  answers={validation.answers}
                  errorIndex={validation.errorIndex}
                  questions={request.questions}
                  onSelectQuestion={(questionIndex) => {
                    setStage("answering");
                    setActiveIndex(questionIndex);
                  }}
                />
              )}
            </section>
          </div>
          <DialogFooter className="rounded-none">
            <Button
              disabled={busy}
              onClick={() => setCancelConfirmOpen(true)}
              type="button"
              variant="outline"
            >
              取消
            </Button>
            {stage === "answering" ? (
              <Button
                disabled={busy}
                onClick={() => {
                  if (isLastQuestion) {
                    setStage("confirm");
                  } else {
                    setActiveIndex((current) => Math.min(current + 1, questionCount - 1));
                  }
                }}
                type="button"
              >
                {isLastQuestion ? "进入确认" : "下一题"}
              </Button>
            ) : (
              <Button disabled={busy || !allAnswered} onClick={handleSubmit} type="button">
                {busy ? "提交中..." : "提交回答"}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={cancelConfirmOpen} onOpenChange={setCancelConfirmOpen}>
        <DialogContent
          className="w-[min(420px,calc(100vw-2rem))] sm:max-w-[420px]"
          onEscapeKeyDown={(event) => {
            event.preventDefault();
            setCancelConfirmOpen(false);
          }}
          onInteractOutside={(event) => event.preventDefault()}
          showCloseButton={false}
        >
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <CircleAlert className="size-4 text-destructive" />
              确认取消 ask？
            </DialogTitle>
            <DialogDescription>
              取消后当前 ask 工具调用会以失败结束，本轮 AI 执行会被中断。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button disabled={busy} onClick={() => setCancelConfirmOpen(false)} type="button" variant="outline">
              继续回答
            </Button>
            <Button disabled={busy} onClick={handleReject} type="button" variant="destructive">
              {busy ? "取消中..." : "确认取消"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

// AskQuestionEditor renders one editable ask prompt page.
// AskQuestionEditor 渲染单个可编辑 ask 问题页面。
function AskQuestionEditor(props: {
  // Question currently being edited.
  // 当前正在编辑的问题。
  question: PendingQuestionPromptSummary;
  // Draft belonging to the current question.
  // 当前问题对应的草稿。
  draft: AskQuestionDraft;
  // Zero-based question index.
  // 从零开始的问题下标。
  questionIndex: number;
  // Total number of questions in the request.
  // 当前请求中的问题总数。
  questionTotal: number;
  // Current validation error shown by the parent modal.
  // 父级弹窗传入的当前校验错误。
  error: string | null;
  // Emits the edited draft.
  // 发出编辑后的草稿。
  onChange: (draft: AskQuestionDraft) => void;
}) {
  const hasOptions = props.question.options.length > 0;
  const customPlaceholder = hasOptions ? "填写其他回答..." : "输入你的回答...";
  const toggleOption = (option: PendingQuestionOptionSummary) => {
    if (props.question.multiple) {
      const selected = props.draft.selectedLabels.includes(option.label)
        ? props.draft.selectedLabels.filter((label) => label !== option.label)
        : [...props.draft.selectedLabels, option.label];
      props.onChange({ ...props.draft, selectedLabels: selected });
      return;
    }
    props.onChange({ ...props.draft, customSelected: false, selectedLabels: [option.label] });
  };

  return (
    <div className="flex min-h-full flex-col gap-5">
      <div>
        <div className="text-xs font-medium text-muted-foreground">
          第 {props.questionIndex + 1}/{props.questionTotal} 题
        </div>
        <h2 className="mt-2 text-lg font-semibold">{props.question.header}</h2>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{props.question.question}</p>
      </div>
      {hasOptions ? (
        <div className="grid gap-2">
          {props.question.options.map((option) => {
            const selected = props.draft.selectedLabels.includes(option.label);
            return (
              <button
                className={cn(
                  "rounded-lg border p-3 text-left transition-colors",
                  selected ? "border-primary bg-primary/5" : "border-border hover:bg-muted/60",
                )}
                key={option.label}
                onClick={() => toggleOption(option)}
                type="button"
              >
                <span className="flex items-start gap-3">
                  <span className={cn("mt-1 size-3 rounded-full border", selected ? "border-primary bg-primary" : "border-muted-foreground/40")} />
                  <span className="min-w-0">
                    <span className="block text-sm font-medium">{option.label}</span>
                    <span className="mt-1 block text-xs leading-5 text-muted-foreground">{option.description}</span>
                  </span>
                </span>
              </button>
            );
          })}
          <button
            className={cn(
              "rounded-lg border p-3 text-left transition-colors",
              props.draft.customSelected ? "border-primary bg-primary/5" : "border-border hover:bg-muted/60",
            )}
            onClick={() => props.onChange({ ...props.draft, customSelected: !props.draft.customSelected })}
            type="button"
          >
            <span className="text-sm font-medium">其他回答</span>
            <span className="mt-1 block text-xs text-muted-foreground">使用自定义文本补充或替代上述选项。</span>
          </button>
        </div>
      ) : null}
      {(!hasOptions || props.draft.customSelected) ? (
        <Textarea
          autoFocus
          className="min-h-28 resize-none"
          onChange={(event) => props.onChange({
            ...props.draft,
            customSelected: true,
            customText: event.target.value,
          })}
          placeholder={customPlaceholder}
          value={props.draft.customText}
        />
      ) : null}
      {props.error ? (
        <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {props.error}
        </div>
      ) : null}
    </div>
  );
}

// AskQuestionConfirm renders the final answer review page.
// AskQuestionConfirm 渲染最终答案确认页面。
function AskQuestionConfirm(props: {
  // Original questions.
  // 原始问题集合。
  questions: PendingQuestionPromptSummary[];
  // Collected answers aligned with the original questions.
  // 与原始问题对齐的已收集答案。
  answers: string[][];
  // First invalid question index.
  // 首个无效问题下标。
  errorIndex: number | null;
  // Opens one question for editing.
  // 打开单个问题进行编辑。
  onSelectQuestion: (questionIndex: number) => void;
}) {
  return (
    <div className="flex min-h-full flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold">确认回答</h2>
        <p className="mt-2 text-sm text-muted-foreground">
          检查无误后提交，AI 会继续当前工具调用。
        </p>
      </div>
      <div className="grid gap-2">
        {props.questions.map((question, questionIndex) => {
          const answerText = props.answers[questionIndex]?.join("、") ?? "";
          const hasError = props.errorIndex === questionIndex || !answerText;
          return (
            <button
              className={cn(
                "rounded-lg border p-3 text-left transition-colors hover:bg-muted/60",
                hasError ? "border-destructive/40" : "border-border",
              )}
              key={`${question.header}-${questionIndex}`}
              onClick={() => props.onSelectQuestion(questionIndex)}
              type="button"
            >
              <span className="flex items-start justify-between gap-3">
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{question.header}</span>
                  <span className="mt-1 block text-xs leading-5 text-muted-foreground">{question.question}</span>
                  <span className={cn("mt-2 block text-sm", answerText ? "text-foreground" : "text-destructive")}>
                    {answerText || "尚未填写"}
                  </span>
                </span>
                {hasError ? <CircleAlert className="size-4 shrink-0 text-destructive" /> : <CheckCircle2 className="size-4 shrink-0 text-primary" />}
              </span>
            </button>
          );
        })}
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

// collectAskQuestionAnswers validates every prompt and returns ordered answers.
// collectAskQuestionAnswers 校验全部问题并返回有序答案。
function collectAskQuestionAnswers(
  questions: PendingQuestionPromptSummary[],
  drafts: AskQuestionDraft[],
): AskQuestionValidationResult {
  const answers: string[][] = [];
  for (let index = 0; index < questions.length; index += 1) {
    const question = questions[index];
    const draft = drafts[index] ?? buildAskQuestionDraft(question);
    const result = collectAskQuestionPromptAnswers(question, draft);
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
): { answers: string[]; error: string | null } {
  const trimmedCustom = draft.customText.trim();
  if (question.options.length === 0) {
    return trimmedCustom
      ? { answers: [trimmedCustom], error: null }
      : { answers: [], error: `请先填写“${question.header}”的回答。` };
  }
  const answers = [...draft.selectedLabels];
  if (draft.customSelected) {
    if (!trimmedCustom) {
      return { answers: [], error: `请先填写“${question.header}”的其他回答。` };
    }
    answers.push(trimmedCustom);
  }
  if (answers.length === 0) {
    return { answers: [], error: `请先选择“${question.header}”的答案。` };
  }
  return { answers, error: null };
}
