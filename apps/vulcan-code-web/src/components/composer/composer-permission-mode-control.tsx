import React from "react";
import { ChevronDown, CircleAlert, Hand, ShieldCheck, Sparkles } from "lucide-react";
import { useI18n } from "@/i18n";
import type { PermissionMode } from "@/service/permission-mode";
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
import { Checkbox } from "@/components/ui/checkbox";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { ComposerPermissionModeController } from "./composer-panel/use-composer-permission-mode";
import { PermissionJudgeEntryContext } from "./permission-judge-entry-context";

// PERMISSION_MODE_OPTIONS lists the modes from most to least supervised with their icon and copy keys.
// PERMISSION_MODE_OPTIONS 按监督程度从高到低列出各模式及其图标与文案键。
const PERMISSION_MODE_OPTIONS = [
  { value: "read_only", icon: Hand, label: "permissionMode.readOnly", hint: "permissionMode.readOnlyHint" },
  { value: "workspace_write", icon: ShieldCheck, label: "permissionMode.workspaceWrite", hint: "permissionMode.workspaceWriteHint" },
  { value: "full_access", icon: CircleAlert, label: "permissionMode.fullAccess", hint: "permissionMode.fullAccessHint" },
] as const;

/** Composer chip that shows and switches the permission mode of the current conversation or draft.
 * 显示并切换当前会话或草稿权限模式的输入区标签按钮。
 * `controller` owns persistence; switching to full access needs an acknowledged confirmation first.
 * `controller` 负责持久化；切换到完全权限前必须先勾选确认。 */
export function ComposerPermissionModeControl(props: { controller: ComposerPermissionModeController }) {
  const { t } = useI18n();
  const judge = React.useContext(PermissionJudgeEntryContext);
  const { mode, saving, available, select } = props.controller;
  // confirmingFullAccess holds the confirmation dialog open; acknowledged gates its action button.
  // confirmingFullAccess 控制确认框开启；acknowledged 控制其确认按钮可用。
  const [confirmingFullAccess, setConfirmingFullAccess] = React.useState(false);
  const [acknowledged, setAcknowledged] = React.useState(false);
  const current = PERMISSION_MODE_OPTIONS.find((option) => option.value === mode) ?? null;
  const CurrentIcon = current?.icon ?? ShieldCheck;
  const fullAccess = mode === "full_access";
  // The judge entry asks for setup while unset, shows its state when enabled and is hidden when off.
  // 判定入口在未设定时提示需要设定，启用时显示状态，关闭时隐藏。
  const judgeLabel = judge === null ? null : judge.state === null ? "permissionMode.judgeNeedsSetup"
    : judge.state === "enabled" ? "permissionMode.judgeEnabled" : null;

  // choose routes full access through the confirmation dialog and applies other modes directly.
  // choose 让完全权限经过确认框，其他模式直接应用。
  const choose = (next: PermissionMode) => {
    if (next === mode) return;
    if (next === "full_access") {
      setAcknowledged(false);
      setConfirmingFullAccess(true);
      return;
    }
    select(next);
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild disabled={!available}>
          <button
            aria-busy={saving}
            aria-label={t("permissionMode.label")}
            className="composer-select-trigger capability inline-flex items-center"
            data-permission-mode-trigger={mode ?? "loading"}
            type="button"
          >
            <span className="composer-select-icon" data-tone={fullAccess ? "warning" : undefined}>
              <CurrentIcon size={15} />
            </span>
            <span className={fullAccess ? "text-xs text-(--vcn-content-warning)" : "text-xs"}>
              {current ? t(current.label) : t("permissionMode.label")}
            </span>
            <ChevronDown aria-hidden="true" className="composer-select-chevron" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="composer-thinking-menu" side="top" width="content" aria-busy={saving}>
          <DropdownMenuLabel>{t("permissionMode.menuTitle")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={mode ?? ""}>
            {PERMISSION_MODE_OPTIONS.map((option) => {
              const Icon = option.icon;
              return (
                <DropdownMenuRadioItem
                  key={option.value}
                  value={option.value}
                  data-permission-mode={option.value}
                  disabled={saving}
                  onSelect={() => choose(option.value)}
                >
                  <span className="composer-select-icon" data-tone={option.value === "full_access" ? "warning" : undefined}>
                    <Icon size={15} />
                  </span>
                  <span className="composer-thinking-mode-label pr-5">
                    <span>{t(option.label)}</span>
                    <small>{t(option.hint)}</small>
                  </span>
                </DropdownMenuRadioItem>
              );
            })}
          </DropdownMenuRadioGroup>
          {judge !== null && judgeLabel !== null ? (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem data-judge-state={judge.state ?? "unset"} onSelect={judge.openSettings}>
                <span className="composer-select-icon" data-tone={judge.state === null ? "warning" : undefined}>
                  <Sparkles size={15} />
                </span>
                <span>{t(judgeLabel)}</span>
              </DropdownMenuItem>
            </>
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>
      <AlertDialog open={confirmingFullAccess} onOpenChange={setConfirmingFullAccess}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("permissionMode.fullAccessConfirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("permissionMode.fullAccessConfirmBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <label className="flex cursor-pointer items-center gap-2 text-sm">
            <Checkbox
              checked={acknowledged}
              data-full-access-acknowledge=""
              onCheckedChange={(checked) => setAcknowledged(checked === true)}
            />
            {t("permissionMode.fullAccessConfirmAcknowledge")}
          </label>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("permissionMode.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              data-full-access-confirm=""
              disabled={!acknowledged}
              onClick={() => select("full_access")}
            >
              {t("permissionMode.fullAccessConfirmAction")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
