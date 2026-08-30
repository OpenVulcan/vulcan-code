//! Strongly typed V1 conversation edit and fork contracts.
//! 强类型 V1 会话编辑与分叉契约。

use serde::{Deserialize, Serialize};

/// Replaces one persisted root input by publishing a new event generation.
/// 通过发布新的事件代次替换一条已持久化根输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationEditGenerationV1Request {
    /// Exact V1 session identity resolved through the SQLite locator.
    /// 通过 SQLite 定位器解析的精确 V1 会话身份。
    pub session_id: String,
    /// Segment generation observed by the event-native WebUI before editing.
    /// 事件原生 WebUI 在编辑前观察到的分片代次。
    pub expected_window_generation: u64,
    /// Persisted seq of the root user input being replaced.
    /// 被替换根用户输入的持久化 seq。
    pub target_seq: u64,
    /// Exact replacement text; project resources remain the target event's ordered references.
    /// 精确替换正文；项目资源保持目标事件原有的有序引用。
    pub replacement_text: String,
}

/// Confirms one fully published and strictly reloaded edited generation.
/// 确认一个已完整发布并严格重载的编辑代次。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationEditGenerationV1Response {
    /// Edited V1 session identity.
    /// 已编辑的 V1 会话身份。
    pub session_id: String,
    /// Rewritten root input seq retained as the open Turn boundary.
    /// 保留为开放回合边界的已重写根输入 seq。
    pub target_seq: u64,
    /// Newly published event-window generation.
    /// 新发布的事件窗口代次。
    pub window_generation: u64,
    /// Complete document revision of the published generation.
    /// 已发布代次的完整文档修订号。
    pub document_revision: u64,
    /// Whether the exact open root Turn entered process-local physical dispatch.
    /// 精确开放根回合是否进入进程内物理分发。
    pub dispatch_started: bool,
}

/// Creates or exactly retries one immutable child session at a completed-Turn boundary.
/// 在完整回合结束边界创建或精确重试一个不可变子会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationForkGenerationV1Request {
    /// Exact parent V1 session identity.
    /// 精确父 V1 会话身份。
    pub source_session_id: String,
    /// Parent event generation observed before the fork action.
    /// 分叉动作前观察到的父事件代次。
    pub expected_window_generation: u64,
    /// Parent terminal TurnStats seq retained as the child's inclusive history boundary.
    /// 作为子会话包含式历史边界保留的父级终态 TurnStats seq。
    pub source_turn_end_seq: u64,
    /// Stable browser-generated child identity reused after response loss.
    /// 响应丢失后复用的稳定浏览器生成子会话身份。
    pub child_session_id: String,
    /// Exact child title used by both first creation and immutable retry validation.
    /// 首次创建与不可变重试校验共同使用的精确子标题。
    pub title: String,
}

/// Confirms one newly created or exactly retried forked V1 child.
/// 确认一个新建或精确重试的分叉 V1 子会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationForkGenerationV1Response {
    /// Exact parent V1 session identity.
    /// 精确父 V1 会话身份。
    pub source_session_id: String,
    /// Stable child V1 session identity.
    /// 稳定子 V1 会话身份。
    pub child_session_id: String,
    /// Whether this request physically created the child rather than validating a retry.
    /// 当前请求是否物理创建子会话，而非校验一次重试。
    pub created: bool,
    /// Parent terminal TurnStats seq retained by the child.
    /// 子会话保留的父级终态 TurnStats seq。
    pub source_turn_end_seq: u64,
    /// Child event-window generation.
    /// 子会话事件窗口代次。
    pub window_generation: u64,
    /// Child complete document revision.
    /// 子会话完整文档修订号。
    pub document_revision: u64,
}

#[cfg(test)]
mod tests {
    //! Strict edit and fork wire-contract tests.
    //! 严格编辑与分叉线协议契约测试。

    use super::{
        ConversationEditGenerationV1Request, ConversationForkGenerationV1Request,
        ConversationForkGenerationV1Response,
    };

    /// Rejects unknown edit fields so no legacy message identity can silently re-enter V1.
    /// 拒绝未知编辑字段，防止旧消息身份静默重新进入 V1。
    #[test]
    fn edit_request_rejects_legacy_message_identity() {
        let error =
            serde_json::from_value::<ConversationEditGenerationV1Request>(serde_json::json!({
                "sessionId": "4c751b3e-b8de-4ae1-866a-46fa4a093404",
                "expectedWindowGeneration": 1,
                "targetSeq": 1,
                "replacementText": "replacement",
                "messageId": "legacy-message"
            }))
            .expect_err("legacy message identity must be rejected");
        assert!(error.to_string().contains("unknown field"));
    }

    /// Rejects root-input fork anchors so only one completed-Turn boundary can enter V1.
    /// 拒绝根输入分叉锚点，确保只有完整回合边界可以进入 V1。
    #[test]
    fn fork_request_rejects_legacy_anchor_message() {
        let error =
            serde_json::from_value::<ConversationForkGenerationV1Request>(serde_json::json!({
                "sourceSessionId": "4c751b3e-b8de-4ae1-866a-46fa4a093404",
                "expectedWindowGeneration": 1,
                "sourceTurnEndSeq": 8,
                "childSessionId": "8da7f088-04da-444b-918b-a52001dfc3d1",
                "title": "fork",
                "targetSeq": 1
            }))
            .expect_err("root-input fork anchor must be rejected");
        assert!(error.to_string().contains("unknown field"));
    }

    /// Fork wire contracts expose only a completed-Turn anchor and no dispatch acknowledgement.
    /// 分叉线协议只暴露完整回合锚点，且不包含分发确认。
    #[test]
    fn fork_wire_contract_uses_completed_turn_without_dispatch() {
        let request = ConversationForkGenerationV1Request {
            source_session_id: String::from("4c751b3e-b8de-4ae1-866a-46fa4a093404"),
            expected_window_generation: 1,
            source_turn_end_seq: 8,
            child_session_id: String::from("8da7f088-04da-444b-918b-a52001dfc3d1"),
            title: String::from("fork"),
        };
        let response = ConversationForkGenerationV1Response {
            source_session_id: request.source_session_id.clone(),
            child_session_id: request.child_session_id.clone(),
            created: true,
            source_turn_end_seq: request.source_turn_end_seq,
            window_generation: 2,
            document_revision: 3,
        };

        assert_eq!(
            serde_json::to_value(request).expect("serialize completed-Turn fork request"),
            serde_json::json!({
                "sourceSessionId": "4c751b3e-b8de-4ae1-866a-46fa4a093404",
                "expectedWindowGeneration": 1,
                "sourceTurnEndSeq": 8,
                "childSessionId": "8da7f088-04da-444b-918b-a52001dfc3d1",
                "title": "fork",
            })
        );
        let response_json =
            serde_json::to_value(response).expect("serialize dormant fork response");
        assert_eq!(response_json["sourceTurnEndSeq"], 8);
        assert!(response_json.get("dispatchStarted").is_none());
    }
}
