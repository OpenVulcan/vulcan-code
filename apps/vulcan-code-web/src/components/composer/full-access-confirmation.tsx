import React from "react";
import { useI18n } from "@/i18n";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { Checkbox } from "@/ui-kit/primitives/checkbox";

// FullAccessConfirmation requires acknowledgement before enabling full access.
// FullAccessConfirmation 要求用户勾选确认后才能启用完全权限。
// open controls visibility, systemDefault identifies the scope, and onConfirm applies the choice.
// open 控制可见性，systemDefault 标识作用域，onConfirm 应用选择。
export function FullAccessConfirmation(props: {
  open: boolean;
  systemDefault?: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  // t resolves the warning and action labels.
  // t 解析提示和操作文案。
  const { t } = useI18n();
  // acknowledged applies only to the current confirmation.
  // acknowledged 只对本次确认有效。
  const [acknowledged, setAcknowledged] = React.useState(false);
  return (
    <AlertDialog open={props.open} onOpenChange={(open) => {
      if (!open) setAcknowledged(false);
      props.onOpenChange(open);
    }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("permissionMode.fullAccessConfirmTitle")}</AlertDialogTitle>
          <AlertDialogDescription>{t(props.systemDefault
            ? "settings.defaultPermissionMode.fullAccessConfirmBody"
            : "permissionMode.fullAccessConfirmBody")}</AlertDialogDescription>
        </AlertDialogHeader>
        <label className="vk-check-label">
          <Checkbox
            checked={acknowledged}
            data-full-access-acknowledge=""
            onCheckedChange={(checked) => setAcknowledged(checked === true)}
          />
          {t("permissionMode.fullAccessConfirmAcknowledge")}
        </label>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("permissionMode.cancel")}</AlertDialogCancel>
          <AlertDialogAction data-full-access-confirm="" disabled={!acknowledged} onClick={props.onConfirm}>
            {t("permissionMode.fullAccessConfirmAction")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
