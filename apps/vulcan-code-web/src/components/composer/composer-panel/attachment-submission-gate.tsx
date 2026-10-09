import React from "react";
import { toast } from "sonner";
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
} from "@/components/ui/alert-dialog";
import { useI18n } from "@/i18n";

// useComposerAttachmentSubmissionGate decides whether the current attachments may go out with a user prompt.
// useComposerAttachmentSubmissionGate 决定当前附件能否随用户提示词发出。
// Parameters: preparation is the live draft plan and send submits the draft after an explicit overflow confirmation.
// 参数：preparation 是实时草稿规划，send 在明确确认超预算后提交草稿。
// Returns: admit, which is true only when sending may proceed now, and the confirmation dialog to render.
// 返回值：admit 仅在可以立即发送时为 true，以及需要渲染的确认对话框。
export function useComposerAttachmentSubmissionGate(
  preparation: ComposerAttachmentPreparationController,
  send: () => Promise<void>,
): { admit: () => boolean; dialog: React.ReactNode } {
  const { t } = useI18n();
  // overflowCount holds the over-budget attachment count while the user decides.
  // overflowCount 在用户决定期间保存超出预算的附件数量。
  const [overflowCount, setOverflowCount] = React.useState<number | null>(null);
  const admit = React.useCallback(() => {
    if (preparation.blocksSubmission) {
      toast.info(t("composer.attachment.preparationBlocked").replace("{count}", String(preparation.blockingCount)));
      return false;
    }
    // Images beyond the single-request budget would go out as addresses only, so the user decides first.
    // 超出单次请求预算的图片只会以地址发送，因此先由用户决定。
    const count = composerAttachmentOverflowCount(preparation.preparations);
    if (count > 0) {
      setOverflowCount(count);
      return false;
    }
    return true;
  }, [preparation.blockingCount, preparation.blocksSubmission, preparation.preparations, t]);
  const dialog = (
    <AlertDialog open={overflowCount !== null} onOpenChange={(open) => { if (!open) setOverflowCount(null); }}>
      <AlertDialogContent size="md">
        <AlertDialogHeader>
          <AlertDialogTitle>{t("composer.attachment.overflowTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("composer.attachment.overflowDescription").replace("{count}", String(overflowCount ?? 0))}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("composer.attachment.overflowCancel")}</AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              setOverflowCount(null);
              void send();
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
