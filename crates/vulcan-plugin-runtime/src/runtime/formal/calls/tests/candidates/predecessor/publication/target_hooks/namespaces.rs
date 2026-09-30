//! Actual unrelated namespace refresh survives original target completion uncertainty.
//! 实际无关命名空间刷新跨原目标完成不确定性保持保留。

use super::*;

/// Refresh another real namespace after initialization, then retain its exact latest closed publication after Unknown.
/// 初始化之后刷新另一真实命名空间，并在 Unknown 之后保留其精确最新关闭发布。
/// No inputs or return value; real store writes, core identities and native counters prove same-owner recovery.
/// 无输入与返回值；真实存储写入、核心身份及原生计数证明同所有者恢复。
#[test]
fn formal_private_target_hook_unknown_preserves_latest_closed_namespace_and_original_vm() {
    // Use the production-selected immutable target and original exclusive request.
    // 使用生产选择的不可变目标及原排他请求。
    let (fixture, store, mut transaction) = published_enabled_target();
    // Initialize only the original private target against its first real closed route.
    // 仅针对首个真实关闭路由初始化原私有目标。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("original initial staging");
    executor().block_on(async {
        fixture
            .manager
            .prewarm_activation_target_hook(&transaction, &staged)
            .await
            .expect("original target initialization");
        // Preserve original pool, session, operation and native domain identities before unrelated publication.
        // 无关发布之前保留原池、会话、操作及原生域身份。
        let original = original_hook(&fixture);
        // Actual retained core identities are read from the original registry instance.
        // 实际保留核心身份从原注册表实例读取。
        let identities = original.target_test_identity();
        // Observe the sole original native domain without extending its physical ownership.
        // 观测唯一原原生域，不延长其物理所有权。
        let domain = fixture
            .owner()
            .host
            .target_test_domain_identity(identities.0.as_deref().expect("original pool"));
        // Count real target initialization events to reject any recovery replay.
        // 计数真实目标初始化事件，以拒绝任何恢复重放。
        let count = target_initializations(&fixture);
        // The fixture's actual bundled source root is empty after the original package moved to immutable management.
        // 原包移动至不可变管理之后，夹具实际内建来源根为空。
        let bundled = fixture._root.path().join("bundled");
        crate::runtime::tests::write_hook_package(&bundled);
        // Give the newly written real package a distinct manifest namespace before discovery.
        // 在发现之前，赋予新写入真实包独立清单命名空间。
        let source = bundled.join("hook-plugin");
        // Replace the exact declared identity, preserving the existing valid Hook package contract.
        // 替换精确声明身份，保留既有有效 Hook 包契约。
        let manifest = std::fs::read_to_string(source.join("plugin.yaml"))
            .expect("actual other namespace source")
            .replace("id: hook-plugin\n", "id: other-plugin\n");
        std::fs::write(source.join("plugin.yaml"), manifest)
            .expect("actual other namespace manifest");
        std::fs::rename(&source, bundled.join("other-plugin"))
            .expect("actual other namespace directory");
        // Production discovery publishes this independent namespace while preserving the original closed route generation.
        // 生产发现发布此独立命名空间，同时保留原关闭路由代次。
        let latest_closed = fixture
            .manager
            .refresh_catalog(
                &BTreeSet::new(),
                &BTreeSet::from(["hook-plugin".into(), "other-plugin".into()]),
            )
            .expect("real unrelated namespace refresh");
        assert!(!Arc::ptr_eq(&staged, &latest_closed));
        assert!(!staged.plugins.contains_key("other-plugin"));
        assert_eq!(
            latest_closed.plugins["hook-plugin"].generation,
            staged.plugins["hook-plugin"].generation
        );
        assert_eq!(
            latest_closed.plugins["hook-plugin"].runtime_state,
            crate::VulcanPluginRuntimeState::Enabling
        );
        assert_eq!(
            latest_closed.plugins["other-plugin"].runtime_state,
            crate::VulcanPluginRuntimeState::Enabled
        );
        assert!(
            latest_closed
                .enabled_hooks
                .values()
                .any(|hook| hook.plugin_id == "other-plugin")
        );
        // Only the real latest closed Arc can issue the successful full publisher witness.
        // 仅真实最新关闭 Arc 可签发成功完整发布器凭据。
        let publication = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &latest_closed, |_| Ok(()))
            .expect("latest complete publication");
        // Lose only this original successful Completed write and its immediate indexed observation.
        // 仅丢失此原成功 Completed 写入及其即时索引观测。
        let fault =
            store.lose_target_completion_observation_once(transaction.record().request_id());
        assert!(
            fixture
                .manager
                .complete_activation_target_handoff(&mut transaction, &publication)
                .is_err()
        );
        drop(fault);
        assert!(
            Arc::ptr_eq(&latest_closed, &fixture.manager.snapshot()),
            "Unknown restores the exact latest publisher input"
        );
        assert!(
            fixture
                .manager
                .snapshot()
                .enabled_hooks
                .values()
                .any(|hook| hook.plugin_id == "other-plugin")
        );
        assert_eq!(original.target_test_identity(), identities);
        assert_eq!(
            fixture
                .owner()
                .host
                .target_test_domain_identity(identities.0.as_deref().expect("same original pool")),
            domain
        );
        assert_eq!(target_initializations(&fixture), count);
        assert_eq!(
            transaction.record().phase(),
            crate::VulcanPluginActivationTransactionPhase::Published
        );
        assert_eq!(
            store
                .activation_request("hook-plugin", transaction.record().request_id())
                .expect("real canonical Completed read")
                .expect("original record")
                .phase(),
            crate::VulcanPluginActivationTransactionPhase::Completed
        );
        assert!(
            fixture
                .manager
                .ensure_formal_hook(
                    "hook-plugin",
                    staged.plugins["hook-plugin"].generation,
                    Instant::now() + WAIT
                )
                .await
                .is_err(),
            "private business remains closed after Unknown"
        );
        // A second successful publisher issues a new witness for the same original VM and preserved namespaces.
        // 第二次成功发布器为相同原 VM 及已保留命名空间签发新凭据。
        let recovered = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &latest_closed, |_| Ok(()))
            .expect("same-owner full publication recovery");
        assert!(
            fixture
                .manager
                .complete_activation_target_handoff(&mut transaction, &publication)
                .is_err(),
            "prior publisher witness cannot complete a later final Arc"
        );
        assert!(Arc::ptr_eq(
            recovered.catalog(),
            &fixture.manager.snapshot()
        ));
        fixture
            .manager
            .complete_activation_target_handoff(&mut transaction, &recovered)
            .expect("same-owner exact Completed confirmation");
        // Transfer keeps every actual original core identity without running initialization again.
        // 转交保留每个实际原核心身份，不再次运行初始化。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&transferred.0, &transferred.1, &transferred.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!transferred.3 && !transferred.4);
        assert_eq!(
            fixture.owner().host.target_test_domain_identity(
                identities.0.as_deref().expect("transferred original pool")
            ),
            domain
        );
        // Select only the actual target's public Hook from the recovered complete catalog.
        // 仅从已恢复完整目录选择实际目标公开 Hook。
        let hook = recovered
            .catalog()
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual recovered target Hook");
        for expected in [1, 2] {
            // The same retained module counts actual business calls after its one initialization.
            // 相同保留模块在单次初始化之后计数实际业务调用。
            let result = fixture
                .manager
                .invoke_formal_hook(
                    hook,
                    json!({"payload_json":"{}", "host_timeout_ms":1000}),
                    None,
                    Instant::now() + WAIT,
                )
                .await
                .expect("same original recovered business VM");
            assert_eq!(result["count"], expected);
        }
        assert_eq!(target_initializations(&fixture), count);
        assert!(
            fixture
                .manager
                .snapshot()
                .enabled_hooks
                .values()
                .any(|hook| hook.plugin_id == "other-plugin")
        );
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("actual recovered target drain");
        retire_original_hook(&fixture).await;
    });
}
