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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    /// Optional project-scoped resource identity used by persisted V1 drafts.
    /// V1 持久化草稿使用的可选项目级资源身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_resource_id: Option<u64>,
    /// Exact original input occurrence for a project resource, resolved only inside the owning V1 session.
    /// 项目资源的精确原输入出现，仅在所属V1会话内解析。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_source: Option<vulcan_session_core::v1::UserPromptResourceSource>,
    /// Project-relative canonical resource path safe for model and host-tool projection.
    /// 可安全投影给模型与宿主工具的项目相对规范资源路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_path: Option<String>,
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
    /// Optional SHA-256 digest of the exact managed source bytes.
    /// 可选的精确托管源字节 SHA-256 摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
    /// Optional request-local delivery decision frozen by draft preparation.
    /// 可选的、由草稿准备冻结的单次请求局部传输决策。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_plan: Option<ComposerAttachmentDeliveryPlan>,
    /// Optional local preview image path.
    /// 可选本地预览图路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_local_path: Option<String>,
    /// Resolved MIME type attached to the staged file.
    /// 绑定到已暂存文件的解析后 MIME 类型。
    pub mime_type: String,
}

/// Names the provider-neutral direct-delivery decision for one staged attachment.
/// 命名单个已暂存附件的供应商中立直接传输决策。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ComposerAttachmentDeliveryPlan {
    /// The exact managed source can be projected inline.
    /// 精确托管源可以直接内联投影。
    InlineOriginal,
    /// The exact route requires one provider-owned upload reference.
    /// 精确路线需要一个供应商持有的上传引用。
    UploadReference,
    /// A host-derived full-frame preview may be projected instead of the original bytes.
    /// 可以用宿主派生的完整画面预览替代原始字节进行投影。
    InlineDerivedPreview,
    /// Only deterministic attachment metadata may be projected.
    /// 只能投影确定性的附件元数据。
    MetadataOnly,
}

/// Names the draft preparation lifecycle for one stable attachment identity.
/// 命名单个稳定附件身份的草稿准备生命周期。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ComposerAttachmentPreparationPhase {
    /// Local staging is complete and no network work is required.
    /// 本地暂存已经完成且不需要网络工作。
    LocalReady,
    /// One provider upload has been queued.
    /// 一个供应商上传任务已经排队。
    UploadQueued,
    /// Source bytes are being transferred.
    /// 源字节正在传输。
    Uploading,
    /// All bytes were sent and the provider reference is pending confirmation.
    /// 全部字节已经发送，正在等待供应商确认引用。
    AwaitingProviderConfirmation,
    /// The selected route can consume the prepared attachment directly.
    /// 所选路线可以直接消费已准备附件。
    DirectReady,
    /// The selected route receives metadata only.
    /// 所选路线只接收元数据。
    MetadataOnly,
    /// Preparation failed and the attachment was downgraded to metadata-only delivery.
    /// 准备失败，附件已降级为仅元数据传输。
    Failed,
    /// Preparation was cancelled by a newer generation or explicit removal.
    /// 准备已被更新代际或显式移除取消。
    Cancelled,
}

/// Carries the frozen direct-delivery plan and current draft-preparation state for one attachment.
/// 携带单个附件的冻结直接传输计划与当前草稿准备状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerAttachmentPreparationSummary {
    /// Stable managed attachment identifier.
    /// 稳定托管附件标识。
    pub attachment_id: String,
    /// Caller-owned generation that invalidates late results.
    /// 由调用方持有、用于使迟到结果失效的准备代际。
    pub generation: u64,
    /// Exact client transfer identity used by progress and cancellation.
    /// 供进度与取消使用的精确客户端传输标识。
    pub transfer_id: String,
    /// Original physical progress identity when this view observes a previously started upload.
    /// 当本视图观察此前启动的上传时，保留原实体进度身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_transfer_id: Option<String>,
    /// Frozen provider-neutral direct-delivery decision.
    /// 冻结的供应商中立直接传输决策。
    pub delivery_plan: ComposerAttachmentDeliveryPlan,
    /// Current closed preparation phase.
    /// 当前封闭准备阶段。
    pub phase: ComposerAttachmentPreparationPhase,
    /// Whether this phase blocks prompt submission.
    /// 当前阶段是否阻止提示词提交。
    pub blocks_submission: bool,
    /// Stable route revision used to reject stale UI results.
    /// 用于拒绝陈旧界面结果的稳定路线修订。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_revision: Option<String>,
    /// Stable controlled reason explaining native delivery or degradation.
    /// 解释原生传输或降级的稳定受控原因。
    pub reason: String,
    /// Optional controlled failure detail for retry UI.
    /// 供重试界面使用的可选受控失败详情。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
