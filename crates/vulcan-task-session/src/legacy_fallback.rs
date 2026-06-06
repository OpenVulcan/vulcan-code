use crate::model::TaskSessionId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Read-only fallback used when an old task payload has no TaskSession directory.
/// 旧 task 载荷没有 TaskSession 目录时使用的只读降级结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LegacyTaskPayloadFallback {
    /// A new file-backed task session exists and can be opened.
    /// 存在可打开的新文件态子代理会话。
    TaskSessionDirectory {
        /// Task session identifier.
        /// 子代理会话标识符。
        task_session_id: TaskSessionId,
    },
    /// Only the persisted task result can be displayed.
    /// 只能显示已持久化的任务返回记录。
    PersistedResult {
        /// Public task identifier from the old payload.
        /// 旧载荷中的公开任务标识符。
        public_task_id: String,
        /// Optional persisted result summary.
        /// 可选的已持久化结果摘要。
        result_summary: Option<String>,
        /// Persisted result payload.
        /// 已持久化结果载荷。
        payload: Value,
    },
    /// No recoverable task-session data exists.
    /// 不存在可恢复的子代理会话数据。
    Unrecoverable {
        /// Public task identifier from the old payload.
        /// 旧载荷中的公开任务标识符。
        public_task_id: String,
        /// Read-only notice shown to the user.
        /// 展示给用户的只读提示。
        message: String,
    },
}

impl LegacyTaskPayloadFallback {
    /// Builds a fallback for a missing TaskSession directory without migrating old payloads.
    /// 在缺少 TaskSession 目录时构建降级结果且不迁移旧载荷。
    pub fn from_missing_task_session(
        public_task_id: impl Into<String>,
        persisted_result: Option<Value>,
    ) -> Self {
        let public_task_id = public_task_id.into();
        if let Some(payload) = persisted_result {
            return Self::PersistedResult {
                public_task_id,
                result_summary: payload
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                payload,
            };
        }

        Self::Unrecoverable {
            public_task_id,
            message: "子代理会话数据不可复看，仅保留父会话中的任务记录。".to_string(),
        }
    }
}
