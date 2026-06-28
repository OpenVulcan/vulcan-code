//! OpenRouter secondary route helpers.
//! OpenRouter 二级路由辅助逻辑。

use super::*;

pub(crate) fn openrouter_route_selection_from_service(
    selection: ProviderModelSecondaryRouteSelection,
) -> provider_config::OpenRouterEndpointRouteSelection {
    let tag = if selection.tag.trim().is_empty() {
        selection.route_id.trim().to_string()
    } else {
        selection.tag.trim().to_string()
    };
    provider_config::OpenRouterEndpointRouteSelection {
        tag,
        provider_name: selection.provider_name.trim().to_string(),
        endpoint_name: selection.endpoint_name.trim().to_string(),
    }
}

/// Projects a persisted OpenRouter route into the service secondary-route summary.
/// 将持久化 OpenRouter 路由投影为 service 二级路由摘要。
pub(crate) fn provider_secondary_route_summary_from_openrouter_selection(
    provider_id: &str,
    model_id: &str,
    selection: provider_config::OpenRouterEndpointRouteSelection,
    active: bool,
) -> ProviderModelSecondaryRouteSummary {
    let label = selection.display_label();
    ProviderModelSecondaryRouteSummary {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        route_id: selection.tag.clone(),
        tag: selection.tag,
        provider_name: selection.provider_name.clone(),
        endpoint_name: selection.endpoint_name.clone(),
        subtitle: if selection.endpoint_name.trim().is_empty() {
            String::from("已选择")
        } else {
            selection.endpoint_name
        },
        label,
        active,
    }
}

/// Minimal OpenRouter endpoint catalog response used by the WebUI secondary-route service.
/// WebUI 二级路由服务使用的最小 OpenRouter 端点目录响应。
#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelEndpointsResponse {
    /// Response payload body containing endpoint rows.
    /// 包含端点行的响应载荷主体。
    data: OpenRouterModelEndpointsData,
}

/// Minimal OpenRouter endpoint catalog payload.
/// 最小 OpenRouter 端点目录载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelEndpointsData {
    /// Endpoint rows available for the requested model.
    /// 目标模型可用的端点行。
    #[serde(default)]
    endpoints: Vec<OpenRouterModelEndpointRecord>,
}

/// Minimal OpenRouter endpoint record.
/// 最小 OpenRouter 端点记录。
#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelEndpointRecord {
    /// Stable endpoint tag forwarded to `provider.order`.
    /// 转发到 `provider.order` 的稳定端点标签。
    tag: Option<String>,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    provider_name: Option<String>,
    /// Human-readable endpoint name.
    /// 人类可读的端点名称。
    name: Option<String>,
    /// Optional recent latency summary.
    /// 可选近期延迟摘要。
    latency_last_30m: Option<Value>,
    /// Optional recent uptime summary.
    /// 可选近期可用性摘要。
    uptime_last_30m: Option<Value>,
    /// Optional recent throughput summary.
    /// 可选近期吞吐速度摘要。
    throughput_last_30m: Option<Value>,
    /// Optional pricing payload returned by OpenRouter.
    /// OpenRouter 返回的可选价格载荷。
    pricing: Option<OpenRouterEndpointPricingRecord>,
}

/// Minimal OpenRouter endpoint pricing payload.
/// 最小 OpenRouter 端点价格载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenRouterEndpointPricingRecord {
    /// Optional prompt price string.
    /// 可选输入价格字符串。
    prompt: Option<String>,
    /// Optional completion price string.
    /// 可选输出价格字符串。
    completion: Option<String>,
    /// Optional cache-read price string.
    /// 可选缓存读取价格字符串。
    input_cache_read: Option<String>,
}

/// Service-local OpenRouter endpoint choice before projection.
/// 投影前的 service 本地 OpenRouter 端点选项。
#[derive(Debug, Clone)]
pub(crate) struct OpenRouterSecondaryRouteChoice {
    /// Stable endpoint tag forwarded to OpenRouter.
    /// 转发到 OpenRouter 的稳定端点标签。
    pub(crate) tag: String,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    provider_name: String,
    /// Human-readable endpoint name.
    /// 人类可读的端点名称。
    endpoint_name: String,
    /// Optional prompt price in USD per token.
    /// 可选输入侧每 token 美元价格。
    prompt_price_per_token_usd: Option<f64>,
    /// Optional completion price in USD per token.
    /// 可选输出侧每 token 美元价格。
    completion_price_per_token_usd: Option<f64>,
    /// Optional cache-read price in USD per token.
    /// 可选缓存读取每 token 美元价格。
    cache_read_price_per_token_usd: Option<f64>,
    /// Optional latency in milliseconds.
    /// 可选延迟毫秒数。
    latency_last_30m_ms: Option<f64>,
    /// Optional uptime percentage.
    /// 可选可用性百分比。
    uptime_last_30m_percent: Option<f64>,
    /// Optional throughput in tokens per second.
    /// 可选每秒 token 吞吐速度。
    throughput_last_30m_tps: Option<f64>,
}

impl OpenRouterSecondaryRouteChoice {
    /// Converts this choice into one WebUI route summary.
    /// 将当前选项转换为 WebUI 路由摘要。
    pub(crate) fn into_summary(
        self,
        provider_id: &str,
        model_id: &str,
        active: bool,
    ) -> ProviderModelSecondaryRouteSummary {
        let label = if self.provider_name.trim().is_empty() {
            self.tag.clone()
        } else {
            self.provider_name.clone()
        };
        let subtitle = openrouter_secondary_route_subtitle(&self);
        ProviderModelSecondaryRouteSummary {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            route_id: self.tag.clone(),
            tag: self.tag,
            subtitle,
            provider_name: self.provider_name,
            endpoint_name: self.endpoint_name,
            label,
            active,
        }
    }
}

/// Loads the OpenRouter endpoint catalog for one exact model identifier.
/// 加载某个精确模型标识对应的 OpenRouter 端点目录。
pub(crate) async fn fetch_openrouter_secondary_route_choices(
    model_id: String,
    api_key: Option<String>,
) -> Result<Vec<OpenRouterSecondaryRouteChoice>, String> {
    let url = format!("https://openrouter.ai/api/v1/models/{model_id}/endpoints");
    let client = reqwest::Client::new();
    let mut request = client.get(url.clone());
    if let Some(api_key) = api_key.as_deref().filter(|value| !value.trim().is_empty()) {
        request = request.bearer_auth(api_key.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("请求 OpenRouter 二级供应商目录失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "请求 OpenRouter 二级供应商目录失败：HTTP {}",
            response.status()
        ));
    }
    let payload = response
        .json::<OpenRouterModelEndpointsResponse>()
        .await
        .map_err(|error| format!("解析 OpenRouter 二级供应商目录失败：{error}"))?;
    let mut choices = payload
        .data
        .endpoints
        .into_iter()
        .filter_map(openrouter_endpoint_record_to_secondary_route_choice)
        .collect::<Vec<_>>();
    choices.sort_by(|left, right| {
        left.provider_name
            .cmp(&right.provider_name)
            .then_with(|| left.tag.cmp(&right.tag))
    });
    choices.dedup_by(|left, right| left.tag == right.tag);
    Ok(choices)
}

/// Converts one raw OpenRouter endpoint record into a route choice.
/// 将单条原始 OpenRouter 端点记录转换为路由选项。
fn openrouter_endpoint_record_to_secondary_route_choice(
    record: OpenRouterModelEndpointRecord,
) -> Option<OpenRouterSecondaryRouteChoice> {
    let tag = record.tag?.trim().to_string();
    if tag.is_empty() {
        return None;
    }
    let provider_name = record
        .provider_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(tag.as_str())
        .to_string();
    let endpoint_name = record
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(provider_name.as_str())
        .to_string();
    Some(OpenRouterSecondaryRouteChoice {
        tag,
        provider_name,
        endpoint_name,
        prompt_price_per_token_usd: record
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.prompt.as_deref())
            .and_then(parse_openrouter_f64_text),
        completion_price_per_token_usd: record
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.completion.as_deref())
            .and_then(parse_openrouter_f64_text),
        cache_read_price_per_token_usd: record
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.input_cache_read.as_deref())
            .and_then(parse_openrouter_f64_text),
        latency_last_30m_ms: openrouter_value_as_f64(record.latency_last_30m.as_ref()),
        uptime_last_30m_percent: openrouter_value_as_f64(record.uptime_last_30m.as_ref()),
        throughput_last_30m_tps: openrouter_value_as_f64(record.throughput_last_30m.as_ref()),
    })
}

/// Builds the compact secondary line for one OpenRouter endpoint choice.
/// 为单个 OpenRouter 端点选项构建紧凑二级说明。
fn openrouter_secondary_route_subtitle(choice: &OpenRouterSecondaryRouteChoice) -> String {
    let mut parts = Vec::new();
    if let Some(prompt_price) =
        format_openrouter_price_per_mtokens(choice.prompt_price_per_token_usd)
    {
        parts.push(format!("输入 {prompt_price}"));
    }
    if let Some(completion_price) =
        format_openrouter_price_per_mtokens(choice.completion_price_per_token_usd)
    {
        parts.push(format!("输出 {completion_price}"));
    }
    if let Some(cache_read_price) =
        format_openrouter_price_per_mtokens(choice.cache_read_price_per_token_usd)
    {
        parts.push(format!("缓存 {cache_read_price}"));
    }
    if let Some(latency) = format_openrouter_latency_ms(choice.latency_last_30m_ms) {
        parts.push(format!("延迟 {latency}"));
    }
    if let Some(throughput) =
        format_openrouter_throughput_tokens_per_second(choice.throughput_last_30m_tps)
    {
        parts.push(format!("速度 {throughput}"));
    }
    if let Some(uptime) = format_openrouter_percentage(choice.uptime_last_30m_percent) {
        parts.push(format!("可用性 {uptime}"));
    }
    if parts.is_empty() {
        choice.endpoint_name.clone()
    } else {
        parts.join(" · ")
    }
}

/// Parses one OpenRouter numeric text field into a floating-point value.
/// 将单个 OpenRouter 数值文本字段解析为浮点数值。
pub(crate) fn parse_openrouter_f64_text(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<f64>().ok()
    }
}

/// Extracts one JSON numeric leaf into a floating-point value when possible.
/// 在可能的情况下，将单个 JSON 数值叶子提取为浮点数值。
pub(crate) fn openrouter_value_as_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => parse_openrouter_f64_text(text),
        Some(Value::Object(object)) => object
            .get("p50")
            .and_then(|percentile| openrouter_value_as_f64(Some(percentile))),
        _ => None,
    }
}

/// Formats one USD-per-token OpenRouter price as USD per million tokens.
/// 将 OpenRouter 每 token 美元价格格式化为每百万 token 美元价格。
pub(crate) fn format_openrouter_price_per_mtokens(value: Option<f64>) -> Option<String> {
    let price = value?;
    if !price.is_finite() {
        return None;
    }
    Some(format!("${:.3}/MTokens", price * 1_000_000.0))
}

/// Formats one OpenRouter latency value.
/// 格式化单个 OpenRouter 延迟值。
pub(crate) fn format_openrouter_latency_ms(value: Option<f64>) -> Option<String> {
    let latency = value?;
    latency.is_finite().then(|| format!("{latency:.0}ms"))
}

/// Formats one OpenRouter throughput value.
/// 格式化单个 OpenRouter 吞吐值。
pub(crate) fn format_openrouter_throughput_tokens_per_second(value: Option<f64>) -> Option<String> {
    let throughput = value?;
    throughput
        .is_finite()
        .then(|| format!("{throughput:.0} tok/s"))
}

/// Formats one OpenRouter percentage value.
/// 格式化单个 OpenRouter 百分比值。
pub(crate) fn format_openrouter_percentage(value: Option<f64>) -> Option<String> {
    let percentage = value?;
    percentage.is_finite().then(|| format!("{percentage:.1}%"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies OpenRouter endpoint records are normalized into secondary route choices.
    /// 验证 OpenRouter endpoint 原始记录会被归一化为二级路由选项。
    #[test]
    fn openrouter_endpoint_record_normalizes_secondary_route_choice() {
        let choice =
            openrouter_endpoint_record_to_secondary_route_choice(OpenRouterModelEndpointRecord {
                tag: Some(String::from("anthropic")),
                provider_name: Some(String::from("Anthropic")),
                name: Some(String::from("Claude direct")),
                latency_last_30m: Some(json!(123.4)),
                uptime_last_30m: Some(json!(99.8)),
                throughput_last_30m: Some(json!("42.5")),
                pricing: Some(OpenRouterEndpointPricingRecord {
                    prompt: Some(String::from("0.000003")),
                    completion: Some(String::from("0.000015")),
                    input_cache_read: Some(String::from("0.0000003")),
                }),
            })
            .expect("endpoint should become a route choice");

        assert_eq!(choice.tag, "anthropic");
        assert_eq!(choice.provider_name, "Anthropic");
        assert_eq!(choice.endpoint_name, "Claude direct");
        let subtitle = openrouter_secondary_route_subtitle(&choice);
        assert!(subtitle.contains("输入 $3.000/MTokens"));
        assert!(subtitle.contains("123ms"));
        assert!(subtitle.contains("99.8%"));
        assert!(subtitle.contains("42 tok/s"));
    }
}
