import React from "react";
import { Switch } from "@/ui-kit/primitives/switch";
import { ThinkingDisplaySystemContext } from "@/components/conversation/thinking-display-preferences";
import { normalizeThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useI18n } from "@/i18n";

/** Renders the global popup default without changing session overrides.
 * 渲染全局弹窗默认值，不改变会话覆盖值。
 * Takes no props; returns a save-aware switch with explicit failure feedback.
 * 无参数；返回具有保存状态和明确失败反馈的开关。
 */
export function ThinkingPopupSettingsSection() {
  // preference owns the authoritative global mode and its save action.
  // preference 持有权威全局模式及保存操作。
  const preference = React.useContext(ThinkingDisplaySystemContext);
  // t selects system-settings copy.
  // t 选择系统设置文案。
  const { t } = useI18n();
  // pending preserves the selected switch state during persistence.
  // pending 在持久化期间保留所选开关状态。
  const [pending, setPending] = React.useState<boolean | null>(null);
  // savingRef rejects duplicate writes before React commits.
  // savingRef 在 React 提交前拒绝重复写入。
  const savingRef = React.useRef(false);
  // error never presents a failed save as the new default.
  // error 防止将失败的保存呈现为新默认值。
  const [error, setError] = React.useState("");
  /** Persists enabled as the established popup/inline mode; resolves when the authoritative save settles.
   * 将 enabled 按既有弹窗／内联模式持久化；权威保存结束后完成。
   */
  const save = async (enabled: boolean): Promise<void> => {
    if (savingRef.current || !preference.save) return;
    savingRef.current = true;
    setPending(enabled);
    setError("");
    try {
      if (!await preference.save(enabled ? "popup_only" : "inline_output")) {
        setError(t("settings.thinkingPopup.savingFailedTheSystem"));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      savingRef.current = false;
      setPending(null);
    }
  };
  return <section className="settings-form-grid mt-6">
    <label className="flex items-center justify-between gap-4">
      <span>{t("settings.thinkingPopup.showThinkingInA")}</span>
      <Switch
        checked={pending ?? normalizeThinkingDisplayMode(preference.mode) === "popup_only"}
        disabled={pending !== null || !preference.save}
        onCheckedChange={(enabled) => { void save(enabled); }}
      />
    </label>
    <p className="settings-muted-line">{t("settings.thinkingPopup.whenOnAiThinking")}</p>
    {pending !== null ? <p role="status" className="settings-muted-line">{t("settings.thinkingPopup.saving")}</p> : null}
    {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
  </section>;
}
