import React from "react";
import { Switch } from "@/components/ui/switch";
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
  // locale selects system-settings copy.
  // locale 选择系统设置文案。
  const { locale } = useI18n();
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
        setError(locale === "zh-CN" ? "保存失败，系统默认值未更改。" : "Saving failed. The system default is unchanged.");
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
      <span>{locale === "zh-CN" ? "默认开启思考弹窗" : "Enable thinking popups by default"}</span>
      <Switch
        checked={pending ?? normalizeThinkingDisplayMode(preference.mode) === "popup_only"}
        disabled={pending !== null || !preference.save}
        onCheckedChange={(enabled) => { void save(enabled); }}
      />
    </label>
    <p className="settings-muted-line">{locale === "zh-CN"
      ? "选择“继承系统”的会话使用此全局默认值。开启时通过思考入口查看弹窗，关闭时在会话中显示思考内容；不会修改单独设置过的会话。"
      : "Sessions set to inherit use this global default. Enable to view thinking in a popup, or disable to display it inline. Explicit session choices remain unchanged."}</p>
    {pending !== null ? <p role="status" className="settings-muted-line">{locale === "zh-CN" ? "正在保存…" : "Saving…"}</p> : null}
    {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
  </section>;
}
