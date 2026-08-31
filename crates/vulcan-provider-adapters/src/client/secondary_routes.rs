//! OpenRouter secondary-route metadata and typed endpoint projection.
//! OpenRouter 二级路由元数据与强类型端点投影。

use super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use crate::{SecondaryRouteOption, ServiceEntry};
use serde::Deserialize;
use serde_json::Value;

/// Queries OpenRouter endpoint metadata for one exact model.
/// 查询一个精确模型的 OpenRouter 端点元数据。
///
/// # Parameters
/// 参数
/// `http` is the shared client; `entry` supplies the configured base URL; `model_id` is exact;
/// `secret` is the selected credential.
/// `http` 是共享客户端；`entry` 提供配置基址；`model_id` 是精确模型标识；
/// `secret` 是选定凭据。
///
/// # Returns
/// 返回值
/// Stable, sorted, de-duplicated endpoint options.
/// 返回稳定排序并去重的端点选项。
pub(super) async fn query_openrouter_model_endpoints(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    model_id: &str,
    secret: &str,
) -> Result<Vec<SecondaryRouteOption>, String> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err(String::from(
            "OpenRouter secondary route discovery requires an exact model id.",
        ));
    }
    let url = format!(
        "{}/models/{model_id}/endpoints",
        entry.base_url.trim_end_matches('/')
    );
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            format!("OpenRouter secondary route request to `{url}` failed: {error}")
        })?;
    let status = response.status();
    let body = read_bounded_provider_metadata_body(
        response,
        "OpenRouter secondary route request",
        url.as_str(),
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "OpenRouter secondary route request to `{url}` returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let payload = parse_provider_metadata_json::<OpenRouterModelEndpointsResponse>(
        body.as_str(),
        "OpenRouter secondary route response",
    )?;
    let mut options = payload
        .data
        .endpoints
        .into_iter()
        .filter_map(openrouter_endpoint_option)
        .collect::<Vec<_>>();
    options.sort_by(|left, right| {
        left.provider_display_name
            .cmp(&right.provider_display_name)
            .then_with(|| left.route_id.cmp(&right.route_id))
    });
    options.dedup_by(|left, right| left.route_id == right.route_id);
    Ok(options)
}

/// Converts one OpenRouter endpoint record into the public secondary-route contract.
/// 将一条 OpenRouter 端点记录转换为公共二级路由契约。
///
/// # Parameters
/// 参数
/// `record` is one provider response row.
/// `record` 是一条供应商响应记录。
///
/// # Returns
/// 返回值
/// A normalized option when the stable tag exists, otherwise `None`.
/// 稳定标签存在时返回归一化选项，否则返回 `None`。
fn openrouter_endpoint_option(
    record: OpenRouterModelEndpointRecord,
) -> Option<SecondaryRouteOption> {
    let route_id = record.tag?.trim().to_string();
    if route_id.is_empty() {
        return None;
    }
    let provider_display_name = non_empty_text(record.provider_name.as_deref())
        .unwrap_or(route_id.as_str())
        .to_string();
    let endpoint_display_name = non_empty_text(record.name.as_deref())
        .unwrap_or(provider_display_name.as_str())
        .to_string();
    Some(SecondaryRouteOption {
        route_id,
        provider_display_name,
        endpoint_display_name,
        prompt_price_per_token: record
            .pricing
            .as_ref()
            .and_then(|value| value.prompt.clone()),
        completion_price_per_token: record
            .pricing
            .as_ref()
            .and_then(|value| value.completion.clone()),
        cache_read_price_per_token: record
            .pricing
            .as_ref()
            .and_then(|value| value.input_cache_read.clone()),
        latency_milliseconds: openrouter_metric(record.latency_last_30m.as_ref()),
        uptime_percent: openrouter_metric(record.uptime_last_30m.as_ref()),
        throughput_tokens_per_second: openrouter_metric(record.throughput_last_30m.as_ref()),
    })
}

/// Returns a trimmed non-empty text reference.
/// 返回去除首尾空白后的非空文本引用。
fn non_empty_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Reads one OpenRouter scalar or percentile metric.
/// 读取一个 OpenRouter 标量或百分位指标。
fn openrouter_metric(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
        Some(Value::Object(object)) => object
            .get("p50")
            .or_else(|| object.get("median"))
            .and_then(|value| openrouter_metric(Some(value))),
        _ => None,
    }
}

/// OpenRouter model-endpoints response envelope.
/// OpenRouter 模型端点响应信封。
#[derive(Debug, Deserialize)]
struct OpenRouterModelEndpointsResponse {
    /// Model endpoint collection.
    /// 模型端点集合。
    data: OpenRouterModelEndpointsData,
}

/// OpenRouter model-endpoints response body.
/// OpenRouter 模型端点响应正文。
#[derive(Debug, Deserialize)]
struct OpenRouterModelEndpointsData {
    /// Available endpoint records.
    /// 可用端点记录。
    #[serde(default)]
    endpoints: Vec<OpenRouterModelEndpointRecord>,
}

/// OpenRouter endpoint metadata used by secondary routing.
/// 二级路由使用的 OpenRouter 端点元数据。
#[derive(Debug, Deserialize)]
struct OpenRouterModelEndpointRecord {
    /// Stable route tag.
    /// 稳定路由标签。
    tag: Option<String>,
    /// Upstream provider display name.
    /// 上游供应商显示名。
    provider_name: Option<String>,
    /// Endpoint display name.
    /// 端点显示名。
    name: Option<String>,
    /// Recent latency metadata.
    /// 近期延迟元数据。
    latency_last_30m: Option<Value>,
    /// Recent uptime metadata.
    /// 近期可用性元数据。
    uptime_last_30m: Option<Value>,
    /// Recent throughput metadata.
    /// 近期吞吐元数据。
    throughput_last_30m: Option<Value>,
    /// Endpoint pricing metadata.
    /// 端点价格元数据。
    pricing: Option<OpenRouterEndpointPricingRecord>,
}

/// OpenRouter endpoint prices retained as exact strings.
/// 以精确字符串保留的 OpenRouter 端点价格。
#[derive(Debug, Deserialize)]
struct OpenRouterEndpointPricingRecord {
    /// Prompt price per token.
    /// 每 Token 输入价格。
    prompt: Option<String>,
    /// Completion price per token.
    /// 每 Token 输出价格。
    completion: Option<String>,
    /// Cache-read price per token.
    /// 每 Token 缓存读取价格。
    input_cache_read: Option<String>,
}
