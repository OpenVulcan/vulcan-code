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

/// Parses prompt-level exact AI capability target bindings from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力精确目标绑定。
///
/// # Parameters
/// 参数
/// `raw_bindings` contains one exact target selector per capability key from the WebUI.
/// `raw_bindings` 包含 WebUI 为每个能力键传入的一个精确目标选择器。
///
/// # Returns
/// 返回值
/// A semantic capability keyed map containing one exact target per included capability.
/// 按语义能力分组、且每个已包含能力恰好拥有一个精确目标的映射。
pub(crate) fn parse_ai_capability_target_bindings(
    raw_bindings: Option<BTreeMap<String, String>>,
) -> Result<BTreeMap<AiToolKind, String>, String> {
    let mut bindings = BTreeMap::new();
    for (raw_kind, selector) in raw_bindings.unwrap_or_default() {
        let Some(kind) =
            vulcan_agent_runtime::workbench_ai_capability_router::parse_workbench_ai_tool_kind(
                &raw_kind,
            )
        else {
            return Err(format!("未知的扩展能力绑定类型：{raw_kind}"));
        };
        if let Some(selector) = clean_ai_capability_target_selector(selector) {
            bindings.insert(kind, selector);
        }
    }
    Ok(bindings)
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

    /// Verifies exact target bindings trim one selector and discard blank values.
    /// 验证精确目标绑定会修剪一个选择器并丢弃空值。
    #[test]
    fn target_bindings_trim_one_selector_and_drop_blanks() {
        let overrides = parse_ai_capability_target_bindings(Some(BTreeMap::from([(
            String::from("understand_image"),
            String::from("  vendor-a/vision  "),
        )])))
        .expect("valid capability kind should parse");

        assert_eq!(
            overrides.get(&AiToolKind::UnderstandImage),
            Some(&String::from("vendor-a/vision"))
        );
    }

    /// Verifies blank exact bindings are discarded instead of becoming implicit selections.
    /// 验证空白精确绑定会被丢弃，而不会变成隐式选择。
    #[test]
    fn target_bindings_drop_blank_selector() {
        let bindings = parse_ai_capability_target_bindings(Some(BTreeMap::from([(
            String::from("understand_image"),
            String::from("  "),
        )])))
        .expect("blank selector should be ignored");

        assert!(bindings.is_empty());
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
