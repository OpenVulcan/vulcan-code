import * as React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { Combobox } from "@/ui-kit/primitives/combobox";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadVoiceInputSettings, saveVoiceInputSelection } from "@/service/voice-input-service";
import type { VoiceInputSettingsSnapshot, VoiceInputTargetSelection } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css";

// VOICE_INPUT_OFF is the picker value that clears the target and disables the composer microphone.
// VOICE_INPUT_OFF 是清除目标并禁用输入区麦克风的选择值。
const VOICE_INPUT_OFF = "voice-input-off";

// selectionKey identifies one target on one account inside the picker.
// selectionKey 在选择器中标识某个账号上的一个目标。
function selectionKey(connectionId: string, targetId: string): string {
  return JSON.stringify([connectionId, targetId]);
}

// VoiceInputSettingsSection picks the speech-transcription target used by the composer microphone.
// VoiceInputSettingsSection 选择输入区麦克风使用的语音转写目标。
// Parameters: `onSaved` runs after a successful save, such as closing the composer's quick setup dialog.
// 参数：`onSaved` 在保存成功后执行，例如关闭输入区的快速设置对话框。
export function VoiceInputSettingsSection(props: { onSaved?: () => void }) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<VoiceInputSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState("");
  const [saveError, setSaveError] = React.useState("");
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    let cancelled = false;
    void loadVoiceInputSettings().then(
      (next) => { if (!cancelled) setSnapshot(next); },
      (cause: unknown) => { if (!cancelled) setLoadError(errorMessage(cause)); },
    );
    return () => { cancelled = true; };
  }, []);

  const candidates = React.useMemo(() => snapshot?.candidates ?? [], [snapshot]);
  const options = React.useMemo(() => [
    { value: VOICE_INPUT_OFF, label: t("settings.voiceInput.none"), keywords: [] },
    ...candidates.map((candidate) => ({
      value: selectionKey(candidate.connectionId, candidate.targetId),
      label: candidate.displayName,
      description: candidate.connectionDisplayName,
      keywords: [candidate.providerId, candidate.targetId, candidate.connectionDisplayName],
    })),
  ], [candidates, t]);

  const saved = snapshot?.selection ?? null;
  const savedKey = saved ? selectionKey(saved.connection_id, saved.target_id) : "";
  const selected = candidates.find((candidate) => selectionKey(candidate.connectionId, candidate.targetId) === savedKey);

  // save commits the target; the service republishes the workbench so the microphone follows.
  // save 提交目标；服务会重新发布工作台，麦克风随之更新。
  const save = async (selection: VoiceInputTargetSelection | null) => {
    setBusy(true);
    setSaveError("");
    try {
      setSnapshot(await saveVoiceInputSelection(selection));
      toast.success(t("settings.voiceInput.saved"));
      props.onSaved?.();
    } catch (cause) {
      setSaveError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const select = (key: string) => {
    if (key === savedKey) return;
    if (key === VOICE_INPUT_OFF) {
      if (saved) void save(null);
      return;
    }
    const candidate = candidates.find((item) => selectionKey(item.connectionId, item.targetId) === key);
    if (candidate) void save({ connection_id: candidate.connectionId, target_id: candidate.targetId });
  };

  return (
    <section className="vk-form-pref" data-voice-input-settings>
      <div className="vk-form-pref-text">
        <strong>{t("settings.voiceInput.title")}</strong>
        <p className="vk-form-note">
          {formatMessage(t("settings.voiceInput.hint"), { seconds: snapshot?.maxDurationSeconds ?? 0 })}
        </p>
      </div>
      <Combobox
        ariaLabel={t("settings.voiceInput.title")}
        disabled={busy || !snapshot}
        emptyText={t("settings.voiceInput.noCandidate")}
        onValueChange={select}
        options={options}
        placeholder={t("settings.voiceInput.none")}
        searchPlaceholder={t("settings.voiceInput.search")}
        value={selected ? savedKey : saved ? "" : VOICE_INPUT_OFF}
      />
      {saved && snapshot && !selected
        ? <p role="alert" className="vk-field-error">{t("settings.voiceInput.unavailable")}</p>
        : null}
      {loadError ? <SettingsErrorNotice detail={loadError} message={t("settings.voiceInput.loadFailed")} /> : null}
      {saveError ? <SettingsErrorNotice detail={saveError} message={t("settings.voiceInput.saveFailed")} /> : null}
    </section>
  );
}
