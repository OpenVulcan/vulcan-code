import React from "react";
import { Bot, ChevronDown, CircleAlert, Hand, ShieldCheck, Sparkles } from "lucide-react";
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
import { ComposerUnattendedDialog } from "./composer-unattended-dialog";
import { PermissionJudgeEntryContext } from "./permission-judge-entry-context";
import { useDedicatedComposer } from "@/components/composer/dedicated/dedicated-session-controls";

// PERMISSION_MODE_OPTIONS lists the modes from most to least supervised with their icon and copy keys.
// PERMISSION_MODE_OPTIONS 按监督程度从高到低列出各模式及其图标与文案键。
const PERMISSION_MODE_OPTIONS = [
  { value: "read_only", icon: Hand, label: "permissionMode.readOnly", hint: "permissionMode.readOnlyHint" },
  { value: "workspace_write", icon: ShieldCheck, label: "permissionMode.workspaceWrite", hint: "permissionMode.workspaceWriteHint" },
  { value: "auto_approve", icon: Sparkles, label: "permissionMode.autoApprove", hint: "permissionMode.autoApproveHint" },
  { value: "full_access", icon: CircleAlert, label: "permissionMode.fullAccess", hint: "permissionMode.fullAccessHint" },
] as const;

/** Composer chip that shows and switches the permission mode of the current conversation or draft.
 * 显示并切换当前会话或草稿权限模式的输入区标签按钮。
 * `controller` owns persistence; switching to full access needs an acknowledged confirmation first.
 * Auto-approve needs a configured judge: until then it is greyed out and opens the judge settings.
 * Unattended mode is offered only under auto-approve, the one mode with a reviewer.
 * `controller` 负责持久化；切换到完全权限前必须先勾选确认。自动审批需要已配置的判定器：在此之前显示为
 * 灰色，点击打开判定设置。无人值守只在自动审批下提供，因为只有该模式有复核器。 */
export function ComposerPermissionModeControl(props: { controller: ComposerPermissionModeController }) {
  // A dedicated session without tools never asks for permissions, so the chip has nothing to switch.
  // 无工具的专职会话从不请求权限，因此该标签没有可切换的内容。
  const dedicated = useDedicatedComposer();
  if (dedicated?.toolAccess === "none") return null;
  return <PermissionModeChip controller={props.controller} />;
}

/** Permission-mode chip body for sessions that can call tools.
 * 可调用工具的会话所用的权限模式标签主体。 */
function PermissionModeChip(props: { controller: ComposerPermissionModeController }) {
  const { t } = useI18n();
  const judge = React.useContext(PermissionJudgeEntryContext);
  const { mode, saving, available, select } = props.controller;
  // confirmingFullAccess holds the confirmation dialog open; acknowledged gates its action button.
  // confirmingFullAccess 控制确认框开启；acknowledged 控制其确认按钮可用。
  const [confirmingFullAccess, setConfirmingFullAccess] = React.useState(false);
  const [acknowledged, setAcknowledged] = React.useState(false);
  // editingUnattended holds the unattended dialog open.
  // editingUnattended 控制无人值守对话框开启。
  const [editingUnattended, setEditingUnattended] = React.useState(false);
  const unattended = props.controller.unattended;
  const unattendedOn = unattended.value !== null;
  const current = PERMISSION_MODE_OPTIONS.find((option) => option.value === mode) ?? null;
  const CurrentIcon = current?.icon ?? ShieldCheck;
  const fullAccess = mode === "full_access";
  // Outside the workbench layout there is no judge to read, so auto-approve is not gated there.
  // 在工作台布局之外无法读取判定器，因此不对自动审批设限。
  const needsSetup = judge !== null && judge.backend === null;

  // choose opens the judge settings for an unconfigured auto-approve, routes full access through
  // the confirmation dialog and applies other modes directly.
  // choose 在自动审批未配置时打开判定设置，让完全权限经过确认框，其他模式直接应用。
  const choose = (next: PermissionMode) => {
    if (next === "auto_approve" && needsSetup) {
      judge?.openSettings();
      return;
    }
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
            data-unattended={unattendedOn ? "" : undefined}
            type="button"
          >
            <span className="composer-select-icon" data-tone={fullAccess ? "warning" : undefined}>
              <CurrentIcon size={15} />
            </span>
            <span className={fullAccess ? "text-xs text-(--vcn-content-warning)" : "text-xs"}>
              {current ? t(current.label) : t("permissionMode.label")}
            </span>
            {unattendedOn ? <span className="inline-flex items-center gap-0.5 text-xs text-muted-foreground" data-unattended-chip="">
              <Bot aria-hidden="true" size={13} />{t("permissionMode.unattended.chip")}
            </span> : null}
            <ChevronDown aria-hidden="true" className="composer-select-chevron" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="top" width="lg" aria-busy={saving}>
          <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">{t("permissionMode.menuTitle")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={mode ?? ""}>
            {PERMISSION_MODE_OPTIONS.map((option) => {
              const Icon = option.icon;
              const greyed = option.value === "auto_approve" && needsSetup;
              return (
                <DropdownMenuRadioItem
                  key={option.value}
                  className={greyed ? "items-start text-xs text-muted-foreground" : "items-start text-xs"}
                  value={option.value}
                  data-permission-mode={option.value}
                  data-needs-setup={greyed ? "" : undefined}
                  disabled={saving}
                  onSelect={() => choose(option.value)}
                >
                  <span className="composer-select-icon mt-0.5" data-tone={option.value === "full_access" ? "warning" : undefined}>
                    <Icon size={14} />
                  </span>
                  <span className="flex min-w-0 flex-1 flex-col items-start gap-0.5 text-left">
                    <span className={option.value === "full_access" ? "text-(--vcn-content-warning)" : undefined}>{t(option.label)}</span>
                    <span className="text-[11px] leading-snug font-normal text-muted-foreground">
                      {t(greyed ? "permissionMode.autoApproveNeedsSetup" : option.hint)}
                    </span>
                  </span>
                </DropdownMenuRadioItem>
              );
            })}
          </DropdownMenuRadioGroup>
          {mode === "auto_approve" ? (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="items-start text-xs"
                data-unattended-entry={unattendedOn ? "on" : "off"}
                disabled={!unattended.available || unattended.saving}
                onSelect={() => setEditingUnattended(true)}
              >
                <span className="composer-select-icon mt-0.5"><Bot size={14} /></span>
                <span className="flex min-w-0 flex-1 flex-col items-start gap-0.5 text-left">
                  <span>{t(unattendedOn ? "permissionMode.unattended.menuOn" : "permissionMode.unattended.menuOff")}</span>
                  <span className="text-[11px] leading-snug font-normal text-muted-foreground">
                    {t(unattended.available ? "permissionMode.unattended.menuHint" : "permissionMode.unattended.draftHint")}
                  </span>
                </span>
              </DropdownMenuItem>
            </>
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>
      <ComposerUnattendedDialog open={editingUnattended} onOpenChange={setEditingUnattended} controller={unattended} />
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
