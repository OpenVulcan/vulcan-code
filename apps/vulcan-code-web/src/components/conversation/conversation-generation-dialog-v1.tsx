import React from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";

/** The exact V1 generation action currently being edited by the dialog.
 * 对话框当前编辑的精确 V1 generation 动作。
 */
export type ConversationGenerationDialogActionV1 =
  | {
      /** Replaces one root-input event in a new generation.
       * 在新 generation 中替换一条根输入事件。
       */
      kind: "edit";
      /** Original persisted root-input text.
       * 原始持久化根输入正文。
       */
      text: string;
    }
  | {
      /** Creates one immutable child generation.
       * 创建一个不可变子 generation。
       */
      kind: "fork";
      /** Initial child title derived from the current session title.
       * 从当前会话标题派生的初始子会话标题。
       */
      title: string;
    };

/** Props for the V1 root-input edit and fork dialog.
 * V1 根输入编辑与分叉对话框属性。
 */
export interface ConversationGenerationDialogV1Props {
  /** Active action, or null when the dialog is closed.
   * 活动作；对话框关闭时为 null。
   */
  action: ConversationGenerationDialogActionV1 | null;
  /** Whether the authoritative generation request is in flight.
   * 权威 generation 请求是否正在执行。
   */
  submitting: boolean;
  /** Closes the current action without changing durable state.
   * 关闭当前动作且不修改持久化状态。
   */
  onCancel: () => void;
  /** Submits the exact replacement text or child title.
   * 提交精确替换正文或子会话标题。
   */
  onConfirm: (value: string) => Promise<void> | void;
}

/** Renders one controlled dialog without legacy message identities.
 * 渲染一个不含旧消息身份的受控对话框。
 *
 * @param props current action and authoritative submit callback.
 *              当前动作与权威提交回调。
 * @returns the edit or fork dialog for the current root input.
 *          当前根输入对应的编辑或分叉对话框。
 */
export function ConversationGenerationDialogV1(
  props: ConversationGenerationDialogV1Props,
) {
  // t resolves the existing edit, fork, cancel, and replay labels.
  // t 解析既有编辑、分叉、取消与重新执行文案。
  const { t } = useI18n();
  // value is process-local until the generation service commits it.
  // value 在 generation 服务提交前仅存在于进程内。
  const [value, setValue] = React.useState("");

  React.useEffect(() => {
    if (props.action?.kind === "edit") {
      setValue(props.action.text);
    } else if (props.action?.kind === "fork") {
      setValue(props.action.title);
    } else {
      setValue("");
    }
  }, [props.action]);

  // confirmDisabled prevents blank fork titles while leaving attachment-only edits valid.
  // confirmDisabled 阻止空白分叉标题，同时保留仅附件编辑的合法性。
  const confirmDisabled = props.submitting
    || (props.action?.kind === "fork" && value.trim().length === 0);

  return (
    <Dialog open={props.action !== null} onOpenChange={(open) => {
      if (!open && !props.submitting) props.onCancel();
    }}>
      <DialogContent aria-describedby={undefined} showCloseButton={!props.submitting} size="sm">
        <DialogHeader>
          <DialogTitle>
            {props.action?.kind === "fork"
              ? t("conversation.action.fork")
              : t("conversation.action.editPrompt")}
          </DialogTitle>
        </DialogHeader>
        {props.action?.kind === "fork" ? (
          <Input
            aria-label={t("conversation.action.fork")}
            disabled={props.submitting}
            onChange={(event) => setValue(event.target.value)}
            value={value}
          />
        ) : (
          <textarea
            aria-label={t("conversation.userPrompt.edit")}
            className="conversation-generation-dialog-v1-textarea"
            disabled={props.submitting}
            onChange={(event) => setValue(event.target.value)}
            rows={8}
            value={value}
          />
        )}
        <DialogFooter>
          <Button
            disabled={props.submitting}
            onClick={props.onCancel}
            type="button"
            variant="outline"
          >
            {t("conversation.action.cancel")}
          </Button>
          <Button
            disabled={confirmDisabled}
            onClick={() => void props.onConfirm(
              props.action?.kind === "fork" ? value.trim() : value,
            )}
            type="button"
          >
            {props.action?.kind === "fork"
              ? t("conversation.action.fork")
              : t("conversation.action.saveAndReplay")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
