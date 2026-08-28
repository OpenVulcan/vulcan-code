//! Strongly typed V1 project-resource service contracts.
//! 强类型 V1 项目资源服务契约。

use serde::{Deserialize, Serialize};

/// Maximum ordered resource references accepted by one metadata request.
/// 单次元数据请求允许携带的最大有序资源引用数量。
pub const SESSION_RESOURCE_LIST_V1_MAX_ITEMS: usize = 256;

/// Requests display metadata for ordered project resources owned by one session.
/// 请求由一个会话所属项目拥有的有序资源展示元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionResourceListV1Request {
    /// Session identity resolved only through the V1 SQLite locator.
    /// 仅通过 V1 SQLite 定位器解析的会话身份。
    pub session_id: String,
    /// Ordered project resource ids; repeated ids remain repeated in the response.
    /// 有序项目资源 ID；重复 ID 在响应中仍保持重复。
    pub resource_ids: Vec<u64>,
}

/// Returns display metadata for ordered project resources owned by one session.
/// 返回由一个会话所属项目拥有的有序资源展示元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionResourceListV1Response {
    /// Exact session identity used to resolve the project owner.
    /// 用于解析项目所有者的精确会话身份。
    pub session_id: String,
    /// Resource results in the exact request order.
    /// 按请求精确顺序排列的资源结果。
    pub resources: Vec<SessionResourceSummaryV1>,
}

/// Browser-safe display projection of one project resource resolution.
/// 单个项目资源解析结果的浏览器安全展示投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "availability", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionResourceSummaryV1 {
    /// The immutable entity exists and passed integrity validation.
    /// 不可变实体存在并已通过完整性校验。
    Available {
        /// Project-scoped resource id.
        /// 项目级资源 ID。
        resource_id: u64,
        /// Original display file name.
        /// 原始展示文件名。
        original_name: String,
        /// Optional authoritative MIME type.
        /// 可选的权威 MIME 类型。
        mime: Option<String>,
        /// Exact entity byte size.
        /// 精确实体字节数。
        size: u64,
    },
    /// The catalog row or immutable entity is missing.
    /// 目录记录或不可变实体缺失。
    Missing {
        /// Project-scoped resource id.
        /// 项目级资源 ID。
        resource_id: u64,
        /// Fixed user-visible missing-resource placeholder.
        /// 固定的用户可见资源缺失占位符。
        placeholder: String,
    },
}

/// Internal host request for one controlled session-resource content response.
/// 单个受控会话资源内容响应的宿主内部请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResourceContentV1Request {
    /// Session identity carried by the opaque browser route.
    /// 不透明浏览器路由携带的会话身份。
    pub session_id: String,
    /// Project-scoped resource id carried by the opaque browser route.
    /// 不透明浏览器路由携带的项目级资源 ID。
    pub resource_id: u64,
    /// Whether only response headers are requested.
    /// 是否仅请求响应头。
    pub is_head: bool,
    /// Optional single HTTP byte range.
    /// 可选的单个 HTTP 字节范围。
    pub range_header: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies metadata requests reject fields outside the frozen V1 contract.
    /// 验证元数据请求会拒绝冻结 V1 契约之外的字段。
    #[test]
    fn resource_list_request_rejects_unknown_fields() {
        let error = serde_json::from_value::<SessionResourceListV1Request>(serde_json::json!({
            "sessionId": "session-a",
            "resourceIds": [1],
            "projectId": "must-not-be-client-owned"
        }))
        .expect_err("unknown physical ownership hints must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    /// Verifies available and missing projections preserve their exact tagged shapes.
    /// 验证可用与缺失投影会保留其精确的带标签结构。
    #[test]
    fn resource_summaries_round_trip_without_physical_fields() {
        let response = SessionResourceListV1Response {
            session_id: String::from("session-a"),
            resources: vec![
                SessionResourceSummaryV1::Available {
                    resource_id: 7,
                    original_name: String::from("diagram.png"),
                    mime: Some(String::from("image/png")),
                    size: 42,
                },
                SessionResourceSummaryV1::Missing {
                    resource_id: 9,
                    placeholder: String::from("[MISSING PROJECT RESOURCE]"),
                },
            ],
        };
        let value = serde_json::to_value(&response).expect("response should serialize");

        assert_eq!(value["resources"][0]["availability"], "available");
        assert_eq!(value["resources"][1]["availability"], "missing");
        assert!(value.to_string().find("entity_relative_path").is_none());
        assert!(value.to_string().find("provider_bindings").is_none());
        assert!(value.to_string().find("sha256").is_none());
        assert_eq!(
            serde_json::from_value::<SessionResourceListV1Response>(value)
                .expect("response should deserialize"),
            response
        );
    }
}
