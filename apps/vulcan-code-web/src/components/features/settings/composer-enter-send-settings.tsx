import React from "react";
import { readComposerEnterSendMode, saveComposerEnterSendMode } from "@/components/composer/composer-send-preference";
import { useI18n } from "@/i18n";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import "@/ui-kit/styles/features/settings-forms.css";

// ComposerEnterSendSettingsSection takes no props and renders a selector for the saved Enter action.
// ComposerEnterSendSettingsSection 无参数，渲染可保存 Enter 操作的下拉选择器。
export function ComposerEnterSendSettingsSection() {
  // t resolves the setting labels in the current locale.
  // t 解析当前语言中的设置文案。
  const { t } = useI18n();
  // mode reflects the last successful choice; error exposes a storage failure.
  // mode 反映上次成功的选择；error 展示存储失败。
  const [mode, setMode] = React.useState(readComposerEnterSendMode);
  const [error, setError] = React.useState("");
  // selectMode accepts the selector value and returns after saving it or displaying the error.
  // selectMode 接收选择器的值，保存成功或展示错误后返回。
  const selectMode = (value: string): void => {
    if (value !== "queue" && value !== "insert") return;
    try {
      saveComposerEnterSendMode(value);
      setMode(value);
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return <section className="vk-form-pref">
    <div className="vk-form-pref-text">
      <strong>{t("settings.composerEnterSend.label")}</strong>
      <p className="vk-form-note">{t("settings.composerEnterSend.description")}</p>
    </div>
    <Select onValueChange={selectMode} value={mode}>
      <SelectTrigger aria-label={t("settings.composerEnterSend.label")}><SelectValue /></SelectTrigger>
      <SelectContent>
        <SelectItem value="queue">{t("settings.composerEnterSend.queue")}</SelectItem>
        <SelectItem value="insert">{t("settings.composerEnterSend.insert")}</SelectItem>
      </SelectContent>
    </Select>
    {error && <p className="vk-field-error" role="alert">{error}</p>}
  </section>;
}
