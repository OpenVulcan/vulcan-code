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
