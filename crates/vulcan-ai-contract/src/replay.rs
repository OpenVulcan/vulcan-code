//! Durable provider-neutral assistant replay state.
//! 可持久化的供应商无关助手重放状态。

use crate::ContentItem;
use serde::{Deserialize, Serialize};

/// Current durable replay envelope format version.
/// 当前可持久化重放封套格式版本。
pub const PROVIDER_REPLAY_ENVELOPE_VERSION: u16 = 1;

/// Preserves ordered assistant content together with its exact protocol scope.
/// 在精确协议作用域内保存有序助手内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderReplayEnvelope {
    /// Closed envelope format version.
    /// 封闭封套格式版本。
    pub version: u16,
    /// Provider definition identifier that produced the content.
    /// 产生内容的供应商定义标识。
    pub provider_id: String,
    /// Protocol profile identifier that produced provider-specific signatures.
    /// 产生供应商专属签名的协议 Profile 标识。
    pub protocol_profile_id: String,
    /// Upstream model identifier that produced the content.
    /// 产生内容的上游模型标识。
    pub model_id: String,
    /// Ordered provider-neutral assistant content.
    /// 有序的供应商无关助手内容。
    pub content: Vec<ContentItem>,
}

impl ProviderReplayEnvelope {
    /// Creates one versioned replay envelope for an exact execution route.
    /// 为一条精确执行路由创建一个带版本的重放封套。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `protocol_profile_id`, and `model_id` define the immutable route scope;
    /// `content` contains assistant output in original stream order.
    /// `provider_id`、`protocol_profile_id` 与 `model_id` 定义不可变路由作用域；
    /// `content` 按原始流顺序保存助手输出。
    ///
    /// # Returns
    /// 返回值
    /// A replay envelope using the current closed format version.
    /// 返回使用当前封闭格式版本的重放封套。
    pub fn new(
        provider_id: impl Into<String>,
        protocol_profile_id: impl Into<String>,
        model_id: impl Into<String>,
        content: Vec<ContentItem>,
    ) -> Self {
        Self {
            version: PROVIDER_REPLAY_ENVELOPE_VERSION,
            provider_id: provider_id.into(),
            protocol_profile_id: protocol_profile_id.into(),
            model_id: model_id.into(),
            content,
        }
    }

    /// Verifies that replay state belongs to the exact active execution route.
    /// 验证重放状态属于当前精确执行路由。
    ///
    /// # Parameters
    /// 参数
    /// The three identifiers are the active provider, protocol profile, and model.
    /// 三个标识分别是当前供应商、协议 Profile 与模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the current version and an exact route match.
    /// 仅当前版本且路由完全匹配时返回 `true`。
    pub fn matches_route(
        &self,
        provider_id: &str,
        protocol_profile_id: &str,
        model_id: &str,
    ) -> bool {
        self.version == PROVIDER_REPLAY_ENVELOPE_VERSION
            && self.provider_id == provider_id
            && self.protocol_profile_id == protocol_profile_id
            && self.model_id == model_id
    }
}
