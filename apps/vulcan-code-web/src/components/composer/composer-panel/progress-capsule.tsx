import React from "react";
import { CheckCircle2, Circle } from "lucide-react";
import { cn } from "@/lib/utils";
import { ProgressRing } from "@/components/ui/progress-ring";
import { useI18n, type TranslationKey } from "@/i18n";
import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type { SessionTodolistItemSummary, SessionTodolistSummary } from "@/types";
export function ComposerSessionProgressCapsule(props: {
  // Current session progress todolist displayed by this capsule.
  // 当前胶囊展示的会话进程 todolist。
  todolist: SessionTodolistSummary;
  // Optional Git review summary shown only as compact change counters.
  // 可选 Git 审查摘要，仅作为紧凑变更计数展示。
  reviewSummary: ConversationSessionReviewSummary | null;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const currentStepNumber = composerSessionCurrentStepNumber(props.todolist);
  const progressValue = composerSessionProgressValue(currentStepNumber, props.todolist.totalCount);
  const reviewSummary = props.reviewSummary?.changedFiles ? props.reviewSummary : null;
  return (
    <div className="composer-session-progress-root">
      <button
        aria-label={t("composer.progress.open")}
        className="composer-session-progress-capsule"
        type="button"
      >
        <ProgressRing aria-label={formatProgressTranslation(t, "composer.progress.progressLabel", { current: String(currentStepNumber), total: String(props.todolist.totalCount) })} size={14} value={progressValue} />
        <span>{formatProgressTranslation(t, "composer.progress.step", { current: String(currentStepNumber), total: String(props.todolist.totalCount) })}</span>
        {reviewSummary ? (
          <span className="composer-session-progress-review-summary">
            <span>· {formatProgressTranslation(t, "composer.progress.changedFiles", { count: String(reviewSummary.changedFiles) })}</span>
            <span className="added">+{reviewSummary.additions}</span>
            <span className="removed">-{reviewSummary.deletions}</span>
          </span>
        ) : null}
      </button>
      <div className="composer-session-progress-popover" role="tooltip">
        <div className="composer-session-progress-list">
          {props.todolist.items.map((item, index) => (
            <div className={cn("composer-session-progress-row", `status-${item.status}`)} key={`${index}-${item.status}-${item.step}`}>
              {composerSessionProgressStatusIcon(item.status, t)}
              <span>{item.step}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// composerSessionProgressValue converts the displayed step count into a bounded ring ratio.
// composerSessionProgressValue 将展示用步骤计数转换为受限的圆环比例。
function composerSessionProgressValue(currentStepNumber: number, totalCount: number): number {
  if (!Number.isFinite(currentStepNumber) || !Number.isFinite(totalCount) || totalCount <= 0) {
    return 0;
  }
  return Math.min(1, Math.max(0, currentStepNumber / totalCount));
}

// composerSessionCurrentStepNumber resolves the one-based active or next step number.
// composerSessionCurrentStepNumber 解析当前或下一步的一基序号。
function composerSessionCurrentStepNumber(todolist: SessionTodolistSummary): number {
  const activeIndex = todolist.items.findIndex((item) => item.status === "in_progress");
  if (activeIndex >= 0) {
    return activeIndex + 1;
  }
  return Math.min(todolist.totalCount, Math.max(1, todolist.completedCount + 1));
}

// composerSessionProgressStatusIcon returns the compact icon for one progress status.
// composerSessionProgressStatusIcon 返回单个进程状态的紧凑图标。
function composerSessionProgressStatusIcon(status: SessionTodolistItemSummary["status"], t: (key: TranslationKey) => string): React.ReactNode {
  if (status === "completed") {
    return <CheckCircle2 size={15} />;
  }
  if (status === "in_progress") {
    return <ProgressRing aria-label={t("composer.progress.stepInProgress")} size={15} value={null} />;
  }
  return <Circle size={15} />;
}

// formatProgressTranslation replaces named placeholders in localized progress labels.
// formatProgressTranslation 替换进度胶囊本地化标签中的具名占位符。
function formatProgressTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

