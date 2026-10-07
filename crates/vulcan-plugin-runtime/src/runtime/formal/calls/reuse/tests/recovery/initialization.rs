//! Real bundle initialization narrows callbacks consistently for cold business and automatic prewarming.
//! 真实包初始化为冷业务及自动预热一致收窄回调。

use super::*;

/// Missing and explicit initialization declarations govern real host effects while later business remains authorized.
/// 缺失及显式初始化声明约束真实宿主副作用，同时后续业务仍获授权。
#[tokio::test]
async fn hot_reuse_initialization_policy_applies_to_cold_calls_and_prewarm() {
    for allow_initialization in [false, true] {
        for prewarm in [false, true] {
            // The real package already grants UI business access; only source initialization is narrowed here.
            // 真实包已经授予 UI 业务访问；此处仅收窄源码初始化。
            let recovery = RecoveryFixture::new(false);
            let fixture = &recovery.fixture;
            let manifest_path = fixture.root.path().join("bundled/idle-plugin/plugin.yaml");
            if allow_initialization {
                let mut manifest: Value = serde_yaml::from_str(
                    &std::fs::read_to_string(&manifest_path).expect("manifest"),
                )
                .expect("mapping");
                manifest["lua_runtime"]["tools"]
                    .as_array_mut()
                    .expect("tools")
                    .iter_mut()
                    .find(|tool| tool["id"] == "idle_ping")
                    .expect("ping tool")["execution"]["reusable"]["initialization_capabilities"] =
                    json!(["vulcan.plugin.ui.invalidate"]);
                std::fs::write(
                    &manifest_path,
                    serde_yaml::to_string(&manifest).expect("encode"),
                )
                .expect("write manifest");
            }
            // Source records actual admission; it cannot select its own trusted execution phase.
            // 源码记录真实入场结果；它不能选择自身可信执行阶段。
            let source_path = fixture
                .root
                .path()
                .join("bundled/idle-plugin/runtime/plugin.bundle.lua");
            let source = std::fs::read_to_string(&source_path).expect("source");
            let prefix = format!(
                "local allowed, result = pcall(vulcan.host.call, 'vulcan.plugin.ui.invalidate', {{topic='initialization-policy',revision=0}})\nassert(allowed == {allow_initialization}, 'unexpected initialization authority')\nif not allowed then assert(string.find(tostring(result), 'not authorized during module initialization', 1, true)) end\n"
            );
            std::fs::write(&source_path, format!("{prefix}{source}")).expect("instrument source");
            fixture
                .manager
                .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
                .expect("publish policy");
            let workspace = fixture.root.path().to_path_buf();
            if prewarm {
                fixture.converge_target(fixture.target(1)).await;
            }
            assert_eq!(
                fixture
                    .invoke("owner", workspace, "ok")
                    .await
                    .expect("ordinary business")["count"],
                1
            );
            // Host-side observations prove source denial without inferring safety from Lua's returned value alone.
            // 宿主侧观测证明源码被拒绝，不仅根据 Lua 返回值推断安全。
            {
                let values = recovery.events.values.lock().expect("real host events");
                assert_eq!(
                    values
                        .iter()
                        .filter(|event| event.topic == "initialization-policy")
                        .count(),
                    usize::from(allow_initialization)
                );
                assert_eq!(
                    values
                        .iter()
                        .filter(|event| event.topic == "business")
                        .count(),
                    1
                );
            }
            fixture.close().await;
        }
    }
}

/// An initialization declaration cannot authorize missing callbacks or capabilities absent from plugin grants.
/// 初始化声明不能授权缺失回调或插件授权之外的能力。
#[tokio::test]
async fn hot_reuse_initialization_policy_cannot_expand_plugin_authority() {
    for name in [
        "vulcan.plugin.ui.invalidate",
        "missing.initialization.callback",
    ] {
        // This fixture grants configuration only, so a declared UI callback must remain unauthorized.
        // 此夹具仅授予配置权限，因此声明的 UI 回调必须仍未授权。
        let fixture = Fixture::new(10_000, 100);
        let manifest_path = fixture.root.path().join("bundled/idle-plugin/plugin.yaml");
        let mut manifest: Value =
            serde_yaml::from_str(&std::fs::read_to_string(&manifest_path).expect("manifest"))
                .expect("mapping");
        manifest["lua_runtime"]["tools"]
            .as_array_mut()
            .expect("tools")
            .iter_mut()
            .find(|tool| tool["id"] == "idle_ping")
            .expect("ping tool")["execution"]["reusable"]["initialization_capabilities"] =
            json!([name]);
        std::fs::write(
            &manifest_path,
            serde_yaml::to_string(&manifest).expect("encode"),
        )
        .expect("write manifest");
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish declaration");
        let owner = fixture.owner();
        let plugin = fixture.manager.snapshot().plugins["idle-plugin"].clone();
        let manifest = plugin.manifest.as_ref().expect("validated manifest");
        assert_eq!(
            owner
                .host
                .validate_manifest_capabilities(manifest, VulcanPluginBridgeLeaseScope::Enabled)
                .expect_err("activation cannot create callback grants")
                .code,
            EmbeddedErrorCode::PermissionDenied
        );
        assert!(
            !fixture
                .manager
                .snapshot()
                .enabled_tools
                .contains_key("idle_ping"),
            "unauthorized initialization declaration must be withheld at catalog admission"
        );
        fixture
            .invoke("owner", fixture.root.path().to_path_buf(), "ok")
            .await
            .expect_err("ordinary cold admission must reject");
        assert_eq!(
            owner
                .host
                .runtime()
                .resources()
                .expect("actual usage")
                .resident,
            0
        );
        fixture.close().await;
    }
}
