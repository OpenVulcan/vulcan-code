import React from "react";
import { Edit3, ListChecks, PauseCircle, PlayCircle, Target, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
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
  const usageTokens = props.goal.usageTokens ?? props.goal.tokensUsed;
  const budgetText = props.goal.tokenBudget
    ? `${usageTokens}/${props.goal.tokenBudget} tokens`
    : `${usageTokens} tokens`;
  const usageTooltip = [
    `API 次数：${props.goal.apiCallCount ?? 0}`,
    `工具调用：${props.goal.toolCallCount ?? 0}`,
    `用量：${usageTokens} tokens`,
    `输入：${props.goal.inputTokens ?? 0}`,
    `缓存：${props.goal.cachedInputTokens ?? 0}`,
    `输出：${props.goal.outputTokens ?? 0}`,
    `总量：${props.goal.totalTokens ?? 0}`,
  ].join("\n");
  return (
    <div className="composer-goal-strip">
      <Target size={15} />
      <span className="composer-goal-strip-status">{composerGoalStatusLabel(props.goal.status)}</span>
      <span className="composer-goal-strip-objective">{props.goal.objective}</span>
      <span className="composer-goal-strip-budget" title={usageTooltip}>{budgetText}</span>
      <Button aria-label="编辑目标" className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onEdit} size="icon-sm" type="button" variant="ghost">
        <Edit3 size={14} />
      </Button>
      {props.goal.status === "budget_limited" ? (
        <Button aria-label="继续任务" className="composer-goal-strip-button text" disabled={props.disabled} onClick={props.onResume} type="button" variant="ghost">
          <PlayCircle size={14} />
          <span>继续任务</span>
        </Button>
      ) : (
        <Button aria-label={props.goal.status === "paused" ? "恢复目标" : "暂停目标"} className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onToggleStatus} size="icon-sm" type="button" variant="ghost">
          {props.goal.status === "paused" ? <PlayCircle size={14} /> : <PauseCircle size={14} />}
        </Button>
      )}
      <Button aria-label="清理目标" className="composer-goal-strip-button" disabled={props.disabled} onClick={props.onClear} size="icon-sm" type="button" variant="ghost">
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
  return (
    <button className="composer-plan-mode-chip" onClick={props.onClose} title={props.artifactName ?? "计划模式"} type="button">
      <span className="composer-plan-mode-icon">
        <ListChecks size={14} />
      </span>
      <span>计划</span>
      <span className="composer-plan-mode-close">
        <X size={12} />
      </span>
    </button>
  );
}

// composerGoalStatusLabel returns a compact Chinese status label for one goal status.
// composerGoalStatusLabel 返回单个目标状态的紧凑中文标签。
function composerGoalStatusLabel(status: SessionGoalSummary["status"]): string {
  if (status === "active") return "进行中的目标";
  if (status === "paused") return "已暂停的目标";
  if (status === "blocked") return "受阻的目标";
  if (status === "usage_limited") return "受限的目标";
  if (status === "budget_limited") return "预算用尽";
  return "已完成的目标";
}

