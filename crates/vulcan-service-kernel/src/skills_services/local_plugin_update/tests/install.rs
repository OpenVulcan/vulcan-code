//! First installs run the update transaction with no predecessor and recover through the update request RPCs.
//! 首次安装以空前驱运行更新交易，并经更新请求 RPC 恢复。

use super::*;

/// Install the fixture source through the production first-install coordinator.
/// 经生产首次安装协调器安装夹具来源。
async fn install_source(fixture: &Fixture) -> Result<LuaSkillsCatalogUpdate, String> {
    let preview = fixture
        .runtime
        .inspect_local_user_plugin(&fixture.source)
        .expect("candidate preview");
    install(
        &ServiceEventEmitter::new(Vec::new()),
        &fixture.state,
        fixture.source.to_string_lossy().into_owned(),
        preview.content_fingerprint,
        preview.permissions,
        false,
        Vec::new(),
    )
    .await
}

/// A first install selects a managed generation, publishes it disabled and leaves a completed request.
/// 首次安装选定受管代次，发布为停用，并留下已完成请求。
#[tokio::test]
async fn first_install_publishes_a_disabled_managed_generation() {
    let fixture = Fixture::uninstalled();
    assert!(!fixture.runtime.snapshot().plugins.contains_key(PLUGIN));
    let destination = fixture.root.path().join("provider.json");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination, async {
            install_source(&fixture).await.expect("first install");
            let installed = fixture.runtime.snapshot().plugins[PLUGIN].clone();
            assert_eq!(installed.runtime_state, VulcanPluginRuntimeState::Disabled);
            assert!(!installed.desired_enabled);
            assert_ne!(installed.package_root, fixture.source);
            let store =
                VulcanPluginGenerationStore::open_existing(fixture.runtime.user_plugin_root())
                    .expect("open store")
                    .expect("existing store");
            let selected = store
                .activation(PLUGIN)
                .expect("selection")
                .expect("selected");
            assert!(
                installed
                    .managed_package()
                    .is_some_and(|package| package.matches_activation(&selected))
            );
            assert!(
                provider_configuration_from_state(&fixture.state)
                    .expect("policy")
                    .luaskills
                    .disabled_vulcan_plugins
                    .contains(PLUGIN)
            );
            let records = local_plugin_update_records(Arc::clone(&fixture.state))
                .await
                .expect("request status");
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].phase, Some(Phase::Completed));
            assert!(!records[0].has_service_owner);

            // The namespace is now owned; a second install is rejected before any new request.
            // 命名空间已被占用；第二次安装在任何新请求之前被拒绝。
            install_source(&fixture)
                .await
                .expect_err("already installed");
            assert_eq!(
                store
                    .activation_transaction_ids(PLUGIN)
                    .expect("request ids")
                    .len(),
                1
            );
        })
        .await;
}

/// An interrupted first install keeps its service owner, is aborted through the update RPC, and does not block a
/// later install.
/// 中断的首次安装保留其服务所有者，经更新 RPC 中止，且不阻止后续安装。
#[tokio::test]
async fn interrupted_first_install_is_aborted_through_update_requests() {
    let fixture = Fixture::uninstalled();
    let manifest_path = fixture.source.join("plugin.yaml");
    let valid_manifest = std::fs::read_to_string(&manifest_path).expect("manifest");
    // Inert bytes cannot start an MCP process on any supported host, so readiness fails after preparation.
    // 惰性字节在任何支持的宿主上都无法启动 MCP 进程，因此就绪在准备之后失败。
    std::fs::write(
        fixture.source.join("server.bin"),
        b"invalid executable fixture",
    )
    .expect("inert executable");
    std::fs::write(
        &manifest_path,
        format!(
            "{valid_manifest}permissions: [process_binary, process_session]\nexecution:\n  native_binaries:\n    - id: server\n      targets:\n        x86_64-pc-windows-msvc: server.bin\n        aarch64-pc-windows-msvc: server.bin\n        x86_64-unknown-linux-gnu: server.bin\n        aarch64-unknown-linux-gnu: server.bin\n        x86_64-apple-darwin: server.bin\n        aarch64-apple-darwin: server.bin\nmcp:\n  servers:\n    - id: local\n      name: Local validation fixture\n      transport:\n        type: stdio\n        launcher: {{type: native_binary, binary_id: server}}\n"
        ),
    )
    .expect("native MCP declaration");
    let destination = fixture.root.path().join("provider.json");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination, async {
            install_source(&fixture)
                .await
                .expect_err("candidate readiness fails");
            assert!(!fixture.runtime.snapshot().plugins.contains_key(PLUGIN));
            let records = local_plugin_update_records(Arc::clone(&fixture.state))
                .await
                .expect("request status");
            assert_eq!(records.len(), 1);
            assert!(records[0].has_service_owner);
            assert_ne!(records[0].phase, Some(Phase::Completed));

            let aborted = service_abort_local_plugin_update(
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
                records[0].request_id.clone(),
                true,
            )
            .await
            .expect("abort first install");
            assert_eq!(aborted.phase, Some(Phase::Aborted));
            let store =
                VulcanPluginGenerationStore::open_existing(fixture.runtime.user_plugin_root())
                    .expect("open store")
                    .expect("existing store");
            assert_eq!(store.activation(PLUGIN).expect("selection"), None);
            assert!(!fixture.runtime.snapshot().plugins.contains_key(PLUGIN));

            // A corrected package installs normally after the terminal abort.
            // 终态中止之后，修正的插件包正常安装。
            std::fs::write(&manifest_path, valid_manifest).expect("restore manifest");
            install_source(&fixture).await.expect("install after abort");
            assert_eq!(
                fixture.runtime.snapshot().plugins[PLUGIN].runtime_state,
                VulcanPluginRuntimeState::Disabled
            );
        })
        .await;
}

/// A leftover explicit enable cannot be honoured by a disabled first install, so it is rejected before any request.
/// 停用的首次安装无法兑现残留的显式启用，因此在任何请求之前拒绝。
#[tokio::test]
async fn first_install_rejects_a_leftover_enable_override() {
    let fixture = Fixture::uninstalled();
    fixture
        .state
        .lock()
        .expect("service state")
        .agent_runtime
        .as_mut()
        .expect("bridge")
        .provider_configuration
        .luaskills
        .enabled_vulcan_plugins
        .insert(PLUGIN.to_owned());
    install_source(&fixture)
        .await
        .expect_err("enable override rejected");
    let store = VulcanPluginGenerationStore::open_existing(fixture.runtime.user_plugin_root())
        .expect("open store");
    assert!(store.is_none_or(|store| {
        store
            .activation_transaction_ids(PLUGIN)
            .expect("request ids")
            .is_empty()
    }));
    assert!(!fixture.runtime.snapshot().plugins.contains_key(PLUGIN));
}
