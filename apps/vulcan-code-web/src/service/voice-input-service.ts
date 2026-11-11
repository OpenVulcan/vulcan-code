import type {
  VoiceInputSettingsSnapshot,
  VoiceInputSelection,
  VoiceInputCloudCatalog,
  VoiceInputTranscription,
} from "@/types";
import { blobToBase64 } from "@/service/conversation-actions-service";
import { serviceClient } from "@/service/webui-service-runtime";

// loadVoiceInputSettings reads the saved voice-input target and the usable candidates.
// loadVoiceInputSettings 读取已保存的语音输入目标与可用候选。
export async function loadVoiceInputSettings(): Promise<VoiceInputSettingsSnapshot> {
  return serviceClient.request<VoiceInputSettingsSnapshot>("service_voice_input_settings");
}

// Loads cloud availability independently of local voice input settings.
// 独立于本地语音输入配置加载云端可用状态。
export async function loadVoiceInputCloudCatalog(): Promise<VoiceInputCloudCatalog> {
  return serviceClient.request<VoiceInputCloudCatalog>("service_voice_input_cloud_catalog");
}

// Returns the visible route for the local settings and the latest regional catalog.
// 根据本地设置和最新区域目录返回界面展示的路由。
// A newly saved exact choice takes precedence over the preference resolved by an earlier catalog read.
// 刚保存的精确选择优先于较早读取目录时解析的偏好。
export function resolveVoiceInputSelection(
  settings: VoiceInputSettingsSnapshot | null,
  cloud: VoiceInputCloudCatalog | null,
): VoiceInputSelection | null {
  if (!settings || settings.disabled) return null;
  const saved = settings.selection;
  if (cloud?.status !== "available" || (saved && "connection_id" in saved)) return saved;
  if (saved) {
    const preference = saved.openvulcan_cloud;
    if (preference.connection_id !== cloud.connection_id || cloud.models.some(({ id }) => id === preference.model_id)) return saved;
  }
  return cloud.selection ? { openvulcan_cloud: cloud.selection } : saved;
}

// Cloud availability message keys are shared between the editor and the microphone card.
// 配置编辑器与麦克风卡片共用云端可用状态的目录键。
export const voiceInputCloudStatusKeys = {
  sign_in_required: "settings.voiceInput.cloudSignIn",
  disabled: "settings.voiceInput.cloudDisabled",
  unavailable: "settings.voiceInput.cloudUnavailable",
} as const;

// saveVoiceInputSelection saves or clears the voice-input target; the workbench republishes on success.
// saveVoiceInputSelection 保存或清除语音输入目标；成功后工作台重新发布。
export async function saveVoiceInputSelection(
  selection: VoiceInputSelection | null,
): Promise<VoiceInputSettingsSnapshot> {
  return serviceClient.request<VoiceInputSettingsSnapshot>("service_save_voice_input_selection", {
    request: { selection },
  });
}

// transcribeVoiceInput sends one finished recording and returns its transcript.
// transcribeVoiceInput 发送一段已完成的录音并返回其转写文本。
export async function transcribeVoiceInput(recording: Blob): Promise<VoiceInputTranscription> {
  return serviceClient.request<VoiceInputTranscription>("service_transcribe_voice_input", {
    request: { dataBase64: await blobToBase64(recording), mimeType: recording.type },
  });
}
