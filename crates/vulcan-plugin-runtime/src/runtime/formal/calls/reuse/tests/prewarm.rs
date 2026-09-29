//! Real host prewarming shares ordinary reusable routing and retained failure ownership.
//! 真实宿主预热共享普通复用路由及保留失败归属。

use super::*;

impl Fixture {
    /// Prewarm the currently published tool for controller and workspace; return the native instance result.
    /// 为 controller 及 workspace 预热当前已发布工具；返回原生实例结果。
    async fn prewarm(
        &self,
        controller: &str,
        workspace: PathBuf,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // The caller supplies an explicit generation from the public catalog, not an inferred latest generation.
        // 调用方从公开目录提供明确代次，而不是推断最新代次。
        let generation = self.manager.snapshot().enabled_tools["idle_ping"].plugin_generation;
        self.manager
            .prewarm_reusable_tool_instance(
                "idle-plugin",
                "idle_ping",
                generation,
                controller.to_owned(),
                workspace,
            )
            .await
    }
}

/// Prewarming allocates distinct VMs, preserves queryable full-pool failure and never runs business hygiene.
/// 预热分配不同 VM，保留可查询满池失败，且绝不执行业务清理流程。
#[tokio::test]
async fn hot_reuse_prewarm_additional_instances_preserve_business_and_failure() {
    // Raise only the real manifest's resident limit before any execution domain exists.
    // 在任何执行域存在前，仅提升真实清单的常驻上限。
    let fixture = Fixture::new(10_000, 100);
    // The authoritative package manifest selects the existing tool by stable identity.
    // 权威包清单按稳定身份选择既有工具。
    let path = fixture.root.path().join("bundled/idle-plugin/plugin.yaml");
    // Preserve unrelated declarations while permitting two real additional instances.
    // 保留无关声明，同时允许两个真实额外实例。
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest")).expect("mapping");
    manifest["lua_runtime"]["tools"]
        .as_array_mut()
        .expect("tools")
        .iter_mut()
        .find(|tool| tool["id"] == "idle_ping")
        .expect("ping tool")["execution"]["reusable"]["max_resident_vms"] = json!(2);
    std::fs::write(&path, serde_yaml::to_string(&manifest).expect("encoding"))
        .expect("write manifest");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish capacity");
    // Both requests have the exact controller and canonical workspace used by later ordinary calls.
    // 两个请求具有后续普通调用使用的精确控制器及规范工作区。
    let workspace = fixture.root.path().to_path_buf();
    // The two returned identities must come from two different actual native VM allocations.
    // 两个返回身份必须来自两个不同的真实原生 VM 分配。
    let first = fixture
        .prewarm("owner", workspace.clone())
        .await
        .expect("first prewarm");
    let second = fixture
        .prewarm("owner", workspace.clone())
        .await
        .expect("second prewarm");
    assert!(
        first["instance_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(
        second["instance_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert_ne!(first["instance_id"], second["instance_id"]);
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("actual usage")
            .resident,
        2
    );
    fixture
        .prewarm("owner", workspace.clone())
        .await
        .expect_err("full pool must fail");
    // Failed operations remain queryable under their original controller; another controller cannot observe them.
    // 失败操作在原控制器下保持可查询；其他控制器不能观测它们。
    let retained = fixture
        .manager
        .formal_calls("owner")
        .expect("retained failure");
    assert_eq!(retained.len(), 1);
    let failure = retained
        .iter()
        .find(|call| call.error.is_some())
        .expect("failed prewarm");
    let operation = failure.operation.as_ref().expect("core evidence");
    assert!(matches!(
        &operation.context,
        OperationContext::Module(context) if context.prewarm
    ));
    assert_eq!(
        operation.error.as_ref().expect("capacity error").code,
        EmbeddedErrorCode::CapacityExceeded
    );
    assert!(failure.retired);
    assert!(
        fixture
            .manager
            .formal_call_status(&failure.invocation_id, "other")
            .is_err()
    );
    assert_eq!(
        fixture
            .invoke("owner", workspace, "ok")
            .await
            .expect("ordinary business")["count"],
        json!(1)
    );
    fixture
        .manager
        .forget_formal_call(&failure.invocation_id, "owner")
        .expect("acknowledge failure");
    fixture.close().await;
}

/// Explicit prewarming rejects non-reusable, stale and ownerless requests before creating any VM.
/// 明确预热在创建任何 VM 前拒绝非复用、陈旧及无所有者请求。
#[tokio::test]
async fn hot_reuse_prewarm_validates_authority_and_preserves_partitions() {
    // Use one real reusable package and the production enabled catalog.
    // 使用单个真实可复用包及生产启用目录。
    let fixture = Fixture::new(10_000, 100);
    // Capture the declared generation before issuing either invalid or valid work.
    // 在发出无效或有效工作前捕获声明代次。
    let generation = fixture.manager.snapshot().enabled_tools["idle_ping"].plugin_generation;
    // Every request receives the actual existing workspace, never a fallback directory.
    // 每个请求获得真实现有工作区，绝不使用回退目录。
    let workspace = fixture.root.path().to_path_buf();
    for (tool, selected, controller) in [
        ("idle_open", generation, "owner"),
        ("missing", generation, "owner"),
        (
            "idle_ping",
            generation.checked_add(1).expect("different generation"),
            "owner",
        ),
        ("idle_ping", generation, " "),
    ] {
        fixture
            .manager
            .prewarm_reusable_tool_instance(
                "idle-plugin",
                tool,
                selected,
                controller.to_owned(),
                workspace.clone(),
            )
            .await
            .expect_err("invalid authority");
    }
    fixture
        .manager
        .prewarm_reusable_tool_instance(
            "foreign-plugin",
            "idle_ping",
            generation,
            "owner".to_owned(),
            workspace.clone(),
        )
        .await
        .expect_err("same generation cannot authorize a foreign plugin");
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("no VM")
            .resident,
        0
    );
    assert!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("no admitted calls")
            .is_empty()
    );
    fixture
        .prewarm("owner", workspace.clone())
        .await
        .expect("owned warm VM");
    assert_eq!(
        fixture
            .invoke("other", workspace.clone(), "ok")
            .await
            .expect("other controller")["count"],
        json!(1)
    );
    assert_eq!(
        fixture
            .invoke("owner", workspace.clone(), "ok")
            .await
            .expect("warm controller")["count"],
        json!(1)
    );
    // A different real directory creates a distinct domain even for the same controller and tool.
    // 即使控制器及工具相同，不同真实目录仍创建不同域。
    let other = workspace.join("other");
    std::fs::create_dir(&other).expect("other workspace");
    fixture
        .prewarm("owner", other.clone())
        .await
        .expect("other workspace warm VM");
    assert_eq!(
        fixture
            .invoke("owner", other, "ok")
            .await
            .expect("other workspace business")["count"],
        json!(1)
    );
    assert_eq!(
        fixture
            .invoke("owner", workspace, "ok")
            .await
            .expect("original state")["count"],
        json!(2)
    );
    fixture.close().await;
}
