//! Actual restored native guards transfer only after their same independent predecessor writer confirms.
//! 实际恢复原生守卫仅在其相同独立前驱写入器确认后转交。

use super::*;
use crate::agent_runtime::extension_generation::ExtensionGeneration;
use vulcan_plugin_runtime::{
    VulcanPluginActivationPredecessorPublication, VulcanPluginActivationPredecessorRegistration,
    VulcanPluginNativeMcpPredecessorHandoffAdapter, VulcanPluginPredecessorHandoffPhase,
};

/// A test adapter borrows genuine native guards before deliberately abandoning or obstructing readback.
/// 测试适配器借用真正原生守卫，然后故意放弃或阻碍回读。
struct InterruptPredecessorTransfer<'a> {
    /// The actual original runtime supplies every resource, source authority and instance identity.
    /// 实际原运行时提供每个资源、来源权威及实例身份。
    native: &'a PredecessorPluginMcpRuntime,
    /// A concrete sidecar path selects readback obstruction; absence explicitly selects guard abandonment.
    /// 具体附属记录路径选择回读阻碍；缺失明确选择守卫放弃。
    obstruct_readback: Option<&'a std::path::Path>,
}

impl VulcanPluginNativeMcpPredecessorHandoffAdapter for InterruptPredecessorTransfer<'_> {
    /// Genuine guards cannot escape the actual borrowed registration scope.
    /// 真正守卫不能逃逸实际借用注册作用域。
    type Guard<'gate>
        = crate::mcp::PredecessorPluginMcpHandoffGuard<'gate>
    where
        Self: 'gate;

    /// Borrow the same originally registered issuer; no input and no replacement authority.
    /// 借用相同原登记签发者；无输入，不替换权威。
    fn authority(&self) -> Option<&VulcanPluginRegisteredNativeMcpHostAuthority> {
        VulcanPluginNativeMcpPredecessorHandoffAdapter::authority(self.native)
    }

    /// The exact original arguments acquire actual guards; return them after obstruction, or abandon with an error.
    /// 精确原参数取得实际守卫；阻碍之后返回它们，或放弃并返回错误。
    fn prepare<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
        registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, VulcanPluginRuntimeError> {
        // Retain actual source and native locks before affecting the ensuing real durable writer.
        // 影响随后真实持久写入器之前，保留实际来源及原生锁。
        let guard = self
            .native
            .prepare_handoff(manager, transaction, source, publication, registration)
            .map_err(VulcanPluginRuntimeError::new)?;
        match self.obstruct_readback {
            Some(path) => {
                fs::write(path, b"invalid predecessor readback")
                    .expect("obstruct exact actual predecessor sidecar");
                Ok(guard)
            }
            None => {
                drop(guard);
                Err(VulcanPluginRuntimeError::new(
                    "Deliberately abandon actual predecessor transfer.",
                ))
            }
        }
    }
}

/// Start the same old physical preparation and observe genuine protocol discovery; fixture fixes every input.
/// 启动相同旧物理准备并观测真正协议发现；fixture 固定每个输入。
/// Return its actual controller without copying receipts or supplying arbitrary native identities.
/// 返回实际控制器，不复制回执或提供任意原生身份。
async fn ready_native(fixture: &OldFixture) -> PredecessorPluginMcpRuntime {
    // The source owns the original old selection and its existing independent File.
    // 来源拥有原旧选择及其既有独立文件。
    let prepared = fixture
        .base
        .manager
        .prepare_predecessor_mcp_resources(&fixture.source, &fixture.base.native_host)
        .expect("same physical old preparation");
    // Definitions remain sealed to those exact physical resources.
    // 定义保持封闭至这些精确物理资源。
    let configs = predecessor_global_mcp_server_configs(prepared, &NoSecrets)
        .expect("same original old inputs");
    // Production dispatch supplies actual runtime and lease identities before observation.
    // 生产派发在观察前提供实际运行时及租约身份。
    let native = PredecessorPluginMcpRuntime::start(
        &fixture.access,
        &fixture.base.manager,
        &fixture.base.transaction,
        configs,
        &fixture.base.native_host,
    )
    .expect("real restored native startup");
    native
        .wait_until_ready_default()
        .await
        .expect("actual restored native Ready");
    native
}

/// Publish an actual test-owned complete extension slot containing this exact old catalog and native execution lease.
/// 发布实际测试持有完整扩展槽，包含此精确旧目录及原生执行租约。
/// fixture/native supply original authorities; return the successful core seal and actual committed Full Arc.
/// fixture／native 提供原权威；返回成功核心封闭凭据及实际已提交 Full Arc。
/// This verifies the public publisher boundary and does not claim the production service slot is covered.
/// 此测试校验公共发布边界，不声称覆盖生产服务槽。
fn publish_full(
    fixture: &OldFixture,
    native: &PredecessorPluginMcpRuntime,
) -> (
    VulcanPluginActivationPredecessorPublication,
    Arc<ExtensionGeneration>,
) {
    // The expected Full allocation is real and contains the fixture's actual closed predecessor catalog.
    // 预期 Full 分配真实存在，包含夹具实际关闭前驱目录。
    let expected = Arc::new(ExtensionGeneration::new(
        1,
        Some(fixture.base.manager.snapshot()),
        fixture.access.clone(),
        None,
    ));
    // Publication derives the same actual prepared execution lease before source registration locks.
    // 发布在来源注册锁之前派生相同实际已准备执行租约。
    let access = native
        .publication_access(&expected.mcp_runtime())
        .expect("actual same native publication access");
    // This local slot is the real recipient of the production ExtensionGeneration pointer writer.
    // 此局部槽为生产 ExtensionGeneration 指针写入器的真实接收者。
    let mut slot = Arc::clone(&expected);
    // Only a successful actual pointer write can produce the returned sealed source publication.
    // 仅成功实际指针写入可以产出返回的封闭来源发布。
    let seal = fixture
        .base
        .manager
        .publish_activation_predecessor_handoff(
            &fixture.base.transaction,
            &fixture.source,
            |catalog| {
                // No Hook is declared by this fixture; its complete standalone generation has exact catalog and native access.
                // 此夹具未声明 Hook；其完整独立代次包含精确目录及原生访问。
                let next = Arc::new(ExtensionGeneration::new(2, Some(catalog), access, None));
                ExtensionGeneration::publish_prepared(&mut slot, &expected, next, |_| {})?;
                Ok(())
            },
        )
        .expect("real old complete extension publication");
    assert!(!Arc::ptr_eq(&slot, &expected));
    assert!(Arc::ptr_eq(
        &slot.plugin_catalog().expect("actual Full catalog"),
        seal.catalog()
    ));
    (seal, slot)
}

/// Open a read-only contention probe against the independently reserved original recovery File.
/// 对独立预留原恢复文件打开只读争用探针。
/// fixture supplies the actual original UUID; return a File without creating or replacing any ownership.
/// fixture 提供实际原 UUID；返回文件，不创建或替换任何所有权。
fn recovery_probe(fixture: &OldFixture) -> std::fs::File {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                .base
                ._temp
                .path()
                .join("user/.vulcan-generations/locks")
                .join(format!(
                    "target-mcp.predecessor-handoff.{}.lock",
                    fixture.base.transaction.record().request_id(),
                )),
        )
        .expect("existing actual independent recovery File")
}

/// Actual native abandonment retains the original, then successful Transferred opens the same PID and releases only recovery ownership.
/// 实际原生放弃保留原实例，随后成功 Transferred 开放相同 PID，并仅释放恢复所有权。
/// No inputs or result; protocol calls, actual File contention and durable records prove the boundary.
/// 无输入及结果；协议调用、实际文件争用及持久记录证明此边界。
#[tokio::test]
async fn predecessor_native_joint_transfer_opens_same_original_and_releases_recovery_file() {
    // Actual old completion and physical retirement precede this one restoration.
    // 此单次恢复之前完成实际旧完成及物理退役。
    let fixture = old_fixture().await;
    // Genuine protocol observation supplies ready evidence, never an injected diagnostic status.
    // 真正协议观察提供就绪证据，绝不注入诊断状态。
    let native = ready_native(&fixture).await;
    // Remember only actual allocated instance identities and real process marker bytes.
    // 仅记录实际分配实例身份及真实进程标记字节。
    let ids = native.actual_instance_ids();
    // The restored child is the third actual distinct startup.
    // 恢复子进程为第三次实际独立启动。
    let starts = fs::read_to_string(&fixture.base.marker).expect("actual startup markers");
    assert_eq!(starts.lines().count(), 3);
    // The existing original record must remain Aborted even after restoration transfer.
    // 即使恢复转交之后，既有原记录也必须保持 Aborted。
    let aborted = fixture.base.transaction.record().clone();
    // The physical recovery File remains locked before and after complete publication.
    // 完整发布之前及之后，物理恢复文件均保持锁定。
    let probe = recovery_probe(&fixture);
    assert!(matches!(
        probe.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // A binding carries the actual preparing lease and is closed until its same source transfer.
    // 绑定携带实际准备租约，在相同来源转交前保持关闭。
    let binding = native
        .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .expect("actual closed native bindings")
        .into_iter()
        .find(|binding| binding.origin_tool_name() == Some("read_page"))
        .expect("read_page declaration may have changed");
    // Actual old business returns the same restored PID after confirmation.
    // 确认后，实际旧业务返回相同恢复 PID。
    let args =
        serde_json::json!({"url":"https://restored-old.test","owner_session_id":"fixture-owner"});
    assert!(binding.invoke(args.clone()).await.is_err());
    // Full commits exact resources while all admission fences are still in force.
    // Full 提交精确资源，同时全部入场屏障仍生效。
    let (seal, full) = publish_full(&fixture, &native);
    // This real adapter abandons genuine guards without any completed writer or business opening.
    // 此真实适配器放弃真正守卫，不执行完成写入器或业务开放。
    let interrupted = InterruptPredecessorTransfer {
        native: &native,
        obstruct_readback: None,
    };
    assert!(
        fixture
            .base
            .manager
            .complete_activation_predecessor_handoff_with(
                &fixture.base.transaction,
                &fixture.source,
                &seal,
                &interrupted,
            )
            .expect_err("guard abandonment must reject transfer")
            .message
            .contains("Deliberately abandon")
    );
    assert!(Arc::ptr_eq(
        &fixture.base.manager.snapshot(),
        seal.catalog()
    ));
    assert!(matches!(
        probe.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(binding.invoke(args.clone()).await.is_err());
    assert!(
        fixture
            .base
            .store
            .predecessor_handoff_blocks_admission("target-mcp")
            .expect("still pending")
    );
    fixture
        .base
        .manager
        .complete_activation_predecessor_handoff_with(
            &fixture.base.transaction,
            &fixture.source,
            &seal,
            &native,
        )
        .expect("genuine same original predecessor joint transfer");
    assert_eq!(fixture.base.transaction.record(), &aborted);
    assert_eq!(
        fixture
            .base
            .store
            .predecessor_handoff_record("target-mcp", aborted.request_id())
            .expect("durable predecessor record")
            .expect("same original predecessor record")
            .phase(),
        VulcanPluginPredecessorHandoffPhase::Transferred
    );
    probe
        .try_lock()
        .expect("confirmed transfer releases the independent recovery File");
    probe.unlock().expect("release only test probe");
    assert!(
        !fixture
            .base
            .store
            .predecessor_handoff_blocks_admission("target-mcp")
            .expect("actual File released")
    );
    assert_eq!(native.actual_instance_ids(), ids);
    native
        .wait_until_ready_default()
        .await
        .expect("transferred actual readiness is read-only");
    fixture
        .access
        .finish_plugin_owner_drain("target-mcp")
        .expect("same original local native fence opens after transfer");
    assert!(
        !full
            .mcp_runtime()
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // The actual response identifies the already observed restored process, proving no replacement startup.
    // 实际响应标识已观察恢复进程，证明没有替代启动。
    let result = binding
        .invoke(args)
        .await
        .expect("same restored native business");
    assert!(
        result
            .to_string()
            .contains(starts.lines().last().expect("actual restored PID"))
    );
    assert!(
        fixture
            .base
            .manager
            .complete_activation_predecessor_handoff_with(
                &fixture.base.transaction,
                &fixture.source,
                &seal,
                &native,
            )
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(&fixture.base.marker).expect("no replacement startup"),
        starts
    );
    // Explicit real drain retires the transferred transport while historical control remains alive.
    // 显式真实排空退役已转交传输，同时历史控制继续存活。
    let observation = native.observer();
    fixture
        .access
        .begin_plugin_mcp_drain("target-mcp")
        .expect("actual transferred old drain");
    fixture
        .access
        .wait_plugin_mcp_drain("target-mcp", OBSERVE)
        .await
        .expect("actual native retirement");
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("actual transferred physical release");
    assert!(binding.invoke(serde_json::json!({})).await.is_err());
}

/// Actual readback failure retains the final Full and closed same native instance until exact original recovery succeeds.
/// 实际回读失败保留最终 Full 及关闭相同原生实例，直到精确原恢复成功。
/// No inputs or result; only the real source sidecar is obstructed and restored, never evidence or native identity.
/// 无输入及结果；仅阻碍并恢复真实来源附属记录，绝不替换证据或原生身份。
#[tokio::test]
async fn predecessor_native_joint_readback_failure_keeps_final_seal_and_same_actual_owners() {
    // This fixture retains one genuine independent original and its restored actual native process.
    // 此夹具保留一个真正独立原请求及其恢复实际原生进程。
    let fixture = old_fixture().await;
    // Native readiness is observed before publication without restarting any already initialized resource.
    // 在不重启任何已初始化资源的情况下，发布前观察原生就绪。
    let native = ready_native(&fixture).await;
    // Capture actual child marker and actual instance identities before failure.
    // 在失败前捕获实际子进程标记及实际实例身份。
    let starts = fs::read_to_string(&fixture.base.marker).expect("actual restored startup");
    // Only the runtime's actual owners supply the instance pair.
    // 仅运行时实际所有者提供实例对。
    let ids = native.actual_instance_ids();
    // The exact production sidecar is derived from its existing original transaction directory.
    // 精确生产附属记录由其既有原事务目录派生。
    let path = fixture
        .base
        ._temp
        .path()
        .join("user/.vulcan-generations/transactions/target-mcp/predecessor-handoffs")
        .join(format!(
            "{}.json",
            fixture.base.transaction.record().request_id()
        ));
    // Restore these original bytes after failure before asserting the observed outcome.
    // 失败后先恢复这些原字节，再断言观察结果。
    let original = fs::read(&path).expect("actual original Pending sidecar");
    // The probe observes the actual independently held File rather than a diagnostic resource count.
    // 探针观察实际独立持有文件，而非诊断资源计数。
    let probe = recovery_probe(&fixture);
    // Preparing binding stays tied to the same genuine native lease through the fault.
    // 准备绑定跨故障保持绑定至相同真正原生租约。
    let binding = native
        .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .expect("actual old bindings")
        .into_iter()
        .find(|binding| binding.origin_tool_name() == Some("read_page"))
        .expect("read_page declaration may have changed");
    // Publication writes the actual complete test slot and produces its sealed provenance.
    // 发布写入实际完整测试槽，并产出其封闭来源。
    let (seal, _full) = publish_full(&fixture, &native);
    // Genuine guarded preparation is completed before the exact readback is deliberately made invalid.
    // 精确回读被故意置为无效之前，先完成真正受保护准备。
    let interrupted = InterruptPredecessorTransfer {
        native: &native,
        obstruct_readback: Some(&path),
    };
    // Retain the result while immediately restoring the same exact sidecar image.
    // 保留结果，同时立即恢复相同精确附属记录映像。
    let outcome = fixture
        .base
        .manager
        .complete_activation_predecessor_handoff_with(
            &fixture.base.transaction,
            &fixture.source,
            &seal,
            &interrupted,
        );
    fs::write(&path, &original).expect("restore exact original Pending bytes");
    assert!(outcome.is_err());
    assert!(Arc::ptr_eq(
        &fixture.base.manager.snapshot(),
        seal.catalog()
    ));
    assert!(matches!(
        probe.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(binding.invoke(serde_json::json!({})).await.is_err());
    assert_eq!(native.actual_instance_ids(), ids);
    fixture
        .base
        .manager
        .complete_activation_predecessor_handoff_with(
            &fixture.base.transaction,
            &fixture.source,
            &seal,
            &native,
        )
        .expect("same actual owner succeeds after exact Pending readback restoration");
    probe
        .try_lock()
        .expect("same confirmed writer releases recovery File");
    probe.unlock().expect("unlock test probe");
    fixture
        .access
        .finish_plugin_owner_drain("target-mcp")
        .expect("confirmed same native owner fence");
    assert!(binding.invoke(serde_json::json!({"url":"https://readback-old.test","owner_session_id":"fixture-owner"})).await.is_ok());
    assert_eq!(native.actual_instance_ids(), ids);
    assert_eq!(
        fs::read_to_string(&fixture.base.marker).expect("no fault replay startup"),
        starts
    );
    // Retire the same actual process explicitly so this test leaves no background transport work.
    // 显式退役相同实际进程，避免此测试留下后台传输工作。
    let observation = native.observer();
    fixture
        .access
        .begin_plugin_mcp_drain("target-mcp")
        .expect("readback original drain");
    fixture
        .access
        .wait_plugin_mcp_drain("target-mcp", OBSERVE)
        .await
        .expect("readback actual retirement");
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("readback actual physical release");
}
