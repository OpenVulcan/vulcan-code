import React from "react";
import { Edit3, ListChecks, PauseCircle, PlayCircle, Target, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { SessionGoalSummary } from "@/types";
export function ComposerGoalStrip(props: {
  // Current persisted goal.
  // 当前持久化目标。
  goal: SessionGoalSummary;
  // Whether goal actions are temporarily disabled.
  // 目标操作是否临时禁用。
  disabled: boolean;
  // Opens the goal editor.
  // 打开目标编辑器。
  onEdit: () => void;
  // Opens the stopped-goal resume editor.
  // 打开已停止目标的继续编辑器。
  onResume: () => void;
  // Pauses or resumes the goal.
  // 暂停或恢复目标。
  onToggleStatus: () => void;
  // Clears the goal.
  // 清理目标。
  onClear: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const usageTokens = props.goal.usageTokens ?? props.goal.tokensUsed;
  const budgetText = props.goal.tokenBudget
    ? `${usageTokens}/${props.goal.tokenBudget} tokens`
    : `${usageTokens} tokens`;
  const usageTooltip = [
    formatGoalTranslation(t, "composer.goal.tooltip.apiCalls", { count: String(props.goal.apiCallCount ?? 0) }),
    formatGoalTranslation(t, "composer.goal.tooltip.toolCalls", { count: String(props.goal.toolCallCount ?? 0) }),
    formatGoalTranslation(t, "composer.goal.tooltip.usage", { tokens: String(usageTokens) }),
    formatGoalTranslation(t, "composer.goal.tooltip.input", { tokens: String(props.goal.inputTokens ?? 0) }),
    formatGoalTranslation(t, "composer.goal.tooltip.cache", { tokens: String(props.goal.cachedInputTokens ?? 0) }),
    formatGoalTranslation(t, "composer.goal.tooltip.output", { tokens: String(props.goal.outputTokens ?? 0) }),
    formatGoalTranslation(t, "composer.goal.tooltip.total", { tokens: String(props.goal.totalTokens ?? 0) }),
  ].join("\n");
  return (
    <div className="composer-goal-strip">
      <Target size={15} />
      <span className="composer-goal-strip-status">{composerGoalStatusLabel(props.goal.status, t)}</span>
      <span className="composer-goal-strip-objective">{props.goal.objective}</span>
      <span className="composer-goal-strip-budget" title={usageTooltip}>{budgetText}</span>
      <Button aria-label={t("composer.goal.action.edit")} className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onEdit} size="icon-sm" type="button" variant="ghost">
        <Edit3 size={14} />
      </Button>
      {props.goal.status === "budget_limited" ? (
        <Button aria-label={t("composer.goal.action.resumeTask")} className="composer-goal-strip-button text" disabled={props.disabled} onClick={props.onResume} type="button" variant="ghost">
          <PlayCircle size={14} />
          <span>{t("composer.goal.action.resumeTask")}</span>
        </Button>
      ) : (
        <Button aria-label={props.goal.status === "paused" ? t("composer.goal.action.resume") : t("composer.goal.action.pause")} className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onToggleStatus} size="icon-sm" type="button" variant="ghost">
          {props.goal.status === "paused" ? <PlayCircle size={14} /> : <PauseCircle size={14} />}
        </Button>
      )}
      <Button aria-label={t("composer.goal.action.clear")} className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onClear} size="icon-sm" type="button" variant="ghost">
        <Trash2 size={14} />
      </Button>
    </div>
  );
}

// ComposerPlanModeChip renders the active Plan mode capsule in the toolbar.
// ComposerPlanModeChip 在工具栏渲染已开启计划模式的胶囊。
export function ComposerPlanModeChip(props: {
  // Optional latest plan artifact name.
  // 可选的最近计划产物名称。
  artifactName: string | null;
  // Closes Plan mode.
  // 关闭计划模式。
  onClose: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <button className="composer-plan-mode-chip" onClick={props.onClose} title={props.artifactName ?? t("composer.planMode")} type="button">
      <span className="composer-plan-mode-icon">
        <ListChecks size={14} />
      </span>
      <span>{t("composer.planMode.label")}</span>
      <span className="composer-plan-mode-close">
        <X size={12} />
      </span>
    </button>
  );
}

// composerGoalStatusLabel returns a compact localized status label for one goal status.
// composerGoalStatusLabel 返回单个目标状态的紧凑本地化标签。
function composerGoalStatusLabel(status: SessionGoalSummary["status"], t: (key: TranslationKey) => string): string {
  if (status === "active") return t("composer.goal.status.active");
  if (status === "paused") return t("composer.goal.status.paused");
  if (status === "blocked") return t("composer.goal.status.blocked");
  if (status === "usage_limited") return t("composer.goal.status.usageLimited");
  if (status === "budget_limited") return t("composer.goal.status.budgetLimited");
  return t("composer.goal.status.completed");
}

// formatGoalTranslation replaces simple named placeholders in localized goal templates.
// formatGoalTranslation 替换目标条本地化模板中的简单具名占位符。
function formatGoalTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

