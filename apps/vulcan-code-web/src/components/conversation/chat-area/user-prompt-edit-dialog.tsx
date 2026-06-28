import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import type { UserPromptEditDraftSummary } from "@/types";

// UserPromptEditDialogProps carries the controlled edit dialog state from ChatArea.
// UserPromptEditDialogProps 承载来自 ChatArea 的受控编辑弹窗状态。
export interface UserPromptEditDialogProps {
  // Localized edit dialog title and textarea label.
  // 本地化编辑弹窗标题与文本框标签。
  title: string;
  // Localized cancel button label.
  // 本地化取消按钮文案。
  cancelLabel: string;
  // Localized submit button label.
  // 本地化提交按钮文案。
  submitLabel: string;
  // Backend-validated edit draft currently being edited.
  // 当前正在编辑的后端校验草稿。
  draft: UserPromptEditDraftSummary | null;
  // Controlled prompt text.
  // 受控提示词文本。
  text: string;
  // Whether an edit submission is currently in flight.
  // 当前是否正在提交编辑。
  saving: boolean;
  // Handler that closes the dialog.
  // 关闭弹窗的处理器。
  onClose: () => void;
  // Handler that updates controlled prompt text.
  // 更新受控提示词文本的处理器。
  onTextChange: (text: string) => void;
  // Handler that submits the current edit.
  // 提交当前编辑的处理器。
  onSubmit: () => void;
}

// UserPromptEditDialog renders the replay edit confirmation dialog for one user prompt.
// UserPromptEditDialog 渲染单条用户提示词的重放编辑确认弹窗。
export function UserPromptEditDialog(props: UserPromptEditDialogProps) {
  return (
    <Dialog open={Boolean(props.draft)} onOpenChange={(open) => !open && props.onClose()}>
      <DialogContent className="user-prompt-edit-dialog">
        <DialogHeader>
          <DialogTitle>{props.title}</DialogTitle>
        </DialogHeader>
        <Textarea
          aria-label={props.title}
          className="user-prompt-edit-textarea"
          onChange={(event) => props.onTextChange(event.currentTarget.value)}
          value={props.text}
        />
        <DialogFooter>
          <Button disabled={props.saving} onClick={props.onClose} type="button" variant="secondary">
            {props.cancelLabel}
          </Button>
          <Button disabled={props.saving || !props.text.trim()} onClick={props.onSubmit} type="button">
            {props.submitLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
