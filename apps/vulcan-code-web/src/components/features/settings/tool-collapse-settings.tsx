import React from "react";
import { ChevronDown, ChevronRight, Terminal } from "lucide-react";
import { ToolCollapsePreferencesContext } from "@/components/conversation/tool-collapse-preferences";
import { useI18n } from "@/i18n";
import { SettingsPreviewChoices } from "./settings-preview-choices";

/** Renders both completed tool layouts without changing per-session choices; takes no props and returns the picker.
 * 渲染工具执行完成后的两种布局，不改变会话选择；无参数，返回选择器。
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
  return <SettingsPreviewChoices
    label={t("settings.toolCollapse.collapseToolCallsBy")}
    description={t("settings.toolCollapse.whenOnToolCalls")}
    value={(pending ?? preferences.defaultEnabled) ? "collapsed" : "expanded"}
    disabled={pending !== null || !preferences.saveDefault}
    onSelect={(next) => { void save(next === "collapsed"); }}
    options={[
      { value: "collapsed", label: t("settings.preview.collapse"), preview: <ToolCallExample expanded={false} /> },
      { value: "expanded", label: t("settings.preview.expand"), preview: <ToolCallExample expanded /> },
    ]}
  >
    {pending !== null ? <p role="status" className="vk-form-note">{t("settings.toolCollapse.saving")}</p> : null}
    {error ? <p role="alert" className="vk-field-error">{error}</p> : null}
  </SettingsPreviewChoices>;
}

// Render the same completed command with or without its arguments and result.
// 使用同一条已完成的命令，按 expanded 展示或隐藏参数和结果。
function ToolCallExample({ expanded }: { expanded: boolean }) {
  return <span className="session-defaults-scene">
    <span className="session-defaults-mini-row">
      <Terminal size={14} /> shell_command
      {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
    </span>
    {expanded ? <span className="session-defaults-mini-code">
      <code>git status --short</code><code>M  README.md</code>
    </span> : null}
  </span>;
}
