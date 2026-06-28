//! AI capability parsing helpers.
//! AI 能力 parsing 辅助逻辑。

use super::*;

/// Parses one AI capability vendor connection mode key.
/// 解析单个 AI 能力供应商连接模式键。
///
/// # Parameters
/// 参数
/// `value` is the optional raw connection-mode key from the WebUI payload.
/// `value` 是 WebUI 载荷中的可选原始连接模式键。
///
/// # Returns
/// 返回值
/// The parsed connection mode, or `None` when no explicit mode was submitted.
/// 解析后的连接模式；未提交显式模式时返回 `None`。
pub(crate) fn parse_ai_capability_connection_mode(
    value: Option<&str>,
) -> Result<Option<AiCapabilityVendorConnectionMode>, String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some("local") => Ok(Some(AiCapabilityVendorConnectionMode::Local)),
        Some("cloud") | Some("ollama-cloud") => Ok(Some(AiCapabilityVendorConnectionMode::Cloud)),
        Some(other) => Err(format!("不支持的 AI 能力连接模式：{other}")),
    }
}

/// Parses prompt-level AI capability target order overrides from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标顺序覆盖。
///
/// # Parameters
/// 参数
/// `raw_overrides` contains capability-keyed target selector arrays from the WebUI.
/// `raw_overrides` 包含 WebUI 传入的按能力键分组的目标选择器数组。
///
/// # Returns
/// 返回值
/// A semantic capability keyed map accepted by prompt and session capability routing.
/// 提示词与会话能力路由可接受的语义能力映射。
pub(crate) fn parse_ai_capability_target_order_overrides(
    raw_overrides: Option<BTreeMap<String, Vec<String>>>,
) -> Result<BTreeMap<AiToolKind, Vec<String>>, String> {
    let mut overrides = BTreeMap::new();
    for (raw_kind, selectors) in raw_overrides.unwrap_or_default() {
        let Some(kind) =
            vulcan_agent_runtime::workbench_ai_capability_router::parse_workbench_ai_tool_kind(
                &raw_kind,
            )
        else {
            return Err(format!("未知的扩展能力排序类型：{raw_kind}"));
        };
        let cleaned_selectors = selectors
            .into_iter()
            .map(|selector| selector.trim().to_string())
            .filter(|selector| !selector.is_empty())
            .collect::<Vec<_>>();
        if !cleaned_selectors.is_empty() {
            overrides.insert(kind, cleaned_selectors);
        }
    }
    Ok(overrides)
}

/// Parses prompt-level AI capability target selectors from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标选择器。
///
/// # Parameters
/// 参数
/// `raw_selectors` contains optional target selector strings from the WebUI.
/// `raw_selectors` 包含 WebUI 传入的可选目标选择器字符串。
///
/// # Returns
/// 返回值
/// A trimmed selector set accepted by prompt and session capability routing.
/// 提示词与会话能力路由可接受的已清洗选择器集合。
pub(crate) fn parse_ai_capability_target_selectors(
    raw_selectors: Option<Vec<String>>,
) -> BTreeSet<String> {
    raw_selectors
        .unwrap_or_default()
        .into_iter()
        .map(|selector| selector.trim().to_string())
        .filter(|selector| !selector.is_empty())
        .collect()
}
