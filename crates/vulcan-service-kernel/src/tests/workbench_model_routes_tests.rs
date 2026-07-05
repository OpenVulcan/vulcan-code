//! Workbench model route helper tests.
//! 工作台模型路由辅助逻辑测试。

use crate::workbench_model_routes::openrouter_route_selection_from_service;
use vulcan_service_api::ProviderModelSecondaryRouteSelection;

/// Builds one secondary-route selection payload for OpenRouter conversion tests.
/// 为 OpenRouter 转换测试构建一个二级路由选择载荷。
///
/// # Parameters
/// 参数
/// `route_id` is the compatibility route id sent by older clients.
/// `route_id` 是旧客户端发送的兼容路由标识。
/// `tag` is the canonical OpenRouter endpoint tag sent by newer clients.
/// `tag` 是新客户端发送的规范 OpenRouter 端点 tag。
///
/// # Returns
/// 返回值
/// A service route-selection payload with stable display labels.
/// 返回一个带稳定显示标签的 service 路由选择载荷。
fn test_secondary_route_selection(
    route_id: &str,
    tag: &str,
) -> ProviderModelSecondaryRouteSelection {
    ProviderModelSecondaryRouteSelection {
        route_id: route_id.to_string(),
        tag: tag.to_string(),
        provider_name: String::from("Provider A"),
        endpoint_name: String::from("Endpoint A"),
    }
}

/// Verifies legacy routeId remains accepted when tag is absent.
/// 验证缺少 tag 时仍接受旧版 routeId。
#[test]
fn openrouter_route_selection_accepts_legacy_route_id() {
    let selection =
        openrouter_route_selection_from_service(test_secondary_route_selection("route-a", ""))
            .expect("legacy route id should be accepted");

    assert_eq!(selection.tag, "route-a");
    assert_eq!(selection.provider_name, "Provider A");
    assert_eq!(selection.endpoint_name, "Endpoint A");
}

/// Verifies the canonical tag wins over legacy routeId and is trimmed before persistence.
/// 验证规范 tag 优先于旧版 routeId，并在持久化前去除空白。
#[test]
fn openrouter_route_selection_prefers_canonical_tag() {
    let selection = openrouter_route_selection_from_service(test_secondary_route_selection(
        "legacy-route",
        "  canonical-tag  ",
    ))
    .expect("canonical tag should be accepted");

    assert_eq!(selection.tag, "canonical-tag");
}

/// Verifies malformed secondary-route payloads cannot be interpreted as a clear action.
/// 验证无效二级路由载荷不会被解释为清除动作。
#[test]
fn openrouter_route_selection_rejects_empty_route_identity() {
    let error = openrouter_route_selection_from_service(test_secondary_route_selection("", ""))
        .expect_err("empty route identity must be rejected");

    assert!(
        error.contains("routeId") && error.contains("tag"),
        "unexpected error: {error}"
    );
}
