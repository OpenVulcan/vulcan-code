import { Edit3, ListChecks, PauseCircle, PlayCircle, Target, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import type { SessionGoalSummary } from "@/types";
import { composerGoalToggleTarget } from "./goal-status-action";
import { ComposerGoalDetails } from "./goal-details";
import { useGoalWorkDuration } from "./goal-work-duration";

// ComposerGoalStrip renders the objective and lifecycle controls; props supply the authoritative goal and actions.
// ComposerGoalStrip 渲染目标与生命周期控件；props 提供权威目标及操作，返回紧凑目标条。
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
  // toggleTarget owns both the icon and command semantics without overwriting the persisted blocked reason.
  // toggleTarget 同时决定图标与命令语义，不覆盖持久化的受阻原因。
  const toggleTarget = composerGoalToggleTarget(props.goal.status);
  // duration updates only this display clock; persistence belongs to backend lifecycle events.
  // duration 仅更新展示时钟；持久化由后端生命周期事件负责。
  const duration = useGoalWorkDuration(props.goal);
  return (
    <div className="composer-goal-strip">
      <Target size={15} />
      <span className="composer-goal-strip-status">{composerGoalStatusLabel(props.goal.status, t)}</span>
      <TooltipProvider delayDuration={500} skipDelayDuration={0}>
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="composer-goal-strip-objective" tabIndex={0}>{props.goal.objective}</span>
          </TooltipTrigger>
          <TooltipContent className="context-usage-tooltip composer-goal-details-tooltip" side="top" sideOffset={6} showArrow={false}>
            <ComposerGoalDetails goal={props.goal} />
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
      <span className="composer-goal-strip-duration" aria-label={`${t("composer.goal.duration")}: ${duration}`}>{duration}</span>
      <Button aria-label={t("composer.goal.action.edit")} className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onEdit} size="icon-sm" type="button" variant="ghost">
        <Edit3 size={14} />
      </Button>
      {props.goal.status === "budget_limited" ? (
        <Button aria-label={t("composer.goal.action.resumeTask")} className="composer-goal-strip-button text" disabled={props.disabled} onClick={props.onResume} type="button" variant="ghost">
          <PlayCircle size={14} />
          <span>{t("composer.goal.action.resumeTask")}</span>
        </Button>
      ) : toggleTarget !== null ? (
        <Button aria-label={toggleTarget === "active" ? t("composer.goal.action.resume") : t("composer.goal.action.pause")} className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onToggleStatus} size="icon-sm" type="button" variant="ghost">
          {toggleTarget === "active" ? <PlayCircle size={14} /> : <PauseCircle size={14} />}
        </Button>
      ) : null}
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

