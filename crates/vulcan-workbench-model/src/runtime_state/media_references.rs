//! Provider-media reference reuse scopes declared by upload channels.
//! 上传通道声明的供应商媒体引用复用作用域。

use serde::{Deserialize, Serialize};

/// Defines the smallest route scope that may reuse one provider-owned media reference.
/// 定义可以复用供应商托管媒体引用的最小路由作用域。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMediaReferenceScope {
    /// Reuse across models and credential connections of one provider service and upload contract.
    /// 在同一供应商服务与上传契约内跨模型、跨凭据连接复用。
    #[default]
    ProviderProtocol,
    /// Reuse only while the exact credential connection remains selected.
    /// 仅在精确凭据连接保持选中时复用。
    Credential,
    /// Reuse only inside the owning session.
    /// 仅在所属会话内部复用。
    Session,
}

impl SessionMediaReferenceScope {
    /// Returns the stable serialized scope label used by diagnostics and key construction.
    /// 返回用于诊断与键构造的稳定序列化作用域标签。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderProtocol => "provider_protocol",
            Self::Credential => "credential",
            Self::Session => "session",
        }
    }
}
