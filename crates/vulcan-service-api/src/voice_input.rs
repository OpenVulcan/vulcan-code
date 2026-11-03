use super::*;

/// Composer voice-input state published with the workbench snapshot.
/// 随工作台快照发布的输入区语音输入状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInputSummary {
    /// Whether the user has chosen a speech-transcription target for voice input.
    /// 用户是否已为语音输入选择语音转写目标。
    pub configured: bool,
    /// Longest single recording in seconds; the composer stops recording at this limit.
    /// 单次录音的最长秒数；输入区到达该上限时停止录音。
    pub max_duration_seconds: u32,
}

/// Text transcribed from one composer voice recording.
/// 一次输入区语音录音转写得到的文本。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInputTranscription {
    /// Transcript to insert into the composer draft.
    /// 插入输入区草稿的转写文本。
    pub text: String,
}
