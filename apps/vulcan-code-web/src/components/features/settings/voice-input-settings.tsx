import * as React from "react";
import { Button } from "@/ui-kit/primitives/button";
import { toast } from "@/ui-kit/primitives/toast";
import { Combobox } from "@/ui-kit/primitives/combobox";
import { Input } from "@/ui-kit/primitives/input";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import {
  formatVoiceInputShortcut,
  readVoiceInputShortcut,
  saveVoiceInputShortcut,
  voiceInputShortcutFromKey,
} from "@/components/composer/composer-panel/voice-input-shortcut";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadVoiceInputSettings, saveVoiceInputSelection } from "@/service/voice-input-service";
import type { VoiceInputParameter, VoiceInputParameters, VoiceInputSettingsSnapshot, VoiceInputTargetSelection } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css";

// VOICE_INPUT_OFF is the picker value that clears the target and disables the composer microphone.
// VOICE_INPUT_OFF 是清除目标并禁用输入区麦克风的选择值。
const VOICE_INPUT_OFF = "voice-input-off";

// Picker value that removes an explicit recognition option.
// 清除显式识别选项的选择值。
const VOICE_INPUT_PARAMETER_DEFAULT = "voice-input-model-default";

// selectionKey identifies one target on one account inside the picker.
// selectionKey 在选择器中标识某个账号上的一个目标。
function selectionKey(connectionId: string, targetId: string): string {
  return JSON.stringify([connectionId, targetId]);
}

// Translation keys for the recognition options declared by the service.
// 服务声明的识别选项所对应的翻译键。
const parameterLabels = {
  language: "settings.voiceInput.language",
  prompt: "settings.voiceInput.parameterPrompt",
  enable_itn: "settings.voiceInput.parameterTextNormalization",
  keep_dialect: "settings.voiceInput.parameterKeepDialect",
  vocabulary_id: "settings.voiceInput.parameterVocabularyId",
} as const;

// Settings for one exact microphone target; onSave receives the explicit overrides.
// 单个精确麦克风目标的设置；onSave 接收用户显式覆盖的参数。
interface VoiceInputModelParametersProps {
  fields: VoiceInputParameter[];
  saved: VoiceInputParameters | undefined;
  busy: boolean;
  onSave: (parameters: VoiceInputParameters) => void;
}

// Renders the target's recognition options and submits only explicit overrides.
// 渲染目标的识别选项，并且只提交显式覆盖的参数。
function VoiceInputModelParameters({ fields, saved, busy, onSave }: VoiceInputModelParametersProps) {
  // Render language names in the active interface locale while retaining provider codes.
  // 用当前界面语言显示语言名称，同时保留供应商语言代码。
  const { locale, t } = useI18n();
  const languageNames = React.useMemo(() => new Intl.DisplayNames([locale], { type: "language" }), [locale]);
  // Keep edits local until the user saves; model switches remount this editor.
  // 编辑暂存在本地，直到用户保存；切换模型时重新挂载编辑器。
  const [draft, setDraft] = React.useState<VoiceInputParameters>(() => ({ ...saved }));
  // Compare the visible fields with the saved values, including an explicit false.
  // 比较可见字段和已保存值，包括显式的 false。
  const changed = fields.some(({ name }) => draft[name] !== saved?.[name]);
  const inputId = React.useId();
  const defaultLabel = t("settings.voiceInput.parameterDefault");

  // Update an explicit value; an unset value leaves the provider default in effect.
  // 更新显式值；未设置时继续使用供应商默认值。
  const edit = (name: VoiceInputParameter["name"], value: string | boolean | undefined) => {
    setDraft((previous) => {
      // Clone the draft so edits do not mutate the last saved snapshot.
      // 复制草稿，避免编辑修改上一次保存的快照。
      const next = { ...previous };
      if (value === undefined) delete next[name];
      else next[name] = value;
      return next;
    });
  };

  // Save only fields still declared for this target.
  // 只保存此目标仍然声明的字段。
  const save = () => onSave(Object.fromEntries(
    fields.filter(({ name }) => Object.hasOwn(draft, name)).map(({ name }) => [name, draft[name]]),
  ));

  return (
    <div className="vk-voice-input-fields">
      {fields.map((field) => {
        // Associate each localized option label with its own input.
        // 将每个本地化选项标签关联到对应输入控件。
        const id = `${inputId}-${field.name}`;
        const label = t(parameterLabels[field.name]);
        const value = draft[field.name];
        if (field.kind === "text") {
          return (
            <div className="vk-voice-input-field" key={field.name}>
              <label className="vk-field-label" htmlFor={id}>{label}</label>
              <Input disabled={busy} id={id} onChange={(event) => edit(field.name, event.target.value || undefined)}
                placeholder={defaultLabel} value={typeof value === "string" ? value : ""} />
            </div>
          );
        }
        // Model enums supply the language choices; boolean fields can be unset, enabled or disabled.
        // 模型枚举提供语言选项；布尔字段可保持未设置、启用或禁用。
        const options = field.kind === "language"
          ? field.choices.map((code) => ({
            value: code,
            label: code === "auto" ? t("settings.voiceInput.languageAuto") : `${languageNames.of(code)} (${code})`,
          }))
          : [
            { value: "true", label: t("settings.voiceInput.parameterEnabled") },
            { value: "false", label: t("settings.voiceInput.parameterDisabled") },
          ];
        return (
          <div className="vk-voice-input-field" key={field.name}>
            <span className="vk-field-label">{label}</span>
            <Combobox ariaLabel={label} className="vk-voice-model-select" disabled={busy}
              emptyText={t("conversation.find.none")}
              onValueChange={(key) => {
                if (key === VOICE_INPUT_PARAMETER_DEFAULT) edit(field.name, undefined);
                else edit(field.name, field.kind === "boolean" ? key === "true" : key);
              }}
              options={[{ value: VOICE_INPUT_PARAMETER_DEFAULT, label: defaultLabel }, ...options]}
              placeholder={defaultLabel} searchPlaceholder={label}
              value={value === undefined ? VOICE_INPUT_PARAMETER_DEFAULT : String(value)} />
          </div>
        );
      })}
      <div>
        <Button disabled={busy || !changed} onClick={save} type="button" variant="outline">
          {t("settings.voiceInput.saveParameters")}
        </Button>
      </div>
    </div>
  );
}

// Renders the microphone model picker and the client-local shortcut editor. Takes no parameters.
// 渲染麦克风模型选择器和本地快捷键编辑器。不接收参数。
export function VoiceInputSettingsSection() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<VoiceInputSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState("");
  const [saveError, setSaveError] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  // The editor saves shortcuts locally, independently of the service's model selection.
  // 编辑器在本地保存快捷键，独立于服务端的模型选择。
  const [shortcut, setShortcut] = React.useState(readVoiceInputShortcut);
  // Report capture and storage errors without changing the existing shortcut.
  // 报告捕获或存储错误，同时保留现有快捷键。
  const [shortcutError, setShortcutError] = React.useState<"invalid" | "save" | null>(null);
  // Connect the shortcut label and help to the keyboard capture field.
  // 将快捷键标签和帮助说明与按键捕获输入框关联。
  const shortcutId = React.useId();

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

  // Commits the target and its settings, then displays the supplied success message.
  // 提交目标和对应设置，随后显示传入的成功提示。
  const save = async (selection: VoiceInputTargetSelection | null, message = t("settings.voiceInput.saved")) => {
    setBusy(true);
    setSaveError("");
    try {
      setSnapshot(await saveVoiceInputSelection(selection));
      toast.success(message);
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

  // Persist the captured chord or clear it for null. The editor updates only after storage succeeds.
  // 保存捕获的组合键，null 表示清除。存储成功后才更新编辑器。
  const saveShortcut = (next: string | null) => {
    try {
      saveVoiceInputShortcut(next);
      setShortcut(next);
      setShortcutError(null);
    } catch {
      setShortcutError("save");
    }
  };

  // Capture a chord in the focused field while keeping Tab navigation and Escape dismissal available.
  // 在焦点输入框捕获组合键，同时保留 Tab 导航与 Escape 关闭。
  const captureShortcut = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Tab" || event.key === "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat || /^(?:Alt|Control|Meta|Shift)(?:Left|Right)$/.test(event.code)) return;
    // An unsupported key leaves the existing binding intact.
    // 不支持的按键保留现有绑定。
    const next = voiceInputShortcutFromKey(event.nativeEvent);
    if (next) saveShortcut(next);
    else setShortcutError("invalid");
  };

  return (
    <section className="vk-form-pref" data-voice-input-settings>
      <div className="vk-form-pref-text">
        <strong>{t("settings.voiceInput.title")}</strong>
        {snapshot ? (
          <p className="vk-form-note">
            {formatMessage(t("settings.voiceInput.hint"), { seconds: snapshot.maxDurationSeconds })}
          </p>
        ) : null}
      </div>
      <Combobox
        ariaLabel={t("settings.voiceInput.title")}
        className="vk-voice-model-select"
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
      {saved && selected && selected.parameters.length > 0 ? (
        <VoiceInputModelParameters busy={busy} fields={selected.parameters} key={savedKey} saved={saved.parameters}
          onSave={(parameters) => void save({ ...saved, parameters }, t("settings.voiceInput.parametersSaved"))} />
      ) : null}
      <div className="vk-voice-input-field">
        <label className="vk-field-label" htmlFor={shortcutId}>{t("settings.voiceInput.shortcut")}</label>
        <div className="vk-voice-shortcut-controls">
          <Input
            aria-describedby={`${shortcutId}-hint`}
            aria-invalid={shortcutError !== null || undefined}
            id={shortcutId}
            onKeyDown={captureShortcut}
            placeholder={t("settings.voiceInput.shortcutPlaceholder")}
            readOnly
            value={shortcut ? formatVoiceInputShortcut(shortcut) : ""}
          />
          <Button disabled={!shortcut} onClick={() => saveShortcut(null)} type="button" variant="outline">
            {t("settings.voiceInput.shortcutClear")}
          </Button>
        </div>
        <p className="vk-form-note" id={`${shortcutId}-hint`}>{t("settings.voiceInput.shortcutHint")}</p>
        {shortcutError ? (
          <p className="vk-field-error" role="alert">
            {t(shortcutError === "save" ? "settings.voiceInput.shortcutSaveFailed" : "settings.voiceInput.shortcutInvalid")}
          </p>
        ) : null}
      </div>
    </section>
  );
}
