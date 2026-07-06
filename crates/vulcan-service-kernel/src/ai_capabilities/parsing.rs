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

/// Trims one AI capability target selector and discards empty values.
/// 修剪单个 AI 能力目标选择器并丢弃空值。
///
/// # Parameters
/// 参数
/// `raw_selector` is the selector string received from the WebUI payload.
/// `raw_selector` 是从 WebUI 载荷接收的选择器字符串。
///
/// # Returns
/// 返回值
/// A non-empty trimmed selector, or `None` when the payload item is blank.
/// 返回非空的已修剪选择器；当载荷项为空白时返回 `None`。
fn clean_ai_capability_target_selector(raw_selector: String) -> Option<String> {
    let selector = raw_selector.trim();
    (!selector.is_empty()).then(|| selector.to_string())
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
            .filter_map(clean_ai_capability_target_selector)
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
        .filter_map(clean_ai_capability_target_selector)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies target order overrides share the canonical selector cleanup rule.
    /// 验证目标顺序覆盖复用标准选择器清洗规则。
    #[test]
    fn target_order_overrides_trim_and_drop_blank_selectors() {
        let overrides = parse_ai_capability_target_order_overrides(Some(BTreeMap::from([(
            String::from("understand_image"),
            vec![
                String::from("  vendor-a/vision  "),
                String::from("  "),
                String::from("vendor-b/vision"),
            ],
        )])))
        .expect("valid capability kind should parse");

        assert_eq!(
            overrides.get(&AiToolKind::UnderstandImage),
            Some(&vec![
                String::from("vendor-a/vision"),
                String::from("vendor-b/vision")
            ])
        );
    }

    /// Verifies selector set parsing trims, drops blanks, and deduplicates values.
    /// 验证选择器集合解析会修剪、丢弃空白并去重。
    #[test]
    fn target_selectors_trim_drop_blank_and_deduplicate() {
        let selectors = parse_ai_capability_target_selectors(Some(vec![
            String::from("  vendor-a/vision  "),
            String::from(""),
            String::from("vendor-a/vision"),
            String::from("vendor-b/vision"),
        ]));

        assert_eq!(
            selectors,
            BTreeSet::from([
                String::from("vendor-a/vision"),
                String::from("vendor-b/vision")
            ])
        );
    }
}
