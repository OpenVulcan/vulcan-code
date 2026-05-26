//! Workbench model and OpenRouter state extracted from the main workbench module.
//! 从主工作台模块中抽取的工作台模型与 OpenRouter 状态。

use crate::provider_config::{OpenRouterEndpointRouteSelection, ReasoningEffortLevel};
use serde::Deserialize;
use serde_json::Value;

/// Workbench-scoped model option that keeps the visible model name and the owning provider label together.
/// 工作台作用域内的模型选项，同时保留可见模型名称与所属供应商标签。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkbenchModelChoice {
    /// Stable provider identifier owning the current model option.
    /// 拥有当前模型选项的稳定供应商标识。
    pub(super) provider_id: String,
    /// Human-readable provider label shown together with the chosen model.
    /// 与已选模型一起展示的人类可读供应商标签。
    pub(super) provider_label: String,
    /// Stable model identifier persisted into the shared configuration file.
    /// 持久化到共享配置文件中的稳定模型标识。
    pub(super) model_id: String,
    /// Human-readable model name rendered inside the workbench trigger surface.
    /// 渲染在工作台触发表面内部的人类可读模型名称。
    pub(super) model_name: String,
}

impl std::fmt::Display for WorkbenchModelChoice {
    /// Renders the visible model-name label shown inside the workbench trigger surface.
    /// 渲染显示在工作台触发表面内部的可见模型名称标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.model_name)
    }
}

/// Lightweight OpenRouter endpoint choice rendered inside the workbench secondary-provider picker.
/// 渲染在工作台二级供应商选择器中的轻量 OpenRouter 端点选项。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct WorkbenchOpenRouterEndpointChoice {
    /// Stable endpoint tag forwarded to OpenRouter provider routing.
    /// 转发到 OpenRouter Provider 路由中的稳定端点标签。
    pub(super) tag: String,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    pub(super) provider_name: String,
    /// Human-readable endpoint name returned by the OpenRouter catalog.
    /// OpenRouter 目录返回的人类可读端点名称。
    pub(super) endpoint_name: String,
    /// Optional prompt-side unit pricing in USD per token.
    /// 可选的输入侧每 token 美元单价。
    pub(super) prompt_price_per_token_usd: Option<f64>,
    /// Optional completion-side unit pricing in USD per token.
    /// 可选的输出侧每 token 美元单价。
    pub(super) completion_price_per_token_usd: Option<f64>,
    /// Optional cache-read unit pricing in USD per token.
    /// 可选的缓存读取每 token 美元单价。
    pub(super) cache_read_price_per_token_usd: Option<f64>,
    /// Optional recent latency summary returned by OpenRouter.
    /// OpenRouter 返回的可选近期延迟摘要。
    pub(super) latency_last_30m_ms: Option<f64>,
    /// Optional recent uptime summary returned by OpenRouter as a percentage value.
    /// OpenRouter 返回的可选近期可用性百分比数值。
    pub(super) uptime_last_30m_percent: Option<f64>,
    /// Optional recent throughput summary returned by OpenRouter in tokens per second.
    /// OpenRouter 返回的可选近期平均吞吐速度（每秒 token 数）。
    pub(super) throughput_last_30m_tps: Option<f64>,
}

impl WorkbenchOpenRouterEndpointChoice {
    /// Converts the UI choice into the persisted route selection payload.
    /// 将界面选项转换为持久化路由选择载荷。
    pub(super) fn to_route_selection(&self) -> OpenRouterEndpointRouteSelection {
        OpenRouterEndpointRouteSelection {
            tag: self.tag.clone(),
            provider_name: self.provider_name.clone(),
            endpoint_name: self.endpoint_name.clone(),
        }
    }

    /// Returns the compact subtitle line rendered under the provider title.
    /// 返回渲染在供应商标题下方的紧凑副标题文本。
    pub(super) fn subtitle_line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(prompt_price) =
            format_openrouter_price_per_mtokens(self.prompt_price_per_token_usd)
        {
            parts.push(format!("输入 {prompt_price}"));
        }
        if let Some(completion_price) =
            format_openrouter_price_per_mtokens(self.completion_price_per_token_usd)
        {
            parts.push(format!("输出 {completion_price}"));
        }
        if let Some(cache_read_price) =
            format_openrouter_price_per_mtokens(self.cache_read_price_per_token_usd)
        {
            parts.push(format!("缓存 {cache_read_price}"));
        }
        if let Some(latency) = format_openrouter_latency_ms(self.latency_last_30m_ms) {
            parts.push(format!("延迟 {latency}"));
        }
        if let Some(throughput) =
            format_openrouter_throughput_tokens_per_second(self.throughput_last_30m_tps)
        {
            parts.push(format!("速度 {throughput}"));
        }
        if let Some(uptime) = format_openrouter_percentage(self.uptime_last_30m_percent) {
            parts.push(format!("可用性 {uptime}"));
        }
        if parts.is_empty() {
            self.endpoint_name.clone()
        } else {
            parts.join(" · ")
        }
    }
}

/// In-memory OpenRouter endpoint picker state shown above the main workbench.
/// 显示在主工作台之上的内存态 OpenRouter 端点选择弹窗状态。
#[derive(Debug, Clone, Default)]
pub(super) struct WorkbenchOpenRouterEndpointPickerState {
    /// Whether the blocking picker modal is currently visible.
    /// 当前阻塞式选择弹窗是否可见。
    pub(super) open: bool,
    /// Stable model identifier whose endpoints are currently displayed.
    /// 当前正在显示端点列表的稳定模型标识。
    pub(super) model_id: String,
    /// Free-text filter applied inside the endpoint picker.
    /// 应用于端点选择弹窗内部的自由文本过滤值。
    pub(super) filter: String,
    /// Whether the picker is currently waiting for the remote catalog response.
    /// 该选择弹窗当前是否正在等待远端目录响应。
    pub(super) loading: bool,
    /// Optional remote loading error rendered inside the picker surface.
    /// 渲染在选择弹窗表面内部的可选远端加载错误。
    pub(super) error: Option<String>,
    /// Loaded endpoint choices for the active OpenRouter model.
    /// 当前 OpenRouter 模型已加载的端点选项集合。
    pub(super) choices: Vec<WorkbenchOpenRouterEndpointChoice>,
}

/// Minimal OpenRouter endpoint catalog response used by the workbench secondary-provider picker.
/// 被工作台二级供应商选择弹窗使用的最小 OpenRouter 端点目录响应结构。
#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelEndpointsResponse {
    /// Response payload body containing the endpoint array.
    /// 包含端点数组的响应载荷主体。
    data: OpenRouterModelEndpointsData,
}

/// Minimal OpenRouter endpoint catalog payload used by the workbench secondary-provider picker.
/// 被工作台二级供应商选择弹窗使用的最小 OpenRouter 端点目录载荷结构。
#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelEndpointsData {
    /// Endpoint rows available for the requested model.
    /// 为目标模型返回的可用端点行集合。
    #[serde(default)]
    endpoints: Vec<OpenRouterModelEndpointRecord>,
}

/// Minimal OpenRouter endpoint record used by the workbench secondary-provider picker.
/// 被工作台二级供应商选择弹窗使用的最小 OpenRouter 端点记录结构。
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
    /// 可选的近期延迟摘要。
    latency_last_30m: Option<Value>,
    /// Optional recent uptime summary.
    /// 可选的近期可用性摘要。
    uptime_last_30m: Option<Value>,
    /// Optional recent throughput summary.
    /// 可选的近期吞吐速度摘要。
    throughput_last_30m: Option<Value>,
    /// Optional pricing payload returned by OpenRouter.
    /// OpenRouter 返回的可选价格载荷。
    pricing: Option<OpenRouterEndpointPricingRecord>,
}

/// Minimal OpenRouter endpoint pricing payload used by the workbench secondary-provider picker.
/// 被工作台二级供应商选择弹窗使用的最小 OpenRouter 端点价格载荷结构。
#[derive(Debug, Clone, Deserialize)]
struct OpenRouterEndpointPricingRecord {
    /// Optional prompt price string.
    /// 可选的输入价格字符串。
    prompt: Option<String>,
    /// Optional completion price string.
    /// 可选的输出价格字符串。
    completion: Option<String>,
    /// Optional cache-read price string.
    /// 可选的缓存读取价格字符串。
    input_cache_read: Option<String>,
}

/// Loads the OpenRouter endpoint catalog for one exact model identifier.
/// 加载某个精确模型标识对应的 OpenRouter 端点目录。
pub(super) async fn fetch_openrouter_endpoint_choices_for_model(
    model_id: String,
    api_key: Option<String>,
) -> Result<Vec<WorkbenchOpenRouterEndpointChoice>, String> {
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
        .filter_map(openrouter_endpoint_record_to_choice)
        .collect::<Vec<_>>();
    choices.sort_by(|left, right| {
        left.provider_name
            .cmp(&right.provider_name)
            .then_with(|| left.tag.cmp(&right.tag))
    });
    choices.dedup_by(|left, right| left.tag == right.tag);
    Ok(choices)
}

/// Converts one raw OpenRouter endpoint record into a UI choice row when the record still exposes a stable provider tag.
/// 当原始记录仍暴露稳定供应商标签时，将单条 OpenRouter 端点记录转换为一条界面选项行。
fn openrouter_endpoint_record_to_choice(
    record: OpenRouterModelEndpointRecord,
) -> Option<WorkbenchOpenRouterEndpointChoice> {
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

    Some(WorkbenchOpenRouterEndpointChoice {
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
        latency_last_30m_ms: value_as_f64(record.latency_last_30m.as_ref()),
        uptime_last_30m_percent: value_as_f64(record.uptime_last_30m.as_ref()),
        throughput_last_30m_tps: value_as_f64(record.throughput_last_30m.as_ref()),
    })
}

/// Parses one OpenRouter numeric text field into a floating-point value.
/// 将单个 OpenRouter 数值文本字段解析为浮点数值。
fn parse_openrouter_f64_text(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

/// Extracts one JSON numeric leaf into a floating-point value when possible.
/// 在可能的情况下，将单个 JSON 数值叶子提取为浮点数值。
pub(super) fn value_as_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => parse_openrouter_f64_text(text),
        Some(Value::Object(object)) => object
            .get("p50")
            .and_then(|percentile| value_as_f64(Some(percentile))),
        _ => None,
    }
}

/// Formats one USD-per-token price into the shared USD-per-million-tokens label.
/// 将单个每 token 美元价格格式化为统一的每百万 tokens 美元标签。
pub(super) fn format_openrouter_price_per_mtokens(
    price_per_token_usd: Option<f64>,
) -> Option<String> {
    let price_per_token_usd = price_per_token_usd?;
    if !price_per_token_usd.is_finite() || price_per_token_usd < 0.0 {
        return None;
    }
    Some(format!("${:.2}/MTokens", price_per_token_usd * 1_000_000.0))
}

/// Formats one percentage-like numeric field with two decimal places.
/// 将单个百分比数值字段格式化为保留两位小数的百分比文本。
pub(super) fn format_openrouter_percentage(value: Option<f64>) -> Option<String> {
    let value = value?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(format!("{value:.2}%"))
}

/// Formats one latency metric into milliseconds text.
/// 将单个延迟指标格式化为毫秒文本。
pub(super) fn format_openrouter_latency_ms(value: Option<f64>) -> Option<String> {
    let value = value?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(format!("{value:.0}ms"))
}

/// Formats one throughput metric into tokens-per-second text.
/// 将单个吞吐速度指标格式化为每秒 tokens 文本。
pub(super) fn format_openrouter_throughput_tokens_per_second(value: Option<f64>) -> Option<String> {
    let value = value?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(format!("{value:.2} tok/s"))
}

/// Shared workbench reasoning-effort alias reused by the application state and compact picker surfaces.
/// 供应用状态与紧凑选择器界面复用的共享工作台推理档位别名。
pub(super) type WorkbenchReasoningEffort = ReasoningEffortLevel;
