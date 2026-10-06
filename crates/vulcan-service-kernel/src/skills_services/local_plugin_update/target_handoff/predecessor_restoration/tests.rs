//! Real old credential reads cannot retire the first physical predecessor preparation on an ordinary error.
//! 真实旧凭据读取不能在普通错误后退役首次物理前驱准备。

use super::*;
use crate::skills_services::vulcan_plugin_lifecycle::generation_tests::published::{
    fixture_with_old_secret,
    predecessor::{failed_update_with_fixture, handoff_file, starts},
};
use std::sync::atomic::{AtomicBool, Ordering};
use vulcan_plugin_runtime::{
    VulcanPluginCredentialBackend, VulcanPluginCredentialBinding, VulcanPluginCredentialKey,
    VulcanPluginCredentialRevision, VulcanPluginPredecessorHandoffPhase,
};

mod mixed;

/// Isolated backend retains the actual staging writer's tuples and records the materializer's complete frozen binding.
/// 隔离后端保留实际暂存写入器的元组，并记录物化器的完整冻结绑定。
#[derive(Default)]
struct OldCredentials {
    /// Real staging writes and readbacks use one exact plugin, revision and declared key without external storage.
    /// 真实暂存写入及回读使用单个精确插件、修订及声明键，不使用外部存储。
    values: Mutex<
        BTreeMap<
            (
                String,
                VulcanPluginCredentialRevision,
                VulcanPluginCredentialKey,
            ),
            String,
        >,
    >,
    /// This switch fails only MCP materialization reads, after the original A has already consumed its actual value.
    /// 此开关仅让 MCP 物化读取失败，此时原 A 已消费其实际值。
    fail_materialization: AtomicBool,
    /// Complete read observations preserve old binding evidence without retaining any physical prepared owner.
    /// 完整读取观测保留旧绑定证据，不保留任何物理准备所有者。
    reads: Mutex<Vec<(String, String, String, VulcanPluginCredentialBinding)>>,
}

impl VulcanPluginCredentialBackend for OldCredentials {
    /// Read plugin_id/revision/key from actual isolated writes; return true absence without guessing another revision.
    /// 从实际隔离写入读取 plugin_id／revision／key；返回真实缺失，不猜测另一修订。
    fn read(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Ok(self
            .values
            .lock()
            .map_err(|error| error.to_string())?
            .get(&(String::from(plugin_id), revision.clone(), key.clone()))
            .cloned())
    }

    /// Store value under the staging writer's complete plugin_id/revision/key; return only the write outcome.
    /// 在暂存写入器的完整 plugin_id／revision／key 下保存 value；仅返回写入结果。
    fn write(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|error| error.to_string())?
            .insert(
                (String::from(plugin_id), revision.clone(), key.clone()),
                String::from(value),
            );
        Ok(())
    }

    /// Delete only plugin_id/revision/key from isolated storage; absent entries succeed without another lookup.
    /// 仅从隔离存储删除 plugin_id／revision／key；缺失条目成功，不查询其他位置。
    fn delete(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|error| error.to_string())?
            .remove(&(String::from(plugin_id), revision.clone(), key.clone()));
        Ok(())
    }
}

impl PluginMcpSecretStore for OldCredentials {
    /// Record plugin_id/server_id/secret_key/binding before failing or reading that exact staged revision.
    /// 在失败或读取精确暂存修订之前，记录 plugin_id／server_id／secret_key／binding。
    /// Return the actual stored value after recovery; unconfigured and missing values are explicit errors.
    /// 恢复后返回实际存储值；未配置及缺失值为明确错误。
    fn read_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
        binding: &VulcanPluginCredentialBinding,
    ) -> Result<String, String> {
        self.reads.lock().map_err(|error| error.to_string())?.push((
            String::from(plugin_id),
            String::from(server_id),
            String::from(secret_key),
            binding.clone(),
        ));
        if self.fail_materialization.load(Ordering::Acquire) {
            return Err(String::from(
                "Declared old credential materialization failure.",
            ));
        }
        // Only the declaration's frozen revision may address the value actually written by credential staging.
        // 仅声明的冻结修订可寻址凭据暂存实际写入的值。
        let VulcanPluginCredentialBinding::Revision(revision) = binding else {
            return Err(String::from(
                "Actual old fixture requires a revision binding.",
            ));
        };
        self.read(
            plugin_id,
            revision,
            &VulcanPluginCredentialKey {
                server_id: String::from(server_id),
                secret_key: String::from(secret_key),
            },
        )?
        .ok_or_else(|| String::from("Actual staged old credential is missing."))
    }
}

/// Failed real materialization retains the original physical preparation, so the same source retries without rebuilding it.
/// 真实物化失败保留原物理准备，使相同来源无须重建即可重试。
/// No inputs or return; actual staging, retired Node A, Pending File, frozen binding and pure source identity prove this boundary.
/// 无输入或返回值；实际暂存、已退役 Node A、Pending 文件、冻结绑定及纯来源身份证明此边界。
#[tokio::test]
async fn failed_old_secret_read_retains_same_physical_preparation_for_retry() {
    // This isolated value is staged by production and consumed by the genuine old A before candidate failure.
    // 此隔离值由生产暂存，并在候选失败之前由真实旧 A 消费。
    let value = "actual-staged-old-service-token";
    // The same backend remains alive for actual staging, initial materialization and the later failed/recovered reads.
    // 相同后端跨实际暂存、首次物化及随后失败／恢复读取保持存活。
    let credentials = OldCredentials::default();
    // The real service update removes only the old secret env from its candidate, then physically retires A.
    // 真实服务更新仅从候选删除旧秘密环境输入，随后物理退役 A。
    let fixture = failed_update_with_fixture(
        fixture_with_old_secret(&credentials, &credentials, value),
        true,
    )
    .await;
    // Read the real one-start evidence before any predecessor materialization can launch native work.
    // 在任何前驱物化能够启动原生工作之前，读取真实单次启动证据。
    let original_starts = starts(&fixture.marker);
    assert_eq!(original_starts.len(), 1);
    assert_eq!(
        original_starts.first().expect("actual retired A")["token"],
        value
    );
    // Freeze the complete actual old binding and prove the candidate owns a distinct credential revision.
    // 固定完整实际旧绑定，并证明候选拥有不同凭据修订。
    let old_revision = fixture
        .prepared
        .predecessor()
        .expect("actual managed predecessor")
        .credential_revision()
        .expect("managed predecessor credential revision")
        .clone();
    assert_ne!(
        fixture.prepared.target().credential_revision(),
        Some(&old_revision)
    );
    // Exact tuple equality checks every binding component rather than relying on a copied UUID alone.
    // 精确元组相等检查每个绑定组件，不仅依赖复制 UUID。
    let expected_read = (
        String::from("published-service"),
        String::from("actual"),
        String::from("old_token"),
        VulcanPluginCredentialBinding::Revision(old_revision),
    );
    assert_eq!(
        *credentials
            .reads
            .lock()
            .expect("actual initial secret read"),
        vec![expected_read.clone()]
    );
    credentials
        .reads
        .lock()
        .expect("clear initial observation")
        .clear();
    credentials
        .fail_materialization
        .store(true, Ordering::Release);
    // Use the genuine service registry and its already retained original owner, never test-only retain/new registration.
    // 使用真实服务注册表及其已经保留原所有者，绝不使用测试专用 retain 或新注册。
    let registry =
        pending_owner::admitted_registry(&fixture.state).expect("actual service registry");
    // The production request is the sole authority for borrowing this existing service slot.
    // 生产请求为借用此既有服务槽的唯一权威。
    let owner = registry
        .get("published-service", fixture.prepared.request_id())
        .expect("actual request lookup")
        .expect("actual retained original owner");
    // Match the original coordinator's exclusion around compensation and frozen policy checks.
    // 在补偿及冻结策略检查周围匹配原协调器的排他。
    let _management = crate::skills_services::lock_luaskills_management_mutation().await;
    // Borrow the actual strong slot, retaining every real non-cloneable transaction and drain inside it.
    // 借用实际强槽，使每个真实不可克隆事务及排空保持在其中。
    let mut resources = owner.resources.lock().await;
    // Follow production's resource-to-policy order without performing any configuration write.
    // 遵循生产从资源到策略的加锁顺序，不执行任何配置写入。
    let _policy = crate::service_state_access::begin_provider_configuration_transaction().await;
    prepare_original_restoration(&mut resources, &owner)
        .await
        .expect("actual old restoration preparation");
    // The same actual manager drives the existing Pending/Aborted writers and source factory.
    // 相同实际管理器驱动既有 Pending／Aborted 写入器及来源工厂。
    let runtime = resources.runtime.clone();
    // The current original baseline supplies exact membership booleans, without policy mutation or inference.
    // 当前原基线提供精确成员布尔值，不改变策略或作推断。
    let configuration =
        provider_configuration_from_state(&fixture.state).expect("actual baseline policy");
    require_original_enabled_policy(&fixture.prepared, &configuration.luaskills)
        .expect("actual unchanged original policy");
    {
        // Split concrete fields just as production does; neither actual owner leaves the retained service slot.
        // 与生产相同分别借用具体字段；任一实际所有者都不离开保留服务槽。
        let pending_owner::PendingLocalPluginUpdateResources { stage, target, .. } =
            &mut *resources;
        // The real failed update already completed preparation before allocating its actual old drain.
        // 真实失败更新在分配实际旧排空之前已完成准备。
        let pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) = stage else {
            panic!("actual configured owner type changed");
        };
        // Only prepare_original_restoration's actual consumed drain can populate this original restoration.
        // 仅 prepare_original_restoration 实际消费的排空可填充此原恢复。
        let restoration = target
            .as_mut()
            .expect("actual enabled owner")
            .restoration
            .as_mut()
            .expect("actual old restoration");
        runtime
            .prepare_activation_predecessor_private_owner(
                &mut prepared.transaction,
                &mut restoration.restoration,
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .contains(&owner.plugin_id),
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .contains(&owner.plugin_id),
            )
            .expect("actual Pending reservation");
        restoration.private_owner_prepared = true;
        runtime
            .abort_and_stage_activation_predecessor_restoration(
                &mut prepared.transaction,
                &mut restoration.restoration,
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .contains(&owner.plugin_id),
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .contains(&owner.plugin_id),
            )
            .expect("actual Aborted and closed old catalog");
        restoration.source = Some(
            runtime
                .prepare_activation_predecessor_source(
                    &prepared.transaction,
                    &mut restoration.restoration,
                )
                .expect("actual retained predecessor source"),
        );
    }
    // Observe the original no-Hook declaration through the genuine prewarm API before MCP conversion.
    // 在 MCP 转换之前，通过真实预热 API 观察原无 Hook 声明。
    let source = Arc::clone(
        resources
            .target
            .as_ref()
            .expect("actual enabled owner")
            .restoration
            .as_ref()
            .expect("actual old restoration")
            .source
            .as_ref()
            .expect("actual retained source"),
    );
    runtime
        .prewarm_activation_predecessor_hook(&source)
        .await
        .expect("actual old Hook preparation");
    drop(source);
    assert_eq!(
        materialize_retained_predecessor_mcp(&mut resources, &credentials)
            .expect_err("actual declared old credential read must fail"),
        "Declared old credential materialization failure."
    );
    assert_eq!(
        *credentials.reads.lock().expect("actual failed bound read"),
        vec![expected_read.clone()]
    );
    // Keep only a pure profile across guard release; it cannot retain source, File or the physical preparation.
    // 跨守卫释放仅保留纯画像；它不能保留来源、文件或物理准备。
    let profile = {
        // The ordinary read error must leave the actual service's unique strong prepared slot populated.
        // 普通读取错误必须让实际服务唯一强准备槽保持已填充。
        let restoration = resources
            .target
            .as_ref()
            .expect("actual enabled owner")
            .restoration
            .as_ref()
            .expect("actual old restoration");
        assert!(restoration.native.is_none());
        restoration
            .prepared_mcp
            .as_ref()
            .expect("same physical preparation retained after read error")
            .host_profile(&resources.authority)
            .expect("actual first issuer profile")
    };
    drop(resources);
    assert_eq!(starts(&fixture.marker), original_starts);
    // Probe the already-created independent File, which the original Pending source must still physically hold.
    // 探测已经创建的独立文件；原 Pending 来源必须仍物理持有它。
    let competing = handoff_file(&fixture);
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert_eq!(
        fixture
            .store
            .predecessor_handoff_record("published-service", fixture.prepared.request_id())
            .expect("actual retained reservation read")
            .expect("actual Pending exists")
            .phase(),
        VulcanPluginPredecessorHandoffPhase::Pending
    );
    // A fresh borrow has no external physical Prepared clone; the registry's strong slot must keep factory retry possible.
    // 新借用没有外部物理 Prepared 克隆；注册表强槽必须使工厂重试仍可行。
    let mut resources = owner.resources.lock().await;
    {
        // Core retry upgrades the original weak physical link instead of building a replacement preparation.
        // 核心重试升级原弱物理链接，不构建替代准备。
        let same = resources
            .runtime
            .prepare_predecessor_mcp_resources(
                resources
                    .target
                    .as_ref()
                    .expect("actual enabled owner")
                    .restoration
                    .as_ref()
                    .expect("actual old restoration")
                    .source
                    .as_ref()
                    .expect("actual retained source"),
                &resources.authority,
            )
            .expect("same source factory survives ordinary read failure");
        assert!(
            profile.same_source(
                &same
                    .host_profile(&resources.authority)
                    .expect("same pure profile")
            )
        );
        drop(same);
    }
    credentials
        .fail_materialization
        .store(false, Ordering::Release);
    // Successful conversion now reads the exact original stored old value through that same physical prepared allocation.
    // 成功转换现通过相同物理准备分配读取精确原存储旧值。
    let inputs = materialize_retained_predecessor_mcp(&mut resources, &credentials)
        .expect("same original materialization retries after reader recovery");
    assert_eq!(
        *credentials
            .reads
            .lock()
            .expect("actual recovered bound read"),
        vec![expected_read.clone(), expected_read]
    );
    drop(inputs);
    assert_eq!(starts(&fixture.marker), original_starts);
    assert!(
        resources
            .target
            .as_ref()
            .expect("actual enabled owner")
            .restoration
            .as_ref()
            .expect("actual old restoration")
            .native
            .is_none()
    );
    // The test ends at materialization; no B startup, fake Ready, Full replacement or repeated transfer is authorized here.
    // 测试止于物化；此处不授权 B 启动、伪造 Ready、Full 替换或重复转交。
    drop(resources);
    drop(owner);
    drop(registry);
}
