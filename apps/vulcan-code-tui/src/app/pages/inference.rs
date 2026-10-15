// Catalog-backed VMM inference selection.
// 基于目录的 VMM 推理选择。

use super::*;
use crate::service_api::settings_actions as settings;

/// Checks the exact capability intersection for disabling reasoning.
/// 检查关闭推理所需的准确能力交集。
fn reasoning_off(candidate: &Value) -> bool {
    match v(candidate, "/capabilities/reasoning_control").as_str() {
        Some("fixed_off") => true,
        Some("toggle" | "effort_selection" | "provider_adaptive") => {
            a(candidate, "/capabilities/reasoning_efforts")
                .iter()
                .any(|effort| effort == "none")
        }
        _ => false,
    }
}

/// Returns the declared positive dimension domain and its valid initial suggestion.
/// 返回声明的正数维度范围及有效初始建议值。
pub(super) fn dimension(capability: &Value) -> Option<u32> {
    // Minimum.
    // 协议允许的最小值。
    let minimum = match capability.get("minimum_dimensions") {
        Some(value) => u32::try_from(value.as_u64()?).ok()?,
        None => 1,
    };
    // Maximum.
    // 协议允许的最大值。
    let maximum = match capability.get("maximum_dimensions") {
        Some(value) => u32::try_from(value.as_u64()?).ok()?,
        None => u32::MAX,
    };
    if minimum == 0 || maximum < minimum {
        return None;
    }
    // Declared.
    // 目录声明的可用值。
    let declared = capability.get("dimensions")?.as_array()?;
    // Dimensions.
    // 模型声明的维度集合。
    let mut dimensions = Vec::new();
    for value in declared {
        // Value.
        // 当前协议值。
        let value = u32::try_from(value.as_u64()?).ok()?;
        if value == 0 {
            return None;
        }
        if (minimum..=maximum).contains(&value) {
            dimensions.push(value);
        }
    }
    dimensions.sort_unstable();
    dimensions.dedup();
    // First.
    // 第一个观察值。
    let first = if declared.is_empty() {
        if !capability.get("minimum_dimensions").is_some()
            && !capability.get("maximum_dimensions").is_some()
        {
            return None;
        }
        minimum
    } else {
        *dimensions.first()?
    };
    if let Some(preferred) = capability.get("default_dimensions").and_then(Value::as_u64)
        && let Ok(preferred) = u32::try_from(preferred)
        && (minimum..=maximum).contains(&preferred)
        && (declared.is_empty() || dimensions.contains(&preferred))
    {
        return Some(preferred);
    }
    Some(first)
}

/// Mirrors the host-background operation eligibility used by the WebUI selectors.
/// 镜像 WebUI 选择器使用的宿主后台操作资格。
fn eligible(candidate: &Value, operation: &str) -> bool {
    if !b(candidate, "/enabled")
        || s(candidate, "/route/usage_scope") != "internal_background"
        || s(candidate, "/route/operation") != operation
    {
        return false;
    }
    match operation {
        "conversation_respond" => {
            reasoning_off(candidate)
                && matches!(
                    v(candidate, "/capabilities/structured_output").as_str(),
                    Some("json_object" | "json_schema")
                )
        }
        "embedding_create" | "rerank_documents" => true,
        _ => false,
    }
}

/// Converts the selected catalog route to the shared background inference API.
/// 将选中目录路由转换为共享后台推理接口调用。
fn select_route(context: &Value, value: &SettingValue) -> Option<ServiceCall> {
    // Selected.
    // 当前选择。
    let selected: Value = serde_json::from_str(value.as_str()).ok()?;
    // Consumer.
    // 宿主后台消费者。
    let consumer = context.get("consumer")?.as_str()?;
    // Operation.
    // 准确推理操作。
    let operation = context.get("operation")?.as_str()?;

    // Purpose.
    // 准确背景推理用途。
    let purpose = context.get("purpose")?.as_str();
    Some(settings::save_public_ai_inference_route(
        consumer, purpose, operation, selected, None,
    ))
}

/// Checks a positive dimension against optional bounds and the declared discrete domain.
/// 按可选边界及声明的离散范围检查正数维度。
pub(super) fn dimension_valid(capability: &Value, dimension: u32) -> bool {
    dimension > 0
        && !capability
            .get("minimum_dimensions")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| u64::from(dimension) < minimum)
        && !capability
            .get("maximum_dimensions")
            .and_then(Value::as_u64)
            .is_some_and(|maximum| u64::from(dimension) > maximum)
        && capability
            .get("dimensions")
            .and_then(Value::as_array)
            .is_some_and(|dimensions| {
                dimensions.is_empty()
                    || dimensions
                        .iter()
                        .any(|value| value.as_u64() == Some(u64::from(dimension)))
            })
}

/// Renders one catalog choice; its internal JSON id is never exposed as an editable route.
/// 渲染单个目录选择项；内部 JSON 标识不会暴露为可编辑路由。
fn route_row(
    app: &App,
    consumer: &str,
    operation: &str,
    purpose: Option<&str>,
    label: String,
) -> Row {
    // Public AI settings.
    // 公开 AI 设置。
    let ai = app.slots.value(Slot::PublicAiSettings);
    // Exact path.
    // 准确路径。
    let path = match purpose {
        Some(purpose) => format!("/inferenceProfiles/{consumer}/llm_routes/{purpose}"),
        None => format!(
            "/inferenceProfiles/{consumer}/{}",
            if operation == "embedding_create" {
                "embedding"
            } else {
                "rerank"
            }
        ),
    };
    // Current.
    // 当前选中或已读取值。
    let current = v(ai, &path);
    // Options.
    // 允许的选择项。
    let mut options = vec![("null".into(), tr!("tui.field.unconfigured"))];
    for candidate in a(ai, "/internalInferenceCandidates")
        .iter()
        .filter(|candidate| eligible(candidate, operation))
    {
        // Route.
        // 目录中的推理路由。
        let route = v(candidate, "/route");
        // Selection.
        // 已批准选择。
        let selection = route.clone();
        options.push((
            selection.to_string(),
            format!(
                "{} / {} / {}",
                s(candidate, "/providerDisplayName"),
                s(candidate, "/connectionDisplayName"),
                s(candidate, "/modelDisplayName")
            ),
        ));
    }
    // Current.
    // 当前选中或已读取值。
    let current = current.to_string();
    choice(
        label,
        options,
        current,
        Edit::Call(Setter::Build {
            ctx: json!({"consumer":consumer,"operation":operation,"purpose":purpose}),
            build: select_route,
        }),
    )
}

/// Builds consumer-specific route controls plus service-published diagnostic details.
/// 构建使用方专属路由控件与服务发布的诊断详情。
pub(super) fn rows(app: &App, consumer: &str) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    if consumer == "vmm" {
        for (purpose, label) in [
            ("precheck_l1", tr!("settings.inference.purpose.precheckL1")),
            ("precheck_l2", tr!("settings.inference.purpose.precheckL2")),
            (
                "postaction_l1",
                tr!("settings.inference.purpose.postactionL1"),
            ),
            (
                "postaction_l2",
                tr!("settings.inference.purpose.postactionL2"),
            ),
            (
                "profile_instruction",
                tr!("settings.inference.purpose.profileInstruction"),
            ),
            ("reserve", tr!("settings.inference.purpose.reserve")),
        ] {
            rows.push(route_row(
                app,
                consumer,
                "conversation_respond",
                Some(purpose),
                label,
            ));
        }
    } else {
        rows.push(route_row(
            app,
            consumer,
            "conversation_respond",
            Some("code_retrieval.query_rewrite"),
            tr!("tui.field.queryRewrite"),
        ));
    }
    rows.push(route_row(
        app,
        consumer,
        "embedding_create",
        None,
        tr!("tui.field.embedding"),
    ));
    rows.push(route_row(
        app,
        consumer,
        "rerank_documents",
        None,
        tr!("tui.field.rerank"),
    ));

    for operation in ["embedding_create", "rerank_documents"] {
        rows.push(button(
            format!("{} · {operation}", tr!("tui.field.testRoute")),
            confirm(
                tr!("tui.field.testRoute"),
                tr!("tui.field.paidTest"),
                tr!("tui.field.confirm"),
                call(settings::test_public_ai_inference_route(
                    consumer, operation,
                )),
            ),
            Tone::Normal,
        ));
    }
    for diagnostic in a(
        app.slots.value(Slot::PublicAiSettings),
        "/inferenceRouteDiagnostics",
    )
    .iter()
    .filter(|diagnostic| s(diagnostic, "/consumerProfileId") == consumer)
    {
        rows.push(
            item(
                &format!("route-{}", s(diagnostic, "/operation")),
                tr!("tui.field.diagnostic"),
                "",
            )
            .lines(details(diagnostic)),
        );
    }
    rows
}
