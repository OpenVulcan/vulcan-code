import type {
  VoiceInputSettingsSnapshot,
  VoiceInputTargetSelection,
  VoiceInputTranscription,
} from "@/types";
import { blobToBase64 } from "@/service/conversation-actions-service";
import { serviceClient } from "@/service/webui-service-runtime";

// loadVoiceInputSettings reads the saved voice-input target and the usable candidates.
// loadVoiceInputSettings 读取已保存的语音输入目标与可用候选。
export async function loadVoiceInputSettings(): Promise<VoiceInputSettingsSnapshot> {
  return serviceClient.request<VoiceInputSettingsSnapshot>("service_voice_input_settings");
}

// saveVoiceInputSelection saves or clears the voice-input target; the workbench republishes on success.
// saveVoiceInputSelection 保存或清除语音输入目标；成功后工作台重新发布。
export async function saveVoiceInputSelection(
  selection: VoiceInputTargetSelection | null,
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
