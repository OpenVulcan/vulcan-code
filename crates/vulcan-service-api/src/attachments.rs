use super::*;

/// Summarizes one composer attachment kind exposed to WebUI controls.
/// 汇总 WebUI 输入区控件可使用的单种附件类型。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerAttachmentCapabilitySummary {
    /// Stable attachment kind key such as image, audio, video, or pdf.
    /// 稳定附件类型键，例如 image、audio、video 或 pdf。
    pub kind: String,
    /// User-facing attachment kind label.
    /// 面向用户显示的附件类型名称。
    pub label: String,
    /// Whether this attachment kind can be submitted for the active model/tool setup.
    /// 当前模型与工具配置下是否允许提交该附件类型。
    pub enabled: bool,
    /// Whether the active provider/model can consume this attachment natively.
    /// 当前 Provider/模型是否能原生消费该附件类型。
    pub native: bool,
    /// Whether host-side AI tools can bridge this attachment kind when native support is missing.
    /// 原生能力缺失时宿主侧 AI 工具是否能桥接该附件类型。
    pub bridged: bool,
    /// Browser file picker accept string for this attachment kind.
    /// 该附件类型对应的浏览器文件选择 accept 字符串。
    pub accept: String,
}

/// Describes one staged composer attachment shared by the service and WebUI.
/// 描述一条由 service 与 WebUI 共享的已暂存输入区附件。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerAttachmentSummary {
    /// Stable attachment kind key such as image, audio, video, or pdf.
    /// 稳定附件类型键，例如 image、audio、video 或 pdf。
    pub kind: String,
    /// Human-readable file name shown in composer and transcript previews.
    /// 展示在输入区与转录预览中的人类可读文件名。
    pub file_name: String,
    /// Canonical managed local file path used for provider replay.
    /// Provider 回放使用的规范托管本地文件路径。
    pub local_path: String,
    /// Optional original source path retained for diagnostics.
    /// 为诊断保留的可选原始来源路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Optional stable managed asset identifier.
    /// 可选稳定托管资产标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    /// Optional source byte length.
    /// 可选源字节长度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    /// Optional local preview image path.
    /// 可选本地预览图路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_local_path: Option<String>,
    /// Resolved MIME type attached to the staged file.
    /// 绑定到已暂存文件的解析后 MIME 类型。
    pub mime_type: String,
}

/// Represents the author role for a conversation message.
/// 表示会话消息的作者角色。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MessageRole {
    /// Message created by the user.
    /// 用户创建的消息。
    User,
    /// Message created by the assistant.
    /// 助手创建的消息。
    Assistant,
    /// Message created by the system.
    /// 系统创建的消息。
    System,
}

/// Summarizes one tool call in the WebUI.
/// 汇总 WebUI 中的单个工具调用。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallSummary {
    /// Stable tool call identifier.
    /// 稳定工具调用标识。
    pub id: String,
    /// Tool display name.
    /// 工具显示名称。
    pub name: String,
    /// Tool call status.
    /// 工具调用状态。
    pub status: String,
    /// Tool call detail text.
    /// 工具调用详情文本。
    pub detail: String,
}
