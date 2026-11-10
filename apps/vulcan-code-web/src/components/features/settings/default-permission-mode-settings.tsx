import React from "react";
import { FullAccessConfirmation } from "@/components/composer/full-access-confirmation";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { DEFAULT_PERMISSION_MODE, type PermissionMode } from "@/service/permission-mode";
import { saveDefaultPermissionMode } from "@/service/permission-mode-service";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import "@/ui-kit/styles/features/settings-forms.css";

// OPTIONS lists the allowed system defaults and the corresponding existing mode labels.
// OPTIONS 列出允许的系统默认权限及其既有模式文案。
const OPTIONS = [
  { value: "workspace_write", label: "permissionMode.workspaceWrite" },
  { value: "auto_approve", label: "permissionMode.autoApprove" },
  { value: "full_access", label: "permissionMode.fullAccess" },
] as const;

// DefaultPermissionModeSettingsSection renders a service-backed default for future sessions.
// DefaultPermissionModeSettingsSection 渲染由服务保存的未来新会话默认权限。
// mode is the last published choice; judgeConfigured governs whether automatic approval is available.
// mode 为最后发布的选择，judgeConfigured 决定自动审批是否可用。
export function DefaultPermissionModeSettingsSection(props: {
  mode?: PermissionMode;
  judgeConfigured: boolean;
}) {
  // t resolves existing permission labels and setting-specific guidance.
  // t 解析既有权限文案和此设置的专属说明。
  const { t } = useI18n();
  // mode is the saved choice, including a retained automatic choice whose judge was removed.
  // mode 为已保存的选择，包括判定器被移除后仍保留的自动审批选择。
  const mode = props.mode ?? DEFAULT_PERMISSION_MODE;
  // pending marks a save, error retains its failure, and confirming controls the full access dialog.
  // pending 标记保存中，error 保存失败信息，confirming 控制完全权限对话框。
  const [pending, setPending] = React.useState<PermissionMode | null>(null);
  const [error, setError] = React.useState("");
  const [confirming, setConfirming] = React.useState(false);

  // save writes next only after explicit selection and settles after the service response.
  // save 只在显式选择后写入 next，服务响应后结束。
  const save = async (next: typeof OPTIONS[number]["value"]): Promise<void> => {
    setPending(next);
    setError("");
    try {
      await saveDefaultPermissionMode(next);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(null);
    }
  };

  // select accepts a displayed value and requests confirmation when it grants full access.
  // select 接收界面选项，若授予完全权限则请求确认。
  const select = (value: string): void => {
    if (value === mode || pending) return;
    if (value === "full_access") setConfirming(true);
    else if (value === "workspace_write" || (value === "auto_approve" && props.judgeConfigured)) void save(value);
  };

  return <section className="vk-form-pref">
    <div className="vk-form-pref-text">
      <strong>{t("settings.defaultPermissionMode.title")}</strong>
      <p className="vk-form-note">{t("settings.defaultPermissionMode.hint")}</p>
      {!props.judgeConfigured && <p className="vk-form-note">{t(mode === "auto_approve"
        ? "settings.defaultPermissionMode.autoApproveUnavailable"
        : "permissionMode.autoApproveNeedsSetup")}</p>}
    </div>
    <SegmentedControl aria-label={t("settings.defaultPermissionMode.title")} disabled={pending !== null} onValueChange={select} value={pending ?? mode}>
      {OPTIONS.map((option) => <SegmentedControlItem disabled={option.value === "auto_approve" && !props.judgeConfigured} key={option.value} value={option.value}>
        {t(option.label)}
      </SegmentedControlItem>)}
    </SegmentedControl>
    {error && <SettingsErrorNotice detail={error} message={t("permissionMode.saveFailed")} />}
    <FullAccessConfirmation open={confirming} systemDefault onOpenChange={setConfirming} onConfirm={() => void save("full_access")} />
  </section>;
}
