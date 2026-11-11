import * as React from "react";
import { Check, ChevronRight, Settings2 } from "lucide-react";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadVoiceInputCloudCatalog, loadVoiceInputSettings, resolveVoiceInputSelection, voiceInputCloudStatusKeys } from "@/service/voice-input-service";
import type { VoiceInputCloudCatalog, VoiceInputSettingsSnapshot } from "@/types";
import {
  readVoiceInputDevice,
  saveVoiceInputDevice,
  useAudioInputDevices,
} from "@/components/composer/composer-panel/voice-input-device";

// VoiceInputCardProps describe the published voice-input state the card summarizes.
// VoiceInputCardProps 描述卡片所概括的已发布语音输入状态。
export interface VoiceInputCardProps {
  configured: boolean;
  maxDurationSeconds: number;
  // Opens the quick setup dialog for the transcription model.
  // 打开转写模型的快速设置对话框。
  onConfigure: () => void;
}

// VoiceInputCard shows the transcription model, picks the microphone and links to quick setup.
// It mounts only while the hover card is open, so the model is read fresh on every open.
// VoiceInputCard 展示转写模型、选择麦克风并链接到快速设置。
// 它只在悬停卡片打开期间挂载，因此每次打开都会重新读取模型。
export function VoiceInputCard(props: VoiceInputCardProps) {
  const { t } = useI18n();
  const [settings, setSettings] = React.useState<VoiceInputSettingsSnapshot | null>(null);
  const [loadError, setLoadError] = React.useState("");
  const [deviceId, setDeviceId] = React.useState(readVoiceInputDevice);
  const devices = useAudioInputDevices(true);

  React.useEffect(() => {
    let cancelled = false;
    void loadVoiceInputSettings().then(
      (next) => { if (!cancelled) setSettings(next); },
      (cause: unknown) => { if (!cancelled) setLoadError(errorMessage(cause)); },
    );
    return () => { cancelled = true; };
  }, []);

  // Only the cloud route needs regional discovery for the model shown in this card.
  // 仅云端路由需要通过区域目录解析此卡片中的模型。
  const [cloud, setCloud] = React.useState<VoiceInputCloudCatalog | null>(null);
  const selection = resolveVoiceInputSelection(settings, cloud);
  const direct = selection && "connection_id" in selection ? selection : null;
  const cloudSelection = selection && "openvulcan_cloud" in selection ? selection.openvulcan_cloud : null;
  // An unset choice can still resolve to a cloud model after native sign-in.
  // 选择未设置时，原生账号登录后仍可能解析出云端模型。
  const needsCloudCatalog = Boolean(settings && !settings.disabled && !direct);
  React.useEffect(() => {
    if (!needsCloudCatalog) return;
    let cancelled = false;
    void loadVoiceInputCloudCatalog().then(
      (next) => { if (!cancelled) setCloud(next); },
      () => { if (!cancelled) setCloud({ status: "unavailable" }); },
    );
    return () => { cancelled = true; };
  }, [needsCloudCatalog]);
  const cloudModel = cloud?.status === "available" && cloudSelection?.connection_id === cloud.connection_id
    ? cloud.models.find((model) => model.id === cloudSelection.model_id)
    : undefined;
  const target = direct ? settings?.candidates.find((candidate) => (
    candidate.connectionId === direct.connection_id && candidate.targetId === direct.target_id
  )) : undefined;
  const modelName = cloudModel?.display_name ?? target?.displayName;
  const sourceName = cloudModel ? t("settings.voiceInput.cloudTitle") : target?.connectionDisplayName;
  const cloudStatusKey = needsCloudCatalog
    ? !cloud ? "composer.voice.loading" : cloud.status !== "available" ? voiceInputCloudStatusKeys[cloud.status] : undefined
    : undefined;
  const selectDevice = (next: string | null) => {
    saveVoiceInputDevice(next);
    setDeviceId(next);
  };
  const namesHidden = devices.some((device) => !device.label);
  const savedMissing = deviceId !== null && devices.length > 0 && !devices.some((device) => device.deviceId === deviceId);

  return (
    <div className="vk-cap-sum" data-voice-input-card>
      <div className="vk-sum-title">
        <strong>{t("composer.voice.title")}</strong>
        <span>{formatMessage(t("composer.voice.limit"), { seconds: props.maxDurationSeconds })}</span>
      </div>
      <p className="vk-cap-sum-note">{t(props.configured ? "composer.voice.usage" : "composer.voice.notConfigured")}</p>
      <div className="vk-cap-sum-section">
        <h4>{t("composer.voice.model")}</h4>
        {loadError ? <p className="vk-cap-sum-note" data-tone="bad">{t("settings.voiceInput.loadFailed")}</p>
          : !settings ? <p className="vk-cap-sum-note">{t("composer.voice.loading")}</p>
            : modelName ? (
              <span className="vk-cap-sum-text">
                <span className="vk-cap-sum-label">{modelName}</span>
                <span className="vk-cap-sum-detail">{sourceName}</span>
              </span>
            ) : (
              <p className="vk-cap-sum-note" data-tone={selection ? "bad" : undefined}>
                {t(cloudStatusKey ?? (selection ? "settings.voiceInput.unavailable" : "composer.voice.modelNone"))}
              </p>
            )}
      </div>
      <div className="vk-cap-sum-section">
        <h4 id="vk-voice-mic-heading">{t("composer.voice.microphone")}</h4>
        <div aria-labelledby="vk-voice-mic-heading" className="vk-voice-mics" role="radiogroup">
          <MicrophoneOption checked={deviceId === null} label={t("composer.voice.systemDefault")} onSelect={() => selectDevice(null)} />
          {devices.map((device, index) => (
            <MicrophoneOption
              checked={deviceId === device.deviceId}
              key={device.deviceId}
              label={device.label || formatMessage(t("composer.voice.micFallback"), { index: index + 1 })}
              onSelect={() => selectDevice(device.deviceId)}
            />
          ))}
          {savedMissing ? <MicrophoneOption checked label={t("composer.voice.micMissing")} onSelect={() => undefined} tone="bad" /> : null}
        </div>
        {namesHidden ? <p className="vk-cap-sum-note">{t("composer.voice.micNamesHidden")}</p> : null}
      </div>
      <div className="vk-cap-sum-foot">
        <button className="vk-cap-sum-action" onClick={props.onConfigure} type="button">
          <Settings2 aria-hidden="true" size={13} />
          <span>{t("composer.voice.configure")}</span>
          <ChevronRight aria-hidden="true" size={13} />
        </button>
      </div>
    </div>
  );
}

// MicrophoneOption is one radio row in the microphone list.
// MicrophoneOption 是麦克风列表中的一行单选项。
function MicrophoneOption(props: { checked: boolean; label: string; onSelect: () => void; tone?: "bad" }) {
  return (
    <button
      aria-checked={props.checked}
      className="vk-menu-item"
      data-pointer-hover=""
      data-tone={props.tone}
      onClick={props.onSelect}
      role="radio"
      type="button"
    >
      <span className="vk-voice-mic-label">{props.label}</span>
      {props.checked ? <Check aria-hidden="true" className="vk-menu-meta" size={13} /> : null}
    </button>
  );
}
