import React from "react";
import { Switch } from "@/components/ui/switch";
import { readElectronLocalBrowserBridge } from "@/shell/electronBridge";
import { readConversationLinkPreference, saveConversationLinkPreference } from "@/components/conversation/conversation-link-preference";
import { useI18n } from "@/i18n";

// ConversationLinkSettingsSection exposes the GUI-only preference in system basics.
// ConversationLinkSettingsSection 在系统基础设置中提供仅 GUI 生效的偏好。
// Takes no props; returns a persisted switch or nothing for clients without the native browser.
// 无参数；返回持久化开关，不具备原生浏览器的客户端不显示此设置。
export function ConversationLinkSettingsSection() {
  // locale selects the visible setting labels.
  // locale 选择设置文案。
  const { locale } = useI18n();
  // enabled reflects the last successfully persisted choice or the enabled product default.
  // enabled 反映最后成功保存的选择，或默认开启的产品行为。
  const [enabled, setEnabled] = React.useState(readConversationLinkPreference);
  // error exposes persistence failures without changing the displayed choice.
  // error 明确展示持久化失败，不改变已显示的选择。
  const [error, setError] = React.useState("");
  if (!readElectronLocalBrowserBridge()) return null;
  return <section className="settings-form-grid mt-6">
    <label className="flex items-center justify-between gap-4">
      <span>{locale === "zh-CN" ? "用内置浏览器打开链接" : "Open links in the built-in browser"}</span>
      <Switch checked={enabled} onCheckedChange={(checked) => {
        try {
          saveConversationLinkPreference(checked);
          setEnabled(checked);
          setError("");
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      }} />
    </label>
    <p className="settings-muted-line">{locale === "zh-CN"
      ? "开启后，点击对话里的网页链接会在右侧内置浏览器打开；关闭则用系统默认浏览器。仅桌面版可用。"
      : "When on, web links in conversations open in the built-in browser on the right; when off they open in your default browser. Desktop app only."}</p>
    {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
  </section>;
}
