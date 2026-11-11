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
import { loadVoiceInputCloudCatalog, loadVoiceInputSettings, resolveVoiceInputSelection, saveVoiceInputSelection, voiceInputCloudStatusKeys } from "@/service/voice-input-service";
import type { VoiceInputCloudCatalog, VoiceInputParameter, VoiceInputParameters, VoiceInputSelection, VoiceInputSettingsSnapshot } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css";

// VOICE_INPUT_OFF is the picker value that clears the target and disables the composer microphone.
// VOICE_INPUT_OFF 是清除目标并禁用输入区麦克风的选择值。
const VOICE_INPUT_OFF = "voice-input-off";

// Picker value that removes an explicit recognition option.
// 清除显式识别选项的选择值。
const VOICE_INPUT_PARAMETER_DEFAULT = "voice-input-model-default";

// Returns a picker key for the given source, account and model so the two sources cannot collide.
// 根据来源、账号和模型返回选择器键，避免两种来源的模型冲突。
function selectionKey(source: "direct" | "cloud", connectionId: string, modelId: string): string {
  return JSON.stringify([source, connectionId, modelId]);
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

  // Cloud discovery runs independently so local provider settings stay usable offline.
  // 云端发现独立运行，让本地供应商设置离线时仍可用。
  const [cloud, setCloud] = React.useState<VoiceInputCloudCatalog | null>(null);
  React.useEffect(() => {
    let cancelled = false;
    void loadVoiceInputCloudCatalog().then(
      (next) => { if (!cancelled) setCloud(next); },
      () => { if (!cancelled) setCloud({ status: "unavailable" }); },
    );
    return () => { cancelled = true; };
  }, []);

  const candidates = React.useMemo(() => snapshot?.candidates ?? [], [snapshot]);
  // Each option carries its exact service selection so saving can preserve the declared routing.
  // 每个选项携带对应的服务选择，保存时按声明的路径转写。
  const options = React.useMemo(() => [
    { value: VOICE_INPUT_OFF, label: t("settings.voiceInput.none"), selection: null },
    ...(cloud?.status === "available" ? cloud.models.map((model) => ({
      value: selectionKey("cloud", cloud.connection_id, model.id),
      label: model.display_name,
      description: model.id === cloud.default_model_id
        ? `${t("settings.voiceInput.cloudTitle")} · ${t("settings.voiceInput.cloudDefaultModel")}`
        : t("settings.voiceInput.cloudTitle"),
      keywords: [model.id],
      selection: { openvulcan_cloud: { connection_id: cloud.connection_id, model_id: model.id } },
    })) : []),
    ...candidates.map((candidate) => ({
      value: selectionKey("direct", candidate.connectionId, candidate.targetId),
      label: candidate.displayName,
      description: candidate.connectionDisplayName,
      keywords: [candidate.providerId, candidate.targetId, candidate.connectionDisplayName],
      selection: { connection_id: candidate.connectionId, target_id: candidate.targetId },
    })),
  ], [candidates, cloud, t]);

  // Identify cloud and direct choices from their disjoint service shapes.
  // 根据互斥的服务结构识别云端和直连选择。
  const saved = resolveVoiceInputSelection(snapshot, cloud);
  const direct = saved && "connection_id" in saved ? saved : null;
  const cloudSaved = saved && "openvulcan_cloud" in saved ? saved.openvulcan_cloud : null;
  const savedKey = cloudSaved ? selectionKey("cloud", cloudSaved.connection_id, cloudSaved.model_id)
    : direct ? selectionKey("direct", direct.connection_id, direct.target_id) : VOICE_INPUT_OFF;
  const selected = direct && candidates.find((candidate) =>
    candidate.connectionId === direct.connection_id && candidate.targetId === direct.target_id);
  const cloudSelected = cloud?.status === "available" && cloudSaved?.connection_id === cloud.connection_id
    ? cloud.models.find((model) => model.id === cloudSaved.model_id)
    : undefined;
  const staleCloudLanguage = cloudSelected && cloudSaved?.language && !cloudSelected.supported_languages.includes(cloudSaved.language);
  const cloudFields: VoiceInputParameter[] = cloudSelected && (cloudSelected.supported_languages.length > 0 || staleCloudLanguage)
    ? [{ name: "language", kind: "language", choices: cloudSelected.supported_languages }]
    : [];

  // Commits the target and its settings, then displays the supplied success message.
  // 提交目标和对应设置，随后显示传入的成功提示。
  const save = async (selection: VoiceInputSelection | null, message = t("settings.voiceInput.saved")) => {
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

  // Saves the option identified by the picker key only when the choice changes.
  // 仅当选择变化时，保存选择器键对应的选项。
  const select = (key: string): void => {
    const option = options.find(({ value }) => value === key);
    if (option && (key !== savedKey || (key === VOICE_INPUT_OFF && !snapshot?.disabled))) {
      void save(option.selection);
    }
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
        emptyText={t("conversation.find.none")}
        onValueChange={select}
        options={options}
        placeholder={t("settings.voiceInput.none")}
        searchPlaceholder={t("settings.voiceInput.search")}
        value={savedKey}
      />
      {cloud?.status !== "available" ? (
        <p className="vk-form-note">{t(cloud ? voiceInputCloudStatusKeys[cloud.status] : "composer.voice.loading")}</p>
      ) : cloudSelected ? (
        <p className="vk-form-note">{t("settings.voiceInput.cloudTitle")}</p>
      ) : null}
      {(direct && !selected) || (cloudSaved && cloud?.status === "available" && !cloudSelected)
        ? <p role="alert" className="vk-field-error">{t("settings.voiceInput.unavailable")}</p>
        : null}
      {loadError ? <SettingsErrorNotice detail={loadError} message={t("settings.voiceInput.loadFailed")} /> : null}
      {saveError ? <SettingsErrorNotice detail={saveError} message={t("settings.voiceInput.saveFailed")} /> : null}
      {direct && selected && selected.parameters.length > 0 ? (
        <VoiceInputModelParameters busy={busy} fields={selected.parameters} key={savedKey} saved={direct.parameters}
          onSave={(parameters) => void save({ ...direct, parameters }, t("settings.voiceInput.parametersSaved"))} />
      ) : null}
      {staleCloudLanguage
        ? <p role="alert" className="vk-field-error">{t("settings.voiceInput.cloudLanguageUnavailable")}</p>
        : null}
      {cloudSaved && cloudFields.length > 0 ? (
        <VoiceInputModelParameters busy={busy} fields={cloudFields}
          key={savedKey}
          saved={cloudSaved.language ? { language: cloudSaved.language } : undefined}
          onSave={(parameters) => {
            const { language, ...identity } = cloudSaved;
            const preferredLanguage = typeof parameters.language === "string" ? parameters.language : undefined;
            if (preferredLanguage !== language) {
              void save({ openvulcan_cloud: { ...identity, language: preferredLanguage } }, t("settings.voiceInput.parametersSaved"));
            }
          }} />
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
