//! Capability RPC regression tests using the real indexed metadata boundary.
//! 使用真实索引元数据边界的能力 RPC 回归测试。

use super::*;
use crate::public_ai_settings::{
    service_public_ai_session_provider_tool_selections,
    service_save_public_ai_session_provider_tool_selections,
};
use vulcan_session_core::v1::config::ProviderToolSelectionConfig;

/// GET and SAVE must succeed with unavailable interaction files and never instantiate a runtime.
/// GET 和 SAVE 必须在交互文件不可用时成功，且绝不实例化运行时。
#[tokio::test(flavor = "current_thread")]
async fn capability_metadata_rpc_does_not_load_turns_or_write_on_read() {
    // The isolated controller exercises the same SQLite locator path used by desktop RPCs.
    // 隔离宿主存储验证与桌面 RPC 相同的 SQLite 定位路径。
    let session_id = SessionId::from_u128(0x62c2);
    let fixture = TestHostRuntimeRoot::new(session_id);
    let controller = fixture.attached_host_runtime_handle().await;
    let locator = fixture.v1_storage_locator();
    std::fs::remove_dir_all(fixture.v1_session_directory().join("segments")).unwrap();
    std::fs::remove_dir_all(fixture.v1_session_directory().join("context")).unwrap();
    assert!(vulcan_workspace_store::workspace_store::load_session_v1_tail(&locator).is_err());
    let original_metadata =
        std::fs::read(fixture.v1_session_directory().join("metadata.json")).unwrap();
    let mut bridge = test_service_state_with_parent_session(session_id);
    bridge.controller_runtime = Some(controller);
    bridge.agent_runtime = Some(test_agent_runtime_bridge());
    let state = Arc::new(Mutex::new(bridge));
    let emitter = ServiceEventEmitter::new(Vec::new());
    service_public_ai_session_provider_tool_selections(
        emitter.clone(),
        state.clone(),
        session_id.to_string(),
    )
    .await
    .expect("GET must not restore Turns");
    assert_eq!(
        std::fs::read(fixture.v1_session_directory().join("metadata.json")).unwrap(),
        original_metadata
    );
    assert!(!locator.read_provider_tool_selection().unwrap().initialized);
    // Reopened sessions legitimately have no execution projection; settings reads must not create one.
    // 重开会话可以合法地没有执行投影；设置读取不能创建执行投影。
    assert!(
        state
            .lock()
            .unwrap()
            .workspace_snapshot
            .as_ref()
            .unwrap()
            .session_runtime_state
            .is_empty()
    );
    state
        .lock()
        .unwrap()
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .session_runtime_state
        .insert(session_id, Default::default());
    // A persisted explicit choice must outrank the already initialized, stale workspace adapter.
    // 持久化显式选择必须优先于已初始化但陈旧的工作区适配器。
    let selection = ProviderToolSelectionConfig {
        initialized: true,
        overridden: true,
        enabled_capabilities: vec![String::from("web_search")],
        ..Default::default()
    };
    locator
        .save_unloaded_provider_tool_selection(selection, 2)
        .unwrap();
    let configured_metadata =
        std::fs::read(fixture.v1_session_directory().join("metadata.json")).unwrap();
    service_public_ai_session_provider_tool_selections(
        emitter.clone(),
        state.clone(),
        session_id.to_string(),
    )
    .await
    .expect("GET must prefer metadata over its workspace cache");
    assert_eq!(
        std::fs::read(fixture.v1_session_directory().join("metadata.json")).unwrap(),
        configured_metadata
    );
    {
        let guard = state.lock().unwrap();
        assert!(guard.session_runtime_registry.get(session_id).is_err());
        assert_eq!(
            guard
                .workspace_snapshot
                .as_ref()
                .unwrap()
                .session_runtime_state[&session_id]
                .provider_tool_enabled_capabilities,
            BTreeSet::from([String::from("web_search")])
        );
    }
    // An explicit empty override is valid and must be durably stored despite missing history.
    // 显式空覆盖合法，即使历史缺失也必须可靠保存。
    state
        .lock()
        .unwrap()
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .session_runtime_state
        .clear();
    let request = serde_json::from_value(serde_json::json!({
        "conversationId": session_id.to_string(),
        "enabledCapabilities": [], "selections": {}, "extensionTargetSets": {},
        "mediaUnderstandingSelections": {},
    }))
    .unwrap();
    service_save_public_ai_session_provider_tool_selections(
        emitter.clone(),
        state.clone(),
        request,
    )
    .await
    .expect("SAVE must not restore Turns");
    assert_eq!(
        locator.read_provider_tool_selection().unwrap(),
        ProviderToolSelectionConfig {
            initialized: true,
            overridden: true,
            ..Default::default()
        }
    );
    let saved_metadata =
        std::fs::read(fixture.v1_session_directory().join("metadata.json")).unwrap();
    service_public_ai_session_provider_tool_selections(
        emitter,
        state.clone(),
        session_id.to_string(),
    )
    .await
    .expect("GET after SAVE");
    assert_eq!(
        std::fs::read(fixture.v1_session_directory().join("metadata.json")).unwrap(),
        saved_metadata
    );
    assert!(
        state
            .lock()
            .unwrap()
            .session_runtime_registry
            .get(session_id)
            .is_err()
    );
    assert!(!fixture.v1_session_directory().join("segments").exists());
    assert!(
        state
            .lock()
            .unwrap()
            .workspace_snapshot
            .as_ref()
            .unwrap()
            .session_runtime_state
            .is_empty()
    );
}

/// A fresh process reads saved choices and can rebuild a malformed addon via the actual RPC router.
/// 新进程可读取已存选择，并通过实际 RPC 路由重建结构损坏的附加项。
#[tokio::test(flavor = "current_thread")]
async fn capability_metadata_reopen_and_rebuild_rpc_preserves_conversation() {
    // scenario is heap-owned because the complete reopened service bridge and RPC futures exceed the default test-thread stack when combined.
    // scenario 固定在堆上，因为完整重开服务桥与RPC Future组合后会超过默认测试线程栈。
    Box::pin(async {
        let session_id = SessionId::from_u128(0x62c3);
        let fixture = TestHostRuntimeRoot::new(session_id);
        let controller = Box::pin(fixture.attached_host_runtime_handle()).await;
        let locator = fixture.v1_storage_locator();
        let path = fixture.v1_session_directory().join("metadata.json");
        let initial: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let explicit = ProviderToolSelectionConfig {
            initialized: true,
            overridden: true,
            enabled_capabilities: vec!["web_search".into()],
            ..Default::default()
        };
        locator
            .save_unloaded_provider_tool_selection(explicit.clone(), 2)
            .unwrap();
        // Each bridge has a new registry and no runtime adapter, matching a newly reopened application.
        // 每个桥都有新注册表且无运行态适配器，对应重新打开的应用。
        for attempt in 0..2 {
            let mut bridge = test_service_state_with_parent_session(session_id);
            bridge.controller_runtime = Some(controller.clone());
            bridge.agent_runtime = Some(test_agent_runtime_bridge());
            let state = Arc::new(Mutex::new(bridge));
            let emitter = ServiceEventEmitter::new(Vec::new());
            let settings = Box::pin(crate::rpc_dispatch::dispatch_service_rpc(
                emitter.clone(),
                state.clone(),
                "service_public_ai_session_provider_tool_selections".into(),
                Some(serde_json::json!({"conversationId": session_id.to_string()})),
            ))
            .await
            .unwrap();
            if attempt == 0 {
                assert_eq!(
                    settings["enabledCapabilities"],
                    serde_json::json!(["web_search"])
                );
                let mut damaged: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                *damaged
                    .pointer_mut("/config/selection/provider_tools")
                    .unwrap() = serde_json::json!("broken addon");
                std::fs::write(&path, serde_json::to_vec(&damaged).unwrap()).unwrap();
                assert!(
                    Box::pin(service_public_ai_session_provider_tool_selections(
                        emitter.clone(),
                        state.clone(),
                        session_id.to_string()
                    ))
                    .await
                    .is_err()
                );
                let rebuilt = Box::pin(crate::rpc_dispatch::dispatch_service_rpc(
                    emitter,
                    state.clone(),
                    "service_rebuild_public_ai_session_provider_tool_selections".into(),
                    Some(serde_json::json!({"conversationId": session_id.to_string()})),
                ))
                .await
                .unwrap();
                assert_eq!(rebuilt["enabledCapabilities"], serde_json::json!([]));
            } else {
                assert_eq!(settings["enabledCapabilities"], serde_json::json!([]));
            }
            let guard = state.lock().unwrap();
            assert!(guard.session_runtime_registry.get(session_id).is_err());
            assert!(
                guard
                    .workspace_snapshot
                    .as_ref()
                    .unwrap()
                    .session_runtime_state
                    .is_empty()
            );
        }
        let persisted = locator.read_provider_tool_selection().unwrap();
        assert!(persisted.initialized);
        assert!(!persisted.overridden);
        let mut final_metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        // Mask exactly the addon and its revision/time, then compare every remaining metadata field.
        // 仅屏蔽附加项及其修订/时间，然后比较全部剩余元数据字段。
        for key in [
            "/config/selection/provider_tools",
            "/document_revision",
            "/updated_at_ms",
        ] {
            *final_metadata.pointer_mut(key).unwrap() = initial
                .pointer(key)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        let mut expected = initial;
        // provider_tools is an optional serde-default addon and may be absent in fresh metadata.
        // provider_tools 是具有 serde 默认值的可选附加项，新元数据可能不包含它。
        if expected
            .pointer("/config/selection/provider_tools")
            .is_none()
        {
            expected
                .pointer_mut("/config/selection")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("provider_tools".into(), serde_json::Value::Null);
        }
        assert_eq!(final_metadata, expected);
        assert!(vulcan_workspace_store::workspace_store::load_session_v1_tail(&locator).is_ok());
    })
    .await;
}
