use super::*;

/// Composer voice-input state published with the workbench snapshot.
/// 随工作台快照发布的输入区语音输入状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInputSummary {
    /// Whether a direct-provider route is configured for voice input.
    /// 是否已为语音输入配置直连供应商路由。
    pub configured: bool,
    /// Native account eligible for a cloud route, pending regional catalog resolution.
    /// 可使用云端路由的原生账号，需查询区域目录确认可用性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_account_id: Option<String>,
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
