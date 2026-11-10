import React from "react";
import { Brain, PanelsTopLeft } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { ThinkingDisplaySystemContext } from "@/components/conversation/thinking-display-preferences";
import { normalizeThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useI18n } from "@/i18n";
import { SettingsPreviewChoices } from "./settings-preview-choices";

/** Renders the global popup default without changing session overrides.
 * 渲染全局弹窗默认值，不改变会话覆盖值。
 * Takes no props; returns a comparison with a popup that can be tried independently.
 * 无参数；返回两种模式的对照，弹窗可独立试开。
 */
export function ThinkingPopupSettingsSection() {
  // preference owns the authoritative global mode and its save action.
  // preference 持有权威全局模式及保存操作。
  const preference = React.useContext(ThinkingDisplaySystemContext);
  // t selects system-settings copy.
  // t 选择系统设置文案。
  const { t } = useI18n();
  // pending preserves the selected mode during persistence.
  // pending 在持久化期间保留所选模式。
  const [pending, setPending] = React.useState<boolean | null>(null);
  // popupOpen controls only the mock panel; it never writes the preference.
  // popupOpen 只控制模拟面板，绝不写入偏好。
  const [popupOpen, setPopupOpen] = React.useState(false);
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
  return <SettingsPreviewChoices
    label={t("settings.thinkingPopup.showThinkingInA")}
    description={t("settings.thinkingPopup.whenOnAiThinking")}
    value={(pending ?? normalizeThinkingDisplayMode(preference.mode) === "popup_only") ? "popup_only" : "inline_output"}
    disabled={pending !== null || !preference.save}
    onSelect={(next) => { void save(next === "popup_only"); }}
    action={<Button aria-expanded={popupOpen} onClick={() => setPopupOpen(!popupOpen)} size="sm" type="button" variant="outline">
      <PanelsTopLeft />{t(popupOpen ? "settings.preview.closePopup" : "settings.preview.tryPopup")}
    </Button>}
    options={[
      { value: "popup_only", label: t("settings.preview.popup"), preview: <ThinkingExample popup open={popupOpen} /> },
      { value: "inline_output", label: t("settings.preview.inline"), preview: <ThinkingExample popup={false} open /> },
    ]}
  >
    {pending !== null ? <p role="status" className="vk-form-note">{t("settings.thinkingPopup.saving")}</p> : null}
    {error ? <p role="alert" className="vk-field-error">{error}</p> : null}
  </SettingsPreviewChoices>;
}

// Render a synthetic thought inline or behind a mock trigger; open reveals the example text.
// 将合成示例显示在对话中或模拟按钮后方；open 控制示例文字是否可见。
function ThinkingExample({ popup, open }: { popup: boolean; open: boolean }) {
  // t supplies the same fictional text to both presentations.
  // t 为两种呈现提供同一段示例文字。
  const { t } = useI18n();
  return <span className="session-defaults-scene">
    <span className="session-defaults-mini-row" data-active={popup || undefined}>
      <Brain size={14} />{t("conversation.block.thinking")}
      {popup ? <PanelsTopLeft size={13} /> : null}
    </span>
    {open ? <span className="session-defaults-thought" data-popup={popup || undefined}>{t("settings.preview.thought")}</span> : null}
  </span>;
}
