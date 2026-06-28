import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";

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
export function ComposerGoalDialog(props: ComposerGoalDialogProps) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="composer-goal-dialog">
        <DialogHeader>
          <DialogTitle>{props.mode === "resume" ? "继续任务" : "编辑目标"}</DialogTitle>
        </DialogHeader>
        <div className="composer-goal-dialog-body">
          <Textarea
            aria-label="目标内容"
            className="composer-goal-objective-input"
            onChange={(event) => props.onObjectiveChange(event.currentTarget.value)}
            placeholder="输入当前会话要持续推进的目标"
            value={props.objective}
          />
          <input
            aria-label="Token 预算"
            className="composer-goal-budget-input"
            inputMode="numeric"
            onChange={(event) => props.onTokenBudgetChange(event.currentTarget.value.replace(/[^\d]/g, ""))}
            placeholder="Token 预算（可选）"
            value={props.tokenBudget}
          />
        </div>
        <DialogFooter>
          <Button disabled={props.saving} onClick={() => props.onOpenChange(false)} type="button" variant="secondary">
            取消
          </Button>
          <Button disabled={props.saving} onClick={props.onSave} type="button">
            {props.mode === "resume" ? "继续任务" : "保存"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
