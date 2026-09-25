//! Integration tests for tool-collapse preference defaults and persistence round trips.
//! 工具折叠偏好的默认值与持久化往返集成测试。

use vulcan_provider_config::config::ProviderConfigurationFile;

/// Verifies absent defaults and explicit false/true preferences round-trip without startup writes.
/// 验证缺省与明确关闭／开启的偏好可往返序列化，且无需启动写入。
#[test]
fn tool_collapse_default_round_trips() {
    // Legacy configuration retains an implicit enabled default.
    // 旧配置保留隐式默认开启行为。
    let legacy: ProviderConfigurationFile = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.tool_collapse_default_enabled, None);
    assert!(legacy.tool_collapse_default_enabled.unwrap_or(true));
    assert!(
        serde_json::to_value(&legacy)
            .unwrap()
            .get("toolCollapseDefaultEnabled")
            .is_none()
    );
    for enabled in [false, true] {
        // Explicit false must not disappear as an omitted field.
        // 明确关闭不得被当成缺省字段丢弃。
        let configuration = ProviderConfigurationFile {
            tool_collapse_default_enabled: Some(enabled),
            ..ProviderConfigurationFile::default()
        };
        // The JSON snapshot and TOML configuration must preserve the same explicit value.
        // JSON 快照与 TOML 配置必须保留同一个明确值。
        let encoded = serde_json::to_string(&configuration).unwrap();
        let decoded: ProviderConfigurationFile = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.tool_collapse_default_enabled, Some(enabled));
        let encoded = toml::to_string(&configuration).unwrap();
        let decoded: ProviderConfigurationFile = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.tool_collapse_default_enabled, Some(enabled));
    }
}
