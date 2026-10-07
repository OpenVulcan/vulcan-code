// Model list regressions: WebUI provider grouping, filters and switch writes over the synthetic
// public AI snapshot.
// 模型清单回归测试：基于合成公开 AI 快照验证 WebUI 供应商分组、筛选与开关写入。

use super::action::{UiAction, perform};
use super::pages::{RowAction, RowKind, rows};
use super::*;
use crate::service_api::{CallKind, ServiceCall, Slot, settings_actions};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};

/// Builds a disconnected app holding the synthetic public AI settings snapshot.
/// 构建持有合成公开 AI 设置快照的断开应用。
/// `edit` adjusts the snapshot first; returns the app and its command receiver.
/// `edit` 先调整快照；返回应用及其命令接收端。
fn fixture_app(edit: impl FnOnce(&mut Value)) -> (App, Receiver<Command>) {
    // Isolated command channel exposes requests without executing them.
    // 隔离命令通道公开请求，但不执行。
    let (commands, receiver) = mpsc::channel();
    // No backend event producer is connected.
    // 不连接后端事件生产者。
    let (_, events) = mpsc::channel();
    // Repository-owned synthetic fixture, never a user service file.
    // 仓库持有的合成夹具，不使用用户服务文件。
    let fixture: Value =
        serde_json::from_str(include_str!("../backend/mock/service-fixtures.json")).unwrap();
    let mut snapshot = fixture["loads"]["service_public_ai_settings_snapshot"].clone();
    edit(&mut snapshot);
    let mut app = App::new(
        BackendHandle {
            commands,
            events,
            label: "test".into(),
            is_mock: true,
        },
        Theme::dark(),
        true,
    );
    app.apply(BackendEvent::Slot {
        slot: Slot::PublicAiSettings,
        call: settings_actions::load_public_ai_settings(),
        value: snapshot,
    });
    receiver.try_iter().for_each(drop);
    (app, receiver)
}

/// Model rows of the list as `(item id, chips)`, excluding diagnostics and filters.
/// 清单中的模型行 `(条目标识, 按钮)`，不含诊断与筛选。
fn model_rows(app: &App) -> Vec<(String, Vec<RowAction>)> {
    rows(app, PageId::Models)
        .into_iter()
        .filter_map(|row| match row.kind {
            RowKind::Item { id, actions, .. } if id.starts_with("model:") => Some((id, actions)),
            _ => None,
        })
        .collect()
}

fn model_ids(app: &App) -> Vec<String> {
    model_rows(app).into_iter().map(|(id, _)| id).collect()
}

/// Chips of one model row.
/// 单个模型行的按钮。
fn chips(app: &App, id: &str) -> Vec<RowAction> {
    model_rows(app)
        .into_iter()
        .find(|(row, _)| row == id)
        .unwrap_or_else(|| panic!("{id} row; the synthetic fixture may have changed"))
        .1
}

/// Performs one chip and returns the service write it sent.
/// 执行一个按钮并返回其发送的服务写入。
fn sent(app: &mut App, commands: &Receiver<Command>, action: UiAction) -> ServiceCall {
    perform(app, action);
    app.apply(BackendEvent::CallFinished);
    commands
        .try_iter()
        .find_map(|command| match command {
            Command::Call(call) => Some(call),
            _ => None,
        })
        .expect("service write")
}

/// Only models with an available account are listed, grouped under providers ordered by
/// priority; an unknown provider choice falls back to the first group.
/// 仅列出有可用账号的模型，并按优先级排序的供应商分组；未知供应商选择回退到第一个分组。
#[test]
fn model_list_groups_available_models_by_provider_priority() {
    let (mut app, _) = fixture_app(|_| {});
    // The support-only demo-model-3 has no available account and stays hidden.
    // 仅存在于支持目录的 demo-model-3 没有可用账号，保持隐藏。
    assert_eq!(
        model_ids(&app),
        [
            "model:demo-provider/demo-model-0",
            "model:demo-provider/demo-model-1",
            "model:demo-provider/demo-model-2",
        ],
        "expected values follow the synthetic fixture and may be outdated",
    );
    app.model_provider = "demo-media".into();
    assert_eq!(model_ids(&app), ["model:demo-media/demo-image-1"]);
    // Raising a provider's priority moves it first.
    // 提高供应商优先级会使其排在首位。
    let (mut app, _) = fixture_app(|snapshot| {
        snapshot["providerPriorities"]["demo-media"] = json!(20);
    });
    app.model_provider = "unknown".into();
    assert_eq!(model_ids(&app), ["model:demo-media/demo-image-1"]);
}

/// Usage, output and search filters follow the WebUI visibleModelRows rules.
/// 用途、输出与搜索筛选遵循 WebUI visibleModelRows 规则。
#[test]
fn model_filters_follow_usage_output_and_search() {
    let (mut app, _) = fixture_app(|_| {});
    let ids = |app: &App| -> Vec<String> {
        model_ids(app)
            .into_iter()
            .map(|id| id.rsplit('/').next().unwrap().to_string())
            .collect()
    };
    app.model_usage = "conversation".into();
    assert_eq!(ids(&app), ["demo-model-0"]);
    app.model_usage = "internal".into();
    assert_eq!(ids(&app), ["demo-model-0", "demo-model-1", "demo-model-2"]);
    app.model_usage = "multimodal".into();
    assert!(ids(&app).is_empty());
    app.model_usage = "all".into();
    app.model_output = "embedding".into();
    assert_eq!(ids(&app), ["demo-model-1"]);
    app.model_output = "all".into();
    app.model_query = "  RERANK ".into();
    assert_eq!(ids(&app), ["demo-model-2"]);
    // Extension outputs count: the image model matches the image filter.
    // 扩展输出同样计入：图片模型匹配图片筛选。
    app.model_query.clear();
    app.model_provider = "demo-media".into();
    app.model_output = "image".into();
    assert_eq!(ids(&app), ["demo-image-1"]);
    // A selected, admitted extension counts as plugin use.
    // 已选中且已准入的扩展计为插件调用。
    app.model_output = "all".into();
    app.model_usage = "internal".into();
    assert!(ids(&app).is_empty());
    let (mut app, _) = fixture_app(|snapshot| {
        snapshot["providerExtensionTargetSets"]["image_generate"] =
            json!([{"connection_id":"demo-media-connection","target_id":"demo-image-generate"}]);
        snapshot["providerToolEnabledCapabilities"] = json!(["image_generate"]);
    });
    app.model_provider = "demo-media".into();
    app.model_usage = "internal".into();
    assert_eq!(ids(&app), ["demo-image-1"]);
}

/// Each switch sends the exact WebUI write that flips it; inapplicable cells stay inert.
/// 每个开关发送翻转它的精确 WebUI 写入；不适用的单元格保持无操作。
#[test]
fn model_switches_send_the_webui_writes() {
    let (mut app, commands) = fixture_app(|_| {});
    // Conversation, multimodal understanding and internal use of a chat model.
    // 对话模型的对话使用、多模态理解与内部使用。
    let switches: Vec<_> = chips(&app, "model:demo-provider/demo-model-0")
        .into_iter()
        .filter(|chip| chip.switch.is_some())
        .collect();
    assert_eq!(switches.len(), 3);
    let mut writes: Vec<(String, Value)> = switches
        .into_iter()
        .map(|chip| {
            let call = sent(&mut app, &commands, chip.action.expect("enabled switch"));
            (
                call.method.to_string(),
                call.param("/request/enabled").clone(),
            )
        })
        .collect();
    writes.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        writes,
        [
            (
                "service_save_public_ai_internal_model_enabled".to_string(),
                json!(false)
            ),
            (
                "service_save_public_ai_model_enabled".to_string(),
                json!(false)
            ),
            (
                "service_save_public_ai_multimodal_understanding_candidate".to_string(),
                json!(true)
            ),
        ]
    );
    // An embedding model offers only internal use; the other cells are disabled.
    // 嵌入模型只提供内部使用；其他单元格为禁用状态。
    let embedding = chips(&app, "model:demo-provider/demo-model-1");
    assert_eq!(
        embedding
            .iter()
            .filter(|chip| chip.switch.is_some())
            .count(),
        1
    );
    assert_eq!(
        embedding
            .iter()
            .filter(|chip| chip.switch.is_none() && !chip.enabled())
            .count(),
        2
    );
    // The stale admission keeps a disable button.
    // 失效准入保留关闭按钮。
    let stale = rows(&app, PageId::Models)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Item { id, actions, .. } if id.starts_with("stale-candidate:") => {
                actions.into_iter().next()
            }
            _ => None,
        })
        .expect("stale admission row");
    let call = sent(&mut app, &commands, stale.action.expect("disable button"));
    assert_eq!(
        call.method,
        "service_save_public_ai_multimodal_understanding_candidate"
    );
    assert_eq!(call.param("/request/modelId"), "retired-model");
    assert_eq!(call.param("/request/enabled"), false);
}

/// Selecting an extension saves the capability's full target set, then admits the capability;
/// a target without a reviewed extension profile cannot be selected.
/// 选中扩展会保存该能力的完整目标集合，随后准入该能力；没有已审核扩展资料的目标不可选中。
#[test]
fn extension_switch_saves_the_target_set_then_admits_the_capability() {
    let (mut app, commands) = fixture_app(|snapshot| {
        snapshot["providerExtensionTargetSets"]["image_generate"] =
            json!([{"connection_id":"other-connection","target_id":"other-target"}]);
    });
    app.model_provider = "demo-media".into();
    let switches: Vec<_> = chips(&app, "model:demo-media/demo-image-1")
        .into_iter()
        .filter(|chip| chip.switch.is_some())
        .collect();
    assert_eq!(switches.len(), 2);
    assert_eq!(switches.iter().filter(|chip| chip.enabled()).count(), 1);
    let enabled = switches
        .into_iter()
        .find_map(|chip| chip.action)
        .expect("image generation switch");
    let call = sent(&mut app, &commands, enabled);
    assert_eq!(
        call.method,
        "service_save_public_ai_provider_extension_target_set"
    );
    assert_eq!(call.param("/request/capability"), "image_generate");
    assert_eq!(
        call.param("/request/selections"),
        &json!([
            {"connection_id":"other-connection","target_id":"other-target"},
            {"connection_id":"demo-media-connection","target_id":"demo-image-generate"},
        ])
    );
    let CallKind::Then(next) = &call.kind else {
        panic!("capability admission follows the target set");
    };
    assert_eq!(
        next.method,
        "service_save_public_ai_provider_tool_capability_enabled"
    );
    assert_eq!(next.param("/request/enabled"), true);
}
