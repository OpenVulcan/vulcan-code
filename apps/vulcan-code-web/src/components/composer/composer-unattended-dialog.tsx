import React from "react";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { UnattendedDefault } from "@/service/permission-mode-service";
import type { ComposerPermissionModeController } from "./composer-panel/use-composer-permission-mode";

// SOURCE_KEYS names where the prefilled deny policy came from.
// SOURCE_KEYS 说明预填拒绝策略的来源。
const SOURCE_KEYS: Record<UnattendedDefault["source"], TranslationKey> = {
  project: "permissionMode.unattended.sourceProject",
  system: "permissionMode.unattended.sourceSystem",
  none: "permissionMode.unattended.sourceNone",
};

/** Dialog that turns unattended mode on with a deny policy the user confirms, edits it, or turns
 * it off. The policy is prefilled from the project default, else the system default; nothing is
 * applied until the user confirms.
 * 以用户确认的拒绝策略开启无人值守模式、编辑该策略或关闭该模式的对话框。策略预填自项目默认值，
 * 否则为系统默认值；用户确认之前不会应用任何内容。 */
export function ComposerUnattendedDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  controller: ComposerPermissionModeController["unattended"];
}) {
  const { t } = useI18n();
  const { controller } = props;
  const enabled = controller.value !== null;
  const fieldId = React.useId();
  const [text, setText] = React.useState("");
  const [source, setSource] = React.useState<UnattendedDefault["source"] | null>(null);
  const [maxChars, setMaxChars] = React.useState<number | undefined>(undefined);
  const [loadFailed, setLoadFailed] = React.useState(false);
  const loadDefault = controller.loadDefault;
  const current = controller.value?.deny_policy ?? null;

  // Opening resets the field: the session's own policy while on, else the layered default.
  // 打开时重置输入框：开启时为会话自身的策略，否则为分层默认值。
  React.useEffect(() => {
    if (!props.open) return;
    let cancelled = false;
    setLoadFailed(false);
    setSource(null);
    if (current !== null) setText(current);
    else setText("");
    void loadDefault().then((loaded) => {
      if (cancelled || loaded === null) return;
      setMaxChars(loaded.maxChars);
      if (current === null) {
        setText(loaded.denyPolicy);
        setSource(loaded.source);
      }
    }).catch(() => {
      if (!cancelled) setLoadFailed(true);
    });
    return () => { cancelled = true; };
  }, [props.open, current, loadDefault]);

  const apply = async (next: { deny_policy: string } | null) => {
    if (await controller.apply(next)) props.onOpenChange(false);
  };
  const tooLong = maxChars !== undefined && [...text].length > maxChars;

  return (
    <AlertDialog open={props.open} onOpenChange={props.onOpenChange}>
      <AlertDialogContent size="md" data-unattended-dialog="">
        <AlertDialogHeader className="place-items-start text-left">
          <AlertDialogTitle>{t(enabled ? "permissionMode.unattended.editTitle" : "permissionMode.unattended.enableTitle")}</AlertDialogTitle>
          <AlertDialogDescription>{t("permissionMode.unattended.body")}</AlertDialogDescription>
        </AlertDialogHeader>
        <div className="flex flex-col gap-1.5 text-sm">
          <div className="flex items-baseline justify-between gap-2">
            <label className="font-medium" htmlFor={fieldId}>{t("permissionMode.unattended.policyLabel")}</label>
            {source !== null && source !== "none" ? <span className="text-xs text-muted-foreground" data-unattended-source={source}>
              {t(SOURCE_KEYS[source])}
            </span> : null}
          </div>
          <Textarea
            id={fieldId}
            data-unattended-policy=""
            maxLength={maxChars}
            placeholder={t("permissionMode.unattended.placeholder")}
            rows={4}
            value={text}
            onChange={(event) => setText(event.target.value)}
          />
          <div className="flex items-baseline justify-between gap-2 text-xs">
            <span className="text-(--vcn-content-warning)">{loadFailed ? t("permissionMode.unattended.loadFailed") : null}</span>
            {maxChars !== undefined ? <span className="text-muted-foreground tabular-nums">
              {formatMessage(t("permissionMode.unattended.counter"), { count: [...text].length, max: maxChars })}
            </span> : null}
          </div>
        </div>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("permissionMode.cancel")}</AlertDialogCancel>
          {enabled ? <Button type="button" variant="outline" data-unattended-disable="" disabled={controller.saving}
            onClick={() => void apply(null)}>
            {t("permissionMode.unattended.disable")}
          </Button> : null}
          <Button type="button" data-unattended-confirm="" disabled={controller.saving || tooLong}
            onClick={() => void apply({ deny_policy: text.trim() })}>
            {t(enabled ? "permissionMode.unattended.update" : "permissionMode.unattended.enable")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
