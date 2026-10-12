import React from "react";
import { Bot, ChevronDown, CircleAlert, Hand, ShieldCheck, SlidersHorizontal, Sparkles } from "lucide-react";
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
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { ComposerPermissionModeController } from "./composer-panel/use-composer-permission-mode";
import { ComposerUnattendedDialog } from "./composer-unattended-dialog";
import { PermissionJudgeEntryContext } from "./permission-judge-entry-context";
import { useDedicatedComposer } from "@/components/composer/dedicated/dedicated-session-controls";
import { OPTION_KEYS, label as codeLabel } from "@/components/features/settings/permission-judge-labels";
import { FOLLOW_DEFAULT, useProjectPermissionJudgePolicy } from "@/components/project/project-permission-judge-section";
import type { ProjectPermissionJudgePolicySnapshot } from "@/types";

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
 * Unattended mode and the project's judge categories live in the auto-approve settings submenu,
 * shown only under auto-approve, the one mode that consults the judge and the reviewer.
 * `controller` 负责持久化；切换到完全权限前必须先勾选确认。自动审批需要已配置的判定器：在此之前显示为
 * 灰色，点击打开判定设置。无人值守与项目的判定类别位于自动审批设置子菜单，只在自动审批下显示，因为只有该
 * 模式会使用判定器与复核器。
 * `projectId` is the conversation's project; without one the categories are not shown.
 * `projectId` 是会话所属项目；没有项目时不显示类别。 */
export function ComposerPermissionModeControl(props: { controller: ComposerPermissionModeController; projectId?: string | null }) {
  // A dedicated session without tools never asks for permissions, so the chip has nothing to switch.
  // 无工具的专职会话从不请求权限，因此该标签没有可切换的内容。
  const dedicated = useDedicatedComposer();
  if (dedicated?.toolAccess === "none") return null;
  return <PermissionModeChip controller={props.controller} projectId={props.projectId ?? null} />;
}

// quickCategories keeps the categories that are a plain allow-or-not choice, which a checkbox can
// express; any other shape stays in the settings page.
// quickCategories 保留可以用勾选表达的“允许或不允许”类别；其他形态只在设置页中编辑。
function quickCategories(snapshot: ProjectPermissionJudgePolicySnapshot) {
  return snapshot.options.filter((option) => option.modes.length === 2 && option.modes.includes("allow"));
}

/** Checkbox rows of the project's judge categories: checked means the judge may allow that kind of
 * call. Each change saves at once; choosing the category's default follows the default again.
 * Mounted only while the auto-approve settings submenu is open, so every opening reads the current
 * choices.
 * 项目判定类别的勾选行：勾选表示判定器可以放行这类调用。每次修改立即保存；选中类别默认值时恢复跟随默认。
 * 只在自动审批设置子菜单打开时挂载，因此每次打开都会读取当前选择。 */
function PermissionCategoryItems(props: { projectId: string }) {
  const { t } = useI18n();
  const policy = useProjectPermissionJudgePolicy(props.projectId);
  const { snapshot } = policy;
  return (
    <>
      <DropdownMenuLabel className="flex flex-col gap-0.5 text-xs font-normal">
        <span className="text-foreground">{t("settings.permissionJudge.policy.title")}</span>
        <span className="text-[11px] leading-snug text-muted-foreground">{t("permissionMode.categoriesHint")}</span>
      </DropdownMenuLabel>
      {policy.loadError !== null ? (
        <DropdownMenuItem className="text-xs" disabled>{t("projectSettings.permissionJudge.loadFailed")}</DropdownMenuItem>
      ) : snapshot === null ? (
        <DropdownMenuItem className="text-xs" disabled>{t("ui.loading")}</DropdownMenuItem>
      ) : quickCategories(snapshot).map((option) => {
        const allowed = (option.chosenMode ?? option.defaultMode) === "allow";
        const toggle = (checked: boolean) => {
          const target = checked ? "allow" : option.modes.find((mode) => mode !== "allow")!;
          void policy.save(option.key, target === option.defaultMode ? FOLLOW_DEFAULT : target);
        };
        return (
          <DropdownMenuCheckboxItem
            key={option.key}
            checked={allowed}
            className="text-xs"
            data-judge-category={option.key}
            disabled={policy.saving !== null}
            onCheckedChange={(checked) => toggle(checked === true)}
            onSelect={(event) => event.preventDefault()}
          >
            {codeLabel(t, OPTION_KEYS, option.key)}
          </DropdownMenuCheckboxItem>
        );
      })}
    </>
  );
}

/** Permission-mode chip body for sessions that can call tools.
 * 可调用工具的会话所用的权限模式标签主体。 */
function PermissionModeChip(props: { controller: ComposerPermissionModeController; projectId: string | null }) {
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

  // choose opens the judge settings for an unconfigured auto-approve and routes full access through
  // the confirmation dialog; both leave the menu. Other modes apply in place and keep the menu open,
  // so the auto-approve settings appear right under the modes once the switch is confirmed.
  // choose 在自动审批未配置时打开判定设置，并让完全权限经过确认框；二者都会离开菜单。其他模式就地应用并
  // 保持菜单打开，切换确认后自动审批设置随即出现在模式下方。
  const choose = (next: PermissionMode, event: Event) => {
    if (next === "auto_approve" && needsSetup) {
      judge?.openSettings();
      return;
    }
    if (next === "full_access") {
      if (next === mode) return;
      setAcknowledged(false);
      setConfirmingFullAccess(true);
      return;
    }
    event.preventDefault();
    if (next !== mode) select(next);
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
          <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">{t("permissionMode.label")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={mode ?? ""}>
            {PERMISSION_MODE_OPTIONS.map((option) => {
              const Icon = option.icon;
              const greyed = option.value === "auto_approve" && needsSetup;
              const warning = option.value === "full_access";
              return (
                <DropdownMenuRadioItem
                  key={option.value}
                  className="items-start gap-2.5 py-2 text-xs"
                  value={option.value}
                  data-permission-mode={option.value}
                  data-needs-setup={greyed ? "" : undefined}
                  disabled={saving}
                  onSelect={(event) => choose(option.value, event)}
                >
                  <span className="composer-select-icon mt-0.5" data-tone={warning ? "warning" : undefined}>
                    <Icon size={14} />
                  </span>
                  <span className="flex min-w-0 flex-1 flex-col items-start gap-0.5 text-left">
                    <span className={greyed ? "text-muted-foreground" : warning ? "text-(--vcn-content-warning)" : undefined}>
                      {t(option.label)}
                    </span>
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
              <DropdownMenuSub>
                <DropdownMenuSubTrigger className="gap-2.5 text-xs" data-auto-approve-settings="">
                  <span className="composer-select-icon"><SlidersHorizontal size={14} /></span>
                  <span className="flex-1">{t("permissionMode.autoSettings")}</span>
                  {unattendedOn ? <span className="inline-flex items-center gap-0.5 text-[11px] text-muted-foreground">
                    <Bot aria-hidden="true" size={12} />{t("permissionMode.unattended.chip")}
                  </span> : null}
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent className="w-72" data-auto-approve-settings-menu="">
                  {props.projectId !== null && !needsSetup ? (
                    <>
                      <PermissionCategoryItems projectId={props.projectId} />
                      <DropdownMenuSeparator />
                    </>
                  ) : null}
                  <DropdownMenuItem
                    className="items-start gap-2.5 py-2 text-xs"
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
                </DropdownMenuSubContent>
              </DropdownMenuSub>
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
