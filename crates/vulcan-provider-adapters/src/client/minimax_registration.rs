//! Reviewed MiniMax key registration and exact regional classification.
//! 已审核的 MiniMax Key 注册与精确区域分类。

use super::HttpProviderMetadataClient;
use super::http::{PROVIDER_METADATA_REQUEST_TIMEOUT, read_bounded_model_discovery_body};
use super::model_discovery::validate_discovered_catalog_bounds;
use crate::parse_openai_model_catalog;

/// Resolves the immutable MiniMax registration dimensions derived from one user key.
/// 表示从一个用户 Key 解析出的不可变 MiniMax 注册维度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniMaxKeyRegistration {
    /// Exact regional service-entry identifier.
    /// 精确的区域服务入口标识。
    pub service_entry_id: String,
    /// Exact credential-profile identifier derived from the reviewed key prefix.
    /// 从已审核 Key 前缀解析出的精确凭据配置档标识。
    pub credential_profile_id: String,
}

impl HttpProviderMetadataClient {
    /// Classifies one MiniMax key and probes its exact region without model inference.
    /// 对一个 MiniMax Key 分类并在不进行模型推理的情况下探测其精确区域。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the complete user-managed MiniMax key retained only for the requests.
    /// `key` 是完整的用户管理 MiniMax Key，仅用于请求。
    ///
    /// # Returns
    /// 返回值
    /// The exact regional entry and billing profile when exactly one catalog accepts the key.
    /// 当且仅当一个区域目录接受 Key 时返回精确区域入口与计费配置档。
    pub async fn register_minimax_key(&self, key: &str) -> Result<MiniMaxKeyRegistration, String> {
        let credential_profile_id = minimax_key_profile_id(key)?;
        let (global, china) = tokio::join!(
            probe_minimax_region(&self.http, "https://api.minimax.io", key),
            probe_minimax_region(&self.http, "https://api.minimaxi.com", key),
        );
        let service_entry_id = resolve_minimax_region(global, china)?;
        Ok(MiniMaxKeyRegistration {
            service_entry_id: service_entry_id.to_string(),
            credential_profile_id: credential_profile_id.to_string(),
        })
    }
}

/// Resolves a MiniMax region only from two explicit catalog probe outcomes.
/// 仅根据两个明确的目录探测结果解析 MiniMax 区域。
///
/// # Parameters
/// 参数
/// `global` and `china` are independent reviewed regional probe results.
/// `global` 与 `china` 是相互独立且已审核的区域探测结果。
///
/// # Returns
/// 返回值
/// One exact entry identifier only when exactly one region accepts the key.
/// 仅当一个区域接受 Key 时返回一个精确入口标识。
fn resolve_minimax_region(
    global: Result<(), String>,
    china: Result<(), String>,
) -> Result<&'static str, String> {
    match (global, china) {
        (Ok(()), Err(_)) => Ok("global"),
        (Err(_), Ok(())) => Ok("cn"),
        (Ok(()), Ok(())) => Err(String::from(
            "MiniMax key was accepted by both regional catalogs; registration is ambiguous.",
        )),
        (Err(global_error), Err(china_error)) => Err(format!(
            "MiniMax key was rejected by both regional catalogs. International: {global_error} China: {china_error}"
        )),
    }
}

/// Classifies the exact reviewed MiniMax key prefixes.
/// 对精确且已审核的 MiniMax Key 前缀进行分类。
///
/// # Parameters
/// 参数
/// `key` is the complete user-managed MiniMax credential.
/// `key` 是完整的用户管理 MiniMax 凭据。
///
/// # Returns
/// 返回值
/// The exact credential profile or a controlled unsupported-prefix failure.
/// 返回精确凭据配置档或受控的不支持前缀错误。
fn minimax_key_profile_id(key: &str) -> Result<&'static str, String> {
    if key.starts_with("sk-api-") {
        Ok("payg-api-key")
    } else if key.starts_with("sk-cp-") {
        Ok("token-plan-key")
    } else {
        Err(String::from(
            "MiniMax key must use the reviewed `sk-api-` or `sk-cp-` prefix.",
        ))
    }
}

/// Probes one MiniMax regional model catalog using the reviewed Bearer projection.
/// 使用已审核的 Bearer 投影探测一个 MiniMax 区域模型目录。
///
/// # Parameters
/// 参数
/// `http` is the shared client, `base_url` is a fixed reviewed region, and `key` is user secret material.
/// `http` 是共享客户端，`base_url` 是固定已审核区域，`key` 是用户秘密材料。
///
/// # Returns
/// 返回值
/// Success only for HTTP success containing a valid non-empty OpenAI-compatible catalog.
/// 仅 HTTP 成功且包含有效非空 OpenAI 兼容目录时返回成功。
async fn probe_minimax_region(
    http: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Result<(), String> {
    let response = http
        .get(format!("{base_url}/v1/models"))
        .bearer_auth(key)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("catalog request failed: {error}"))?;
    let status = response.status();
    let url = format!("{base_url}/v1/models");
    let body_result =
        read_bounded_model_discovery_body(response, "MiniMax region probe", url.as_str()).await;
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let body = body_result.map_err(|error| error.message)?;
    let catalog = validate_discovered_catalog_bounds(
        parse_openai_model_catalog("minimax-region-probe", 1, body.as_str())
            .map_err(|error| format!("invalid catalog: {error}"))?,
        "minimax-region-probe",
    )
    .map_err(|error| error.message)?;
    if catalog.models.is_empty() {
        return Err(String::from("catalog contained no models"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
