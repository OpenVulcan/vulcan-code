//! Provider-neutral resource contracts.
//! 供应商无关的资源契约。

use serde::{Deserialize, Serialize};

/// Identifies a resource media category.
/// 标识资源媒体类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// Image resource.
    /// 图片资源。
    Image,
    /// Audio resource.
    /// 音频资源。
    Audio,
    /// Video resource.
    /// 视频资源。
    Video,
    /// General file resource.
    /// 通用文件资源。
    File,
}

/// Identifies how one resource reference must be projected onto provider wire formats.
/// 标识一项资源引用必须如何投影到供应商 Wire 格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSourceKind {
    /// Host-resolved local path or URL used by existing internal resource preparation.
    /// 由既有内部资源准备流程使用的宿主已解析本地路径或 URL。
    #[default]
    HostResolved,
    /// Standard provider file identifier.
    /// 标准供应商文件标识。
    ProviderFileId,
    /// Standard remote URL field.
    /// 标准远程 URL 字段。
    RemoteUrl,
    /// Standard inline data field.
    /// 标准内联数据字段。
    InlineData,
}

/// Selects the standard image-input fidelity requested by the caller.
/// 选择调用方请求的标准图片输入保真度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageInputDetail {
    /// Let the provider select image fidelity.
    /// 由供应商选择图片保真度。
    Auto,
    /// Prefer lower-cost image processing.
    /// 优先使用较低成本的图片处理。
    Low,
    /// Prefer high-fidelity image processing.
    /// 优先使用高保真图片处理。
    High,
}

/// Describes one resource without exposing provider upload credentials.
/// 描述一个资源且不暴露供应商上传凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceReference {
    /// Stable Vulcan-side resource identifier.
    /// 稳定的 Vulcan 侧资源标识。
    pub id: String,
    /// Resource media category.
    /// 资源媒体类别。
    pub kind: ResourceKind,
    /// Optional MIME type.
    /// 可选 MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Exact source-field semantics retained from the standard request.
    /// 从标准请求保留的精确来源字段语义。
    #[serde(default)]
    pub source_kind: ResourceSourceKind,
    /// Optional caller-supplied file name required by inline file inputs.
    /// 内联文件输入所需的可选调用方文件名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    /// Optional standard image-input detail policy.
    /// 可选的标准图片输入细节策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_detail: Option<ImageInputDetail>,
    /// Optional local path supplied by the user or AI tool.
    /// 用户或 AI 工具提供的可选本地路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// Optional user-safe remote URL.
    /// 可选的用户安全远程 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Represents one resource transfer lifecycle state.
/// 表示一个资源传输生命周期状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ResourceTransferState {
    /// Transfer has not started.
    /// 传输尚未开始。
    Pending,
    /// Transfer is currently running.
    /// 传输正在进行。
    Running {
        /// Bytes processed so far.
        /// 当前已处理字节数。
        bytes_processed: u64,
        /// Optional total byte count.
        /// 可选总字节数。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bytes_total: Option<u64>,
    },
    /// Transfer completed successfully.
    /// 传输已成功完成。
    Completed,
    /// Transfer was cancelled.
    /// 传输已取消。
    Cancelled,
    /// Transfer failed.
    /// 传输失败。
    Failed {
        /// Controlled failure description.
        /// 受控失败说明。
        message: String,
    },
}
