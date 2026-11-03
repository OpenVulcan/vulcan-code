import * as React from "react";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { useI18n } from "@/i18n";
import { OUTSIDE_FILE_ACCESS_OPTIONS } from "@/components/file-windows/outside-file-access-options";
import { errorMessage } from "@/lib/display-format";
import { saveOutsideWorkspaceFileAccess } from "@/service/outside-file-access-service";
import type { OutsideWorkspaceFileAccess } from "@/types";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import "@/ui-kit/styles/features/settings-forms.css";

// OutsideFileAccessSettingsSection picks how sessions without their own choice open files outside the workspace. The
// saved value returns through the workbench the service publishes.
// OutsideFileAccessSettingsSection 选择未自行选择的会话如何打开工作区之外的文件。保存的值经服务发布的工作台返回。
// Parameters: `access` is the saved system default.
// 参数：`access` 是已保存的系统默认值。
export function OutsideFileAccessSettingsSection(props: { access: OutsideWorkspaceFileAccess }) {
  const { t } = useI18n();
  const [pending, setPending] = React.useState<OutsideWorkspaceFileAccess | null>(null);
  const [error, setError] = React.useState("");
  const save = (next: OutsideWorkspaceFileAccess) => {
    if (pending || next === props.access) return;
    setPending(next);
    setError("");
    saveOutsideWorkspaceFileAccess(next).catch((cause: unknown) => setError(errorMessage(cause))).finally(() => setPending(null));
  };
  return (
    <section className="vk-form-pref" data-outside-file-access-settings>
      <div className="vk-form-pref-text">
        <strong>{t("settings.outsideFiles.title")}</strong>
        <p className="vk-form-note">{t("settings.outsideFiles.hint")}</p>
      </div>
      <SegmentedControl
        aria-label={t("settings.outsideFiles.title")}
        disabled={pending !== null}
        onValueChange={(value) => save(value as OutsideWorkspaceFileAccess)}
        value={pending ?? props.access}
      >
        {OUTSIDE_FILE_ACCESS_OPTIONS.map((option) => (
          <SegmentedControlItem key={option.value} value={option.value}>{t(option.labelKey)}</SegmentedControlItem>
        ))}
      </SegmentedControl>
      {error ? <SettingsErrorNotice detail={error} message={t("settings.outsideFiles.saveFailed")} /> : null}
    </section>
  );
}
