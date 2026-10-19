import React from "react";
import { Switch } from "@/ui-kit/primitives/switch";
import { ToolCollapsePreferencesContext } from "@/components/conversation/tool-collapse-preferences";
import { useI18n } from "@/i18n";

/** Renders the shared default independently of per-session choices; takes no props and returns a save-aware switch.
 * 独立于会话选择渲染共享默认值；无参数，返回具有保存状态的开关。
 */
export function ToolCollapseSettingsSection() {
  // preferences contains the authoritative default and its service-backed save operation.
  // preferences 包含权威默认值及其服务保存操作。
  const preferences = React.useContext(ToolCollapsePreferencesContext);
  // t selects user-facing copy.
  // t 选择面向用户的文案。
  const { t } = useI18n();
  // pending keeps the selected value visible while preventing concurrent writes.
  // pending 在阻止并发写入时保持所选值可见。
  const [pending, setPending] = React.useState<boolean | null>(null);
  // savingRef closes the synchronous double-click window.
  // savingRef 关闭同步双击窗口。
  const savingRef = React.useRef(false);
  // error retains explicit persistence failures instead of pretending a default was saved.
  // error 保留明确的持久化失败，不伪装成已保存默认值。
  const [error, setError] = React.useState("");
  /** Saves enabled explicitly, preserving the previous default on failure; resolves when the request settles.
   * 明确保存 enabled，失败时保留原默认值；请求结束后完成。
   */
  const save = async (enabled: boolean): Promise<void> => {
    if (savingRef.current || !preferences.saveDefault) return;
    savingRef.current = true;
    setPending(enabled);
    setError("");
    try {
      await preferences.saveDefault(enabled);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      savingRef.current = false;
      setPending(null);
    }
  };
  return <section className="settings-form-grid mt-6">
    <label className="flex items-center justify-between gap-4">
      <span>{t("settings.toolCollapse.collapseToolCallsBy")}</span>
      <Switch
        checked={pending ?? preferences.defaultEnabled}
        disabled={pending !== null || !preferences.saveDefault}
        onCheckedChange={(enabled) => { void save(enabled); }}
      />
    </label>
    <p className="settings-muted-line">{t("settings.toolCollapse.whenOnToolCalls")}</p>
    {pending !== null ? <p role="status" className="settings-muted-line">{t("settings.toolCollapse.saving")}</p> : null}
    {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
  </section>;
}
