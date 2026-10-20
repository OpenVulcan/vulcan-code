import React from "react";
import { Switch } from "@/ui-kit/primitives/switch";
import { readElectronLocalBrowserBridge } from "@/host-shell/electron-bridge";
import { readConversationLinkPreference, saveConversationLinkPreference } from "@/components/conversation/conversation-link-preference";
import { useI18n } from "@/i18n";
import "@/ui-kit/styles/features/settings-forms.css"

// ConversationLinkSettingsSection exposes the GUI-only preference in system basics.
// ConversationLinkSettingsSection 在系统基础设置中提供仅 GUI 生效的偏好。
// Takes no props; returns a persisted switch or nothing for clients without the native browser.
// 无参数；返回持久化开关，不具备原生浏览器的客户端不显示此设置。
export function ConversationLinkSettingsSection() {
  // t selects the visible setting labels.
  // t 选择设置文案。
  const { t } = useI18n();
  // enabled reflects the last successfully persisted choice or the enabled product default.
  // enabled 反映最后成功保存的选择，或默认开启的产品行为。
  const [enabled, setEnabled] = React.useState(readConversationLinkPreference);
  // error exposes persistence failures without changing the displayed choice.
  // error 明确展示持久化失败，不改变已显示的选择。
  const [error, setError] = React.useState("");
  if (!readElectronLocalBrowserBridge()) return null;
  return <section className="vk-form-pref">
    <div className="vk-form-pref-text">
      <strong>{t("settings.conversationLinks.openLinksInThe")}</strong>
      <p className="vk-form-note">{t("settings.conversationLinks.whenOnWebLinks")}</p>
    </div>
      <Switch
 aria-label={t("settings.conversationLinks.openLinksInThe")}
 checked={enabled} onCheckedChange={(checked) => {
        try {
          saveConversationLinkPreference(checked);
          setEnabled(checked);
          setError("");
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      }} />
    {error && <p role="alert" className="vk-field-error">{error}</p>}
  </section>;
}
