import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { Button } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { useI18n } from "@/i18n";

// ComposerGoalDialogProps carries the controlled goal editing dialog state.
// ComposerGoalDialogProps 承载受控目标编辑弹窗状态。
export interface ComposerGoalDialogProps {
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean;
  // Whether the dialog is saving the goal.
  // 弹窗是否正在保存目标。
  saving: boolean;
  // Current dialog mode.
  // 当前弹窗模式。
  mode: "edit" | "resume";
  // Controlled objective draft.
  // 受控目标草稿。
  objective: string;
  // Controlled optional token budget draft.
  // 受控可选 Token 预算草稿。
  tokenBudget: string;
  // Handler that changes open state.
  // 修改打开状态的处理器。
  onOpenChange: (open: boolean) => void;
  // Handler that updates objective text.
  // 更新目标文本的处理器。
  onObjectiveChange: (value: string) => void;
  // Handler that updates token budget text.
  // 更新 Token 预算文本的处理器。
  onTokenBudgetChange: (value: string) => void;
  // Handler that saves or resumes the goal.
  // 保存或继续目标的处理器。
  onSave: () => void;
}

// ComposerGoalDialog renders goal creation, editing, and resume confirmation.
// ComposerGoalDialog 渲染目标创建、编辑与继续确认弹窗。
// Parameters: props supplies goal drafts, validation and controller actions; returns the goal form without rewriting invalid raw budget input.
// 参数：props 提供目标草稿、校验与控制器操作；返回不改写非法预算原始输入的目标表单。
export function ComposerGoalDialog(props: ComposerGoalDialogProps) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <Dialog open={props.open} onOpenChange={(open) => { if (!props.saving) props.onOpenChange(open); }}>
      <DialogContent size="sm" scrollMode="fixed-footer">
        <DialogHeader>
          <DialogTitle>{props.mode === "resume" ? t("composer.goalDialog.resumeTitle") : t("composer.goalDialog.editTitle")}</DialogTitle>
        </DialogHeader>
        <div className="vk-goal-form">
          <Textarea
            aria-label={t("composer.goalDialog.objectiveAria")}
            disabled={props.saving}
            onChange={(event) => props.onObjectiveChange(event.currentTarget.value)}
            placeholder={t("composer.goalDialog.objectivePlaceholder")}
            value={props.objective}
          />
          <Input
            aria-label={t("composer.goalDialog.tokenBudgetAria")}
            disabled={props.saving}
            inputMode="numeric"
            onChange={(event) => props.onTokenBudgetChange(event.currentTarget.value)}
            placeholder={t("composer.goalDialog.tokenBudgetPlaceholder")}
            value={props.tokenBudget}
          />
        </div>
        <DialogFooter>
          <Button disabled={props.saving} onClick={() => props.onOpenChange(false)} type="button" variant="secondary">
            {t("conversation.action.cancel")}
          </Button>
          <Button disabled={props.saving || !props.objective.trim()} onClick={props.onSave} type="button">
            {props.mode === "resume" ? t("composer.goalDialog.resumeTitle") : t("composer.goalDialog.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
