//! Typed local summaries and route-bound opaque compaction checkpoints.
//! 类型化本地摘要与绑定路由的不透明压缩检查点。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_ai_contract::is_replayable_codex_responses_item;

use crate::v1::errors::DomainError;

/// Exclusive persisted context payload; diagnostic prose never substitutes for opaque output.
/// 互斥的持久上下文载荷；诊断文案绝不能替代不透明输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompactionContent {
    /// Exact completed local model summary, also present in its source event.
    /// 精确完成的本地模型摘要，同时存在于其来源事件。
    Summary {
        /// Unmodified semantic text used by the next context generation.
        /// 下一上下文代次使用的未修改语义文本。
        text: String,
    },
    /// Exact canonical remote output with its immutable replay route.
    /// 精确规范远端输出及其不可变重放路由。
    RemoteResponses {
        /// The provider checkpoint, never rendered as an assistant response.
        /// 供应商检查点，绝不渲染成助手回复。
        checkpoint: RemoteResponsesCheckpoint,
    },
}

/// Complete replay identity and canonical output returned by the compact endpoint.
/// compact 端点返回的完整重放身份与规范输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteResponsesCheckpoint {
    /// Exact provider identifier selected before dispatch.
    /// 分派前选定的精确供应商标识。
    pub provider_id: String,
    /// Exact non-secret credential connection identifier.
    /// 精确且不含秘密的凭据连接标识。
    pub connection_id: String,
    /// Exact ordinary Responses endpoint used to validate future native replay.
    /// 用于校验后续原生重放的精确普通 Responses 端点。
    pub route_fingerprint: String,
    /// Declared protocol profile; only Codex Responses produces this checkpoint.
    /// 声明的协议档案；只有 Codex Responses 产生此检查点。
    pub protocol_profile_id: String,
    /// Exact model identifier selected before dispatch.
    /// 分派前选定的精确模型标识。
    pub model_id: String,
    /// Ordered canonical provider items, including at least one opaque compaction item.
    /// 有序规范供应商项，至少包含一个不透明压缩项。
    pub output_items: Vec<Value>,
    /// Upstream response identifier when actually supplied; it is not a continuation token.
    /// 上游实际提供时的响应标识；它不是延续令牌。
    pub response_id: Option<String>,
}

impl RemoteResponsesCheckpoint {
    /// Checks `self` without rewriting stored bytes; returns the first invalid field.
    /// 检查`self`且不改写存储字节；返回首个无效字段。
    pub fn validate(&self) -> Result<(), DomainError> {
        for (field, value) in [
            ("provider_id", self.provider_id.as_str()),
            ("connection_id", self.connection_id.as_str()),
            ("route_fingerprint", self.route_fingerprint.as_str()),
            ("model_id", self.model_id.as_str()),
        ] {
            if value.is_empty() || value.trim() != value {
                return Err(invalid_checkpoint(field));
            }
        }
        // This persisted identity matches vulcan-ai-protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID.
        // 此持久身份与 vulcan-ai-protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID 一致。
        if self.protocol_profile_id != "openai-codex-responses.v1" {
            return Err(invalid_checkpoint("protocol_profile_id"));
        }
        if self
            .response_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.trim() != id)
        {
            return Err(invalid_checkpoint("response_id"));
        }
        if !self
            .output_items
            .iter()
            .all(is_replayable_codex_responses_item)
            || !self
                .output_items
                .iter()
                .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"))
        {
            return Err(invalid_checkpoint("output_items"));
        }
        Ok(())
    }
}

/// Creates a content-free domain error for an invalid checkpoint `field`.
/// 为无效检查点`field`创建不包含内容的领域错误。
fn invalid_checkpoint(field: &'static str) -> DomainError {
    DomainError::ImmutableFieldMutation {
        entity: "remote_compaction_checkpoint",
        field,
    }
}
