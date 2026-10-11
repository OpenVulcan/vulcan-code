//! OpenRouter public secondary-route presentation helpers.
//! OpenRouter 公共二级路由展示辅助逻辑。

use super::*;

/// Builds the compact workbench subtitle for one public OpenRouter route option.
/// 为一个公共 OpenRouter 路由选项构建紧凑工作台副标题。
pub(crate) fn openrouter_secondary_route_option_subtitle(
    option: &vulcan_provider_adapters::SecondaryRouteOption,
) -> String {
    let mut parts = Vec::new();
    if let Some(price) = option
        .prompt_price_per_token
        .as_deref()
        .and_then(parse_openrouter_f64_text)
        .and_then(|value| format_openrouter_price_per_mtokens(Some(value)))
    {
        parts.push(format!("Input {price}"));
    }
    if let Some(price) = option
        .completion_price_per_token
        .as_deref()
        .and_then(parse_openrouter_f64_text)
        .and_then(|value| format_openrouter_price_per_mtokens(Some(value)))
    {
        parts.push(format!("Output {price}"));
    }
    if let Some(price) = option
        .cache_read_price_per_token
        .as_deref()
        .and_then(parse_openrouter_f64_text)
        .and_then(|value| format_openrouter_price_per_mtokens(Some(value)))
    {
        parts.push(format!("Cache {price}"));
    }
    if let Some(latency) = format_openrouter_latency_ms(option.latency_milliseconds) {
        parts.push(format!("Latency {latency}"));
    }
    if let Some(throughput) =
        format_openrouter_throughput_tokens_per_second(option.throughput_tokens_per_second)
    {
        parts.push(format!("Throughput {throughput}"));
    }
    if let Some(uptime) = format_openrouter_percentage(option.uptime_percent) {
        parts.push(format!("Uptime {uptime}"));
    }
    if parts.is_empty() {
        option.endpoint_display_name.clone()
    } else {
        parts.join(" · ")
    }
}

/// Projects one persisted public route selection into the workbench model summary.
/// 将一个持久化公共路由选择投影到工作台模型摘要。
pub(crate) fn provider_secondary_route_summary_from_public_selection(
    provider_id: &str,
    model_id: &str,
    selection: &vulcan_provider_adapters::SecondaryRouteSelection,
) -> Option<ProviderModelSecondaryRouteSummary> {
    let vulcan_provider_adapters::SecondaryRouteSelection::PinnedEndpoint {
        route_id,
        provider_display_name,
        endpoint_display_name,
    } = selection
    else {
        return None;
    };
    Some(ProviderModelSecondaryRouteSummary {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        route_id: route_id.clone(),
        tag: route_id.clone(),
        provider_name: provider_display_name.clone(),
        endpoint_name: endpoint_display_name.clone(),
        subtitle: endpoint_display_name.clone(),
        label: provider_display_name.clone(),
        active: true,
    })
}

/// Parses one OpenRouter numeric text field into a floating-point value.
/// 将单个 OpenRouter 数值文本字段解析为浮点数值。
fn parse_openrouter_f64_text(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<f64>().ok()
    }
}

/// Formats one USD-per-token OpenRouter price as USD per million tokens.
/// 将 OpenRouter 每 Token 美元价格格式化为每百万 Token 美元价格。
fn format_openrouter_price_per_mtokens(value: Option<f64>) -> Option<String> {
    let price = value?;
    if !price.is_finite() {
        return None;
    }
    Some(format!("${:.3}/MTokens", price * 1_000_000.0))
}

/// Formats one OpenRouter latency value.
/// 格式化单个 OpenRouter 延迟值。
fn format_openrouter_latency_ms(value: Option<f64>) -> Option<String> {
    let latency = value?;
    latency.is_finite().then(|| format!("{latency:.0}ms"))
}

/// Formats one OpenRouter throughput value.
/// 格式化单个 OpenRouter 吞吐值。
fn format_openrouter_throughput_tokens_per_second(value: Option<f64>) -> Option<String> {
    let throughput = value?;
    throughput
        .is_finite()
        .then(|| format!("{throughput:.0} tok/s"))
}

/// Formats one OpenRouter percentage value.
/// 格式化单个 OpenRouter 百分比值。
fn format_openrouter_percentage(value: Option<f64>) -> Option<String> {
    let percentage = value?;
    percentage.is_finite().then(|| format!("{percentage:.1}%"))
}

#[cfg(test)]
mod tests {
    //! Focused tests for public OpenRouter route presentation.
    //! 公共 OpenRouter 路由展示定向测试。

    use super::*;

    /// Verifies public route metrics produce a compact deterministic subtitle.
    /// 验证公共路由指标会生成紧凑且确定性的副标题。
    #[test]
    fn public_route_option_formats_metrics() {
        let option = vulcan_provider_adapters::SecondaryRouteOption {
            route_id: String::from("anthropic"),
            provider_display_name: String::from("Anthropic"),
            endpoint_display_name: String::from("Claude direct"),
            prompt_price_per_token: Some(String::from("0.000003")),
            completion_price_per_token: Some(String::from("0.000015")),
            cache_read_price_per_token: Some(String::from("0.0000003")),
            latency_milliseconds: Some(123.4),
            uptime_percent: Some(99.8),
            throughput_tokens_per_second: Some(42.5),
        };
        let subtitle = openrouter_secondary_route_option_subtitle(&option);
        assert!(subtitle.contains("Input $3.000/MTokens"));
        assert!(subtitle.contains("Latency 123ms"));
        assert!(subtitle.contains("Uptime 99.8%"));
        assert!(subtitle.contains("Throughput 42 tok/s"));
    }
}
