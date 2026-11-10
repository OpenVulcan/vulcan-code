import React from "react";
import type { ComposerSendMode } from "@/components/composer/composer-send-preference";
import { toast } from "@/ui-kit/primitives/toast";
import { composerAttachmentOverflowCount } from "@/components/composer/composer-image-compression-preference";
import type { ComposerAttachmentPreparationController } from "@/components/composer/composer-panel/use-composer-attachment-preparation";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { useI18n } from "@/i18n";

// useComposerAttachmentSubmissionGate decides whether the current attachments may go out with a user prompt.
// useComposerAttachmentSubmissionGate 决定当前附件能否随用户提示词发出。
// Parameters: preparation is the live draft plan and send submits the draft after an explicit overflow confirmation.
// 参数：preparation 是实时草稿规划，send 在明确确认超预算后提交草稿。
// Returns: admit, which is true only when sending may proceed now, and the confirmation dialog to render.
// 返回值：admit 仅在可以立即发送时为 true，以及需要渲染的确认对话框。
export function useComposerAttachmentSubmissionGate(
  preparation: ComposerAttachmentPreparationController,
  send: (sendMode: ComposerSendMode) => Promise<void>,
): { admit: (sendMode?: ComposerSendMode) => boolean; dialog: React.ReactNode } {
  const { t } = useI18n();
  // overflow holds the attachment count and original send action across confirmation.
  // overflow 在确认期间保留附件数量及原始发送操作。
  const [overflow, setOverflow] = React.useState<{ count: number; sendMode: ComposerSendMode } | null>(null);
  // admit checks preparation for sendMode and reports whether it can proceed now.
  // admit 检查 sendMode 的准备情况，并返回是否可以立即提交。
  const admit = React.useCallback((sendMode: ComposerSendMode = "queue") => {
    if (preparation.blocksSubmission) {
      toast.info(t("composer.attachment.preparationBlocked").replace("{count}", String(preparation.blockingCount)));
      return false;
    }
    // Images beyond the single-request budget would go out as addresses only, so the user decides first.
    // 超出单次请求预算的图片只会以地址发送，因此先由用户决定。
    const count = composerAttachmentOverflowCount(preparation.preparations);
    if (count > 0) {
      setOverflow({ count, sendMode });
      return false;
    }
    return true;
  }, [preparation.blockingCount, preparation.blocksSubmission, preparation.preparations, t]);
  const dialog = (
    <AlertDialog open={overflow !== null} onOpenChange={(open) => { if (!open) setOverflow(null); }}>
      <AlertDialogContent size="md">
        <AlertDialogHeader>
          <AlertDialogTitle>{t("composer.attachment.overflowTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("composer.attachment.overflowDescription").replace("{count}", String(overflow?.count ?? 0))}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("composer.attachment.overflowCancel")}</AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              if (!overflow) return;
              setOverflow(null);
              void send(overflow.sendMode);
            }}
          >
            {t("composer.attachment.overflowConfirm")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
  return { admit, dialog };
}
