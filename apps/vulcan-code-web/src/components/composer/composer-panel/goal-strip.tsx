import { Edit3, ListChecks, PauseCircle, PlayCircle, Target, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import type { SessionGoalSummary } from "@/types";
import {
  formatCompactGoalTokenCount,
  formatFullGoalTokenCount,
} from "@/components/composer/composer-panel/goal-token-format";
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
  const { t, locale } = useI18n();
  // newInputTokens is the normalized non-cached input persisted for the goal.
  // newInputTokens 是为目标持久化的标准化非缓存输入。
  const newInputTokens = Math.max(0, props.goal.inputTokens ?? 0);
  // cachedInputTokens is the cache-hit input detail excluded from the visible budget usage.
  // cachedInputTokens 是不计入可见预算用量的缓存命中输入明细。
  const cachedInputTokens = Math.max(0, props.goal.cachedInputTokens ?? 0);
  // outputTokens is complete model output and already contains reasoning where the Provider reports it as a subset.
  // outputTokens 是完整模型输出；Provider 将思考报告为子集时已包含该部分。
  const outputTokens = Math.max(0, props.goal.outputTokens ?? 0);
  // reasoningTokens is the separately reported reasoning detail inside complete output.
  // reasoningTokens 是完整输出中单独报告的思考明细。
  const reasoningTokens = Math.max(0, props.goal.reasoningTokens ?? 0);
  // totalTokens preserves the cumulative Provider-reported total.
  // totalTokens 保留累计的 Provider 报告总量。
  const totalTokens = Math.max(0, props.goal.totalTokens ?? 0);
  // usageTokens is derived directly from the required new-input plus complete-output contract.
  // usageTokens 直接由要求的新增输入加完整输出契约派生。
  const usageTokens = newInputTokens + outputTokens;
  // budgetText is the compact goal usage label without a redundant token suffix.
  // budgetText 是不带冗余 token 后缀的紧凑目标用量标签。
  const budgetText = props.goal.tokenBudget
    ? `${formatCompactGoalTokenCount(usageTokens)} / ${formatCompactGoalTokenCount(props.goal.tokenBudget)}`
    : formatCompactGoalTokenCount(usageTokens);
  // usageAriaLabel exposes the same five exact Tooltip details to assistive technology.
  // usageAriaLabel 向辅助技术公开与 Tooltip 相同的五项精确明细。
  const usageAriaLabel = [
    `${t("composer.goal.tooltip.input")}: ${formatFullGoalTokenCount(newInputTokens, locale)}`,
    `${t("composer.goal.tooltip.cache")}: ${formatFullGoalTokenCount(cachedInputTokens, locale)}`,
    `${t("composer.goal.tooltip.output")}: ${formatFullGoalTokenCount(outputTokens, locale)}`,
    `${t("composer.goal.tooltip.reasoning")}: ${formatFullGoalTokenCount(reasoningTokens, locale)}`,
    `${t("composer.goal.tooltip.total")}: ${formatFullGoalTokenCount(totalTokens, locale)}`,
  ].join(", ");
  // usageTriggerAriaLabel preserves the visible used/budget value before the detailed breakdown.
  // usageTriggerAriaLabel 在详细明细前保留可见的已用量与预算值。
  const usageTriggerAriaLabel = `${budgetText}. ${usageAriaLabel}`;
  return (
    <div className="composer-goal-strip">
      <Target size={15} />
      <span className="composer-goal-strip-status">{composerGoalStatusLabel(props.goal.status, t)}</span>
      <span className="composer-goal-strip-objective">{props.goal.objective}</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            aria-label={usageTriggerAriaLabel}
            className="composer-goal-strip-budget"
            tabIndex={0}
          >
            {budgetText}
          </span>
        </TooltipTrigger>
        <TooltipContent
          className="composer-goal-usage-tooltip"
          side="top"
          sideOffset={6}
        >
          <dl>
            <div>
              <dt>{t("composer.goal.tooltip.input")}</dt>
              <dd>{formatFullGoalTokenCount(newInputTokens, locale)}</dd>
            </div>
            <div>
              <dt>{t("composer.goal.tooltip.cache")}</dt>
              <dd>{formatFullGoalTokenCount(cachedInputTokens, locale)}</dd>
            </div>
            <div>
              <dt>{t("composer.goal.tooltip.output")}</dt>
              <dd>{formatFullGoalTokenCount(outputTokens, locale)}</dd>
            </div>
            <div>
              <dt>{t("composer.goal.tooltip.reasoning")}</dt>
              <dd>{formatFullGoalTokenCount(reasoningTokens, locale)}</dd>
            </div>
            <div>
              <dt>{t("composer.goal.tooltip.total")}</dt>
              <dd>{formatFullGoalTokenCount(totalTokens, locale)}</dd>
            </div>
          </dl>
        </TooltipContent>
      </Tooltip>
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

