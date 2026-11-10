import React from "react";
import { readComposerEnterSendMode, saveComposerEnterSendMode } from "@/components/composer/composer-send-preference";
import { useI18n } from "@/i18n";
import { ListEnd, ListStart } from "lucide-react";
import { SettingsPreviewChoices } from "./settings-preview-choices";
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

  return <SettingsPreviewChoices
    label={t("settings.composerEnterSend.label")}
    description={t("settings.composerEnterSend.description")}
    value={mode}
    onSelect={selectMode}
    options={[
      { value: "queue", label: t("settings.composerEnterSend.queue"), preview: <EnterSendExample insert={false} /> },
      { value: "insert", label: t("settings.composerEnterSend.insert"), preview: <EnterSendExample insert /> },
    ]}
  >
    {error && <p className="vk-field-error" role="alert">{error}</p>}
  </SettingsPreviewChoices>;
}

// Show where the newly sent message goes relative to the running task.
// 展示新发送的消息相对于运行中任务的位置。
function EnterSendExample({ insert }: { insert: boolean }) {
  // Both previews share the same two sample labels so their ordering is comparable.
  // 两个预览共用两条标签，便于比较顺序。
  const { t } = useI18n();
  return <span className="session-defaults-scene" data-short="">
    {insert ? null : <span className="session-defaults-mini-row">{t("settings.preview.currentTask")}</span>}
    <span className="session-defaults-mini-row" data-active="">
      {insert ? <ListStart size={14} /> : <ListEnd size={14} />}{t("settings.preview.newMessage")}
    </span>
    {insert ? <span className="session-defaults-mini-row">{t("settings.preview.currentTask")}</span> : null}
  </span>;
}
