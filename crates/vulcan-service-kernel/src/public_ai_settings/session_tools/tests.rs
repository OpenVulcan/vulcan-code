//! Regression coverage for session inheritance, canonical V1 ordering, and explicit tool conflicts.
//! 会话继承、规范 V1 排序与显式工具冲突的回归覆盖。

use super::*;

/// Rebuilt defaults inherit later global changes; explicit session overrides remain independent.
/// 重建后的默认值继承后续全局变化；显式会话覆盖仍保持独立。
#[test]
fn capability_metadata_defaults_keep_inheritance_after_rebuild() {
    use super::*;
    // No model catalog or account is needed to resolve the configuration inheritance contract.
    // 解析配置继承契约不需要模型目录或账号。
    let runtime = PublicAiRuntime::unavailable().unwrap();
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .provider_tool_enabled_capabilities
        .insert(ProviderToolCapabilityKind::WebSearch);
    let (defaults, _) =
        resolved_session_provider_tool_settings(&runtime, &configuration, &Default::default())
            .unwrap();
    assert_eq!(
        defaults.enabled_capabilities,
        BTreeSet::from([ProviderToolCapabilityKind::WebSearch])
    );
    let inherited = provider_tool_selection_config_v1(&defaults, false);
    configuration.provider_tool_enabled_capabilities.clear();
    let (changed, _) =
        resolved_session_provider_tool_settings(&runtime, &configuration, &inherited).unwrap();
    assert!(changed.enabled_capabilities.is_empty());
    let overridden = provider_tool_selection_config_v1(&defaults, true);
    let (retained, _) =
        resolved_session_provider_tool_settings(&runtime, &configuration, &overridden).unwrap();
    assert_eq!(retained.enabled_capabilities, defaults.enabled_capabilities);
}

/// Verifies an enabled session capability without a model-account pair is an explicit conflict.
/// 验证缺少模型账号组合的已启用会话能力会形成显式冲突。
#[test]
fn session_provider_tool_settings_reject_missing_selection() {
    let runtime = PublicAiRuntime::unavailable().expect("test public AI runtime");
    let configuration = PersistedAiConfiguration::default();
    let conflicts = provider_tool_session_conflicts(
        &runtime,
        &configuration,
        &BTreeSet::from([ProviderToolCapabilityKind::WebSearch]),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("conflict analysis");

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, "missing_selection");
    assert_eq!(
        conflicts[0].capability,
        ProviderToolCapabilityKind::WebSearch
    );
}

/// Verifies V1 Provider-tool persistence uses serialized capability order instead of enum order.
/// 验证 V1 Provider 工具持久化使用序列化能力键顺序，而不是枚举顺序。
#[test]
fn provider_tool_selection_v1_orders_final_persisted_keys() {
    // enabledCapabilities deliberately uses enum pairs whose declaration and string orders differ.
    // enabledCapabilities 刻意使用声明顺序与字符串顺序不同的枚举组合。
    let enabled_capabilities = BTreeSet::from([
        ProviderToolCapabilityKind::WebSearch,
        ProviderToolCapabilityKind::WebExtract,
    ]);
    // selections covers the same ordering divergence for direct target entries.
    // selections 覆盖直接目标条目的相同顺序差异。
    let selections = BTreeMap::from([
        (
            ProviderToolCapabilityKind::WebSearch,
            ProviderToolTargetSelection {
                connection_id: String::from("search-connection"),
                target_id: String::from("search-target"),
            },
        ),
        (
            ProviderToolCapabilityKind::WebExtract,
            ProviderToolTargetSelection {
                connection_id: String::from("extract-connection"),
                target_id: String::from("extract-target"),
            },
        ),
    ]);
    // mediaModes proves media capability entries are sorted by their final persisted key.
    // mediaModes 证明媒体能力条目按最终持久化键排序。
    let media_modes = BTreeMap::from([
        (
            ProviderToolCapabilityKind::ImageUnderstand,
            vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool,
        ),
        (
            ProviderToolCapabilityKind::AudioUnderstand,
            vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement,
        ),
    ]);
    // extensionTargets also carries two targets in reverse insertion order for nested sorting.
    // extensionTargets 还以反向插入顺序携带两个目标，用于验证嵌套排序。
    let extension_targets = BTreeMap::from([
        (
            ProviderToolCapabilityKind::ImageGenerate,
            BTreeSet::from([ProviderToolTargetSelection {
                connection_id: String::from("generate-connection"),
                target_id: String::from("generate-target"),
            }]),
        ),
        (
            ProviderToolCapabilityKind::ImageEdit,
            BTreeSet::from([
                ProviderToolTargetSelection {
                    connection_id: String::from("z-edit-connection"),
                    target_id: String::from("z-edit-target"),
                },
                ProviderToolTargetSelection {
                    connection_id: String::from("a-edit-connection"),
                    target_id: String::from("a-edit-target"),
                },
            ]),
        ),
    ]);

    // selection is the exact producer result sent to the strict Session Core setter.
    // selection 是发送给严格 Session Core setter 的精确生产结果。
    let selection = provider_tool_selection_config_from_parts_v1(
        &enabled_capabilities,
        &selections,
        &media_modes,
        &extension_targets,
        true,
    );

    assert_eq!(
        selection.enabled_capabilities,
        vec![String::from("web_extract"), String::from("web_search")]
    );
    assert_eq!(
        selection
            .selections
            .iter()
            .map(|entry| entry.capability.as_str())
            .collect::<Vec<_>>(),
        vec!["web_extract", "web_search"]
    );
    assert_eq!(
        selection
            .media_understanding_modes
            .iter()
            .map(|entry| entry.capability.as_str())
            .collect::<Vec<_>>(),
        vec!["audio_understand", "image_understand"]
    );
    assert_eq!(
        selection
            .extension_target_sets
            .iter()
            .map(|entry| entry.capability.as_str())
            .collect::<Vec<_>>(),
        vec!["image_edit", "image_generate"]
    );
    assert_eq!(
        selection.extension_target_sets[0]
            .targets
            .iter()
            .map(|target| target.connection_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a-edit-connection", "z-edit-connection"]
    );
}

/// Verifies an enabled media capability reads its dedicated backend store instead of the legacy session target map.
/// 验证已启用媒体能力读取专用后端存储，而不是旧会话目标映射。
#[test]
fn session_media_understanding_settings_reject_missing_backend() {
    let runtime = PublicAiRuntime::unavailable().expect("test public AI runtime");
    let configuration = PersistedAiConfiguration::default();
    let conflicts = provider_tool_session_conflicts(
        &runtime,
        &configuration,
        &BTreeSet::from([ProviderToolCapabilityKind::VideoUnderstand]),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("media conflict analysis");

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, "missing_selection");
    assert_eq!(
        conflicts[0].capability,
        ProviderToolCapabilityKind::VideoUnderstand
    );
}

/// Verifies lazy session provider-tool initialization is deterministic and idempotent.
/// 验证惰性会话供应商工具初始化具备确定性与幂等性。
#[test]
fn session_provider_tool_settings_reconciliation_is_idempotent() {
    // configuration owns one exact global capability and model-account default.
    // configuration 持有一项精确的全局能力与模型账号默认值。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .provider_tool_enabled_capabilities
        .insert(ProviderToolCapabilityKind::WebSearch);
    configuration.provider_tool_selections.insert(
        ProviderToolCapabilityKind::WebSearch,
        ProviderToolTargetSelection {
            connection_id: String::from("search-connection"),
            target_id: String::from("search-model"),
        },
    );
    // extensionDefault owns one multi-select generation target inherited by a new session.
    // extensionDefault 持有一项由新会话继承的多选生成目标。
    let extension_default = ProviderToolTargetSelection {
        connection_id: String::from("image-connection"),
        target_id: String::from("image-model"),
    };
    configuration.provider_extension_target_sets.insert(
        ProviderToolCapabilityKind::ImageGenerate,
        BTreeSet::from([extension_default.clone()]),
    );
    // runtimeState starts in the current uninitialized representation.
    // runtimeState 从当前未初始化表示开始。
    let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();
    // firstReconciliation must materialize the global defaults exactly once.
    // firstReconciliation 必须且仅须一次物化全局默认值。
    let (
        enabled_capabilities,
        selections,
        media_understanding_modes,
        extension_target_sets,
        first_changed,
    ) = reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
        .expect("uninitialized session settings should reconcile");
    assert!(first_changed);
    assert_eq!(
        enabled_capabilities,
        BTreeSet::from([ProviderToolCapabilityKind::WebSearch])
    );
    assert_eq!(
        selections
            .get(&ProviderToolCapabilityKind::WebSearch)
            .map(|selection| selection.connection_id.as_str()),
        Some("search-connection")
    );
    assert!(media_understanding_modes.is_empty());
    assert_eq!(
        extension_target_sets.get(&ProviderToolCapabilityKind::ImageGenerate),
        Some(&BTreeSet::from([extension_default]))
    );
    // secondChanged proves an identical generation performs no authoritative state write.
    // secondChanged 证明相同代际不会再次执行权威状态写入。
    let (
        _enabled_capabilities,
        _selections,
        _media_understanding_modes,
        _extension_target_sets,
        second_changed,
    ) = reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
        .expect("initialized session settings should remain readable");
    assert!(!second_changed);
}

/// Verifies legacy media targets are removed even from sessions with explicit overrides.
/// 验证即使会话存在显式覆盖，也会移除历史媒体目标。
#[test]
fn session_media_direct_targets_are_removed_from_overridden_sessions() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.provider_tool_selections.insert(
        ProviderToolCapabilityKind::VideoUnderstand,
        ProviderToolTargetSelection {
            connection_id: String::from("legacy-video-connection"),
            target_id: String::from("legacy-video-target"),
        },
    );
    let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
        provider_tool_settings_initialized: true,
        provider_tool_selections_overridden: true,
        ..Default::default()
    };
    runtime_state.provider_tool_selections.insert(
        provider_tool_capability_key(ProviderToolCapabilityKind::VideoUnderstand),
        vulcan_workbench_model::SessionProviderToolTargetSelection {
            connection_id: String::from("legacy-video-connection"),
            target_id: String::from("legacy-video-target"),
        },
    );
    runtime_state.provider_tool_selections.insert(
        provider_tool_capability_key(ProviderToolCapabilityKind::WebSearch),
        vulcan_workbench_model::SessionProviderToolTargetSelection {
            connection_id: String::from("search-connection"),
            target_id: String::from("search-target"),
        },
    );

    let (
        _enabled_capabilities,
        selections,
        _media_understanding_modes,
        _extension_target_sets,
        changed,
    ) = reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
        .expect("legacy media targets should be normalized");

    assert!(changed);
    assert!(
        !runtime_state
            .provider_tool_selections
            .contains_key(&provider_tool_capability_key(
                ProviderToolCapabilityKind::VideoUnderstand
            ))
    );
    assert!(
        runtime_state
            .provider_tool_selections
            .contains_key(&provider_tool_capability_key(
                ProviderToolCapabilityKind::WebSearch
            ))
    );
    assert!(!selections.contains_key(&ProviderToolCapabilityKind::VideoUnderstand));
    assert!(selections.contains_key(&ProviderToolCapabilityKind::WebSearch));
}

/// Verifies first session materialization persists the active global media backend mode.
/// 验证首次物化会话时会持久化当前全局媒体后端模式。
#[test]
fn session_media_backend_mode_is_materialized_with_session_defaults() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::ImageUnderstand,
        MediaUnderstandingBackendSelection::ProviderTool {
            selection: ProviderToolTargetSelection {
                connection_id: String::from("image-account"),
                target_id: String::from("image-target"),
            },
        },
    );
    let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();

    let (_enabled_capabilities, _selections, modes, _extension_target_sets, changed) =
        reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
            .expect("session defaults should materialize");

    assert!(changed);
    assert_eq!(
        modes.get(&ProviderToolCapabilityKind::ImageUnderstand),
        Some(&vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool)
    );
    assert_eq!(
        runtime_state.media_understanding_modes,
        BTreeMap::from([(
            String::from("image_understand"),
            vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool,
        )])
    );
}
