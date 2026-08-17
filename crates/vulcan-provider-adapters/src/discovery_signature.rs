//! Discovery signature grouping for multi-protocol model discovery.
//! 多协议模型发现的发现签名分组。
//!
//! Routes with the same signature share exactly one discovery request; the result can then be
//! projected onto every protocol in the group. Distinct signatures discover separately and are
//! merged per model by identity (plan §14.2).
//! 签名相同的路由恰好共享一次发现请求，结果可投影到组内每个协议。不同签名分别发现，
//! 并按模型身份合并（方案 §14.2）。

use crate::{AuthenticationProjection, ModelDiscoveryKind};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Stable request-format version for discovery signatures.
/// 发现签名的稳定请求格式版本。
const DISCOVERY_FORMAT_VERSION: &str = "vulcan.discovery-format.v1";

/// Computes the canonical discovery signature for one protocol route.
/// 为一条协议路由计算规范发现签名。
///
/// # Parameters
/// 参数
/// `discovery_kind` is the effective discovery contract; `url` is the exact full discovery
/// URL; `credential_profile_id` identifies the credential profile; `authentication` is the
/// resolved projection (not the secret value); `fixed_headers` are the route-level headers.
/// `discovery_kind` 是生效发现契约；`url` 是精确完整发现 URL；`credential_profile_id` 标识
/// 凭据配置档；`authentication` 是已解析投影（非秘密值）；`fixed_headers` 是路由级请求头。
///
/// # Returns
/// 返回值
/// A SHA-256 hex digest that is stable across identical routes and distinct across any
/// difference in the signed material.
/// 返回跨相同路由稳定、对签名材料的任何差异都不同的 SHA-256 十六进制摘要。
pub fn discovery_signature(
    discovery_kind: ModelDiscoveryKind,
    url: &str,
    credential_profile_id: &str,
    authentication: &AuthenticationProjection,
    fixed_headers: &BTreeMap<String, String>,
) -> String {
    let material = serde_json::to_vec(&(
        discovery_kind,
        url,
        credential_profile_id,
        authentication,
        fixed_headers,
        DISCOVERY_FORMAT_VERSION,
    ))
    .expect("discovery signature material must serialize");
    format!("sha256:{}", hex::encode(Sha256::digest(material)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    /// Verifies identical routes share one signature and any material difference splits it.
    /// 验证相同路由共享一个签名，任何材料差异都拆分签名。
    #[test]
    fn signature_groups_identical_routes_and_splits_differences() {
        let base = headers(&[("x-route", "a")]);
        let auth = AuthenticationProjection::BearerAuthorization;
        let kind = ModelDiscoveryKind::OpenAi;
        let first = discovery_signature(
            kind,
            "https://api.example.com/v1/models",
            "api-key",
            &auth,
            &base,
        );
        let second = discovery_signature(
            kind,
            "https://api.example.com/v1/models",
            "api-key",
            &auth,
            &base,
        );
        assert_eq!(first, second);

        let different_url = discovery_signature(
            kind,
            "https://api.example.com/v1/models?region=eu",
            "api-key",
            &auth,
            &base,
        );
        assert_ne!(first, different_url);

        let different_profile = discovery_signature(
            kind,
            "https://api.example.com/v1/models",
            "team-key",
            &auth,
            &base,
        );
        assert_ne!(first, different_profile);

        let different_headers = discovery_signature(
            kind,
            "https://api.example.com/v1/models",
            "api-key",
            &auth,
            &headers(&[("x-route", "b")]),
        );
        assert_ne!(first, different_headers);

        let different_kind = discovery_signature(
            ModelDiscoveryKind::Anthropic,
            "https://api.example.com/v1/models",
            "api-key",
            &auth,
            &base,
        );
        assert_ne!(first, different_kind);
    }
}
