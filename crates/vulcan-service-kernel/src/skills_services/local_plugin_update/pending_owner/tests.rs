//! Original-owner regressions exercise real request-file exclusion, worker failure, and service recovery dispatch.
//! 原所有者回归使用真实请求文件互斥、工作线程故障及服务恢复分派。

use super::*;
use crate::skills_services::local_plugin_update::tests::{Fixture, PLUGIN};
use vulcan_plugin_runtime::{
    VulcanPluginActivationTransactionPhase as Phase, VulcanPluginStoreErrorCode as Code,
};

/// The independent managed peer exercises actual configuration persistence outside the updated namespace.
/// 独立受管同级插件在更新命名空间之外使用实际配置持久化。
const CONFIGURATION_PEER: &str = "configuration-peer";

/// Retains an independent candidate source alongside the existing complete production service fixture.
/// 保留独立候选来源及既有完整生产服务夹具。
pub(super) struct Harness {
    /// Actual service state and installed managed predecessor come from the existing integration fixture.
    /// 实际服务状态及已安装受管前驱来自既有集成夹具。
    pub(super) fixture: Fixture,
    /// Source bytes outlive preparation and contain no external dependencies or callback effects.
    /// 来源字节比准备存活更久，且不包含外部依赖或回调副作用。
    pub(super) source: tempfile::TempDir,
}

impl Harness {
    /// Install an unrelated declared-config peer and persist its original value before any route reservation.
    /// 在任何路由预约之前安装无关且声明配置的同级插件，并持久化其原值。
    /// Uses this harness's isolated source and manager; returns unit after the actual configuration writer succeeds.
    /// 使用此夹具的隔离来源及管理器；实际配置写入器成功后返回空值。
    fn install_configuration_peer(&self) {
        // The exact directory identity is owned by this temporary source, never a host-wide catalog.
        // 精确目录身份由此临时来源拥有，绝不使用宿主全局目录。
        let package = self.source.path().join(CONFIGURATION_PEER);
        std::fs::create_dir_all(package.join("skills/review-helper"))
            .expect("unrelated configuration peer source");
        std::fs::write(
            package.join("skills/review-helper/SKILL.md"),
            "---\nname: review-helper\ndescription: Review a change.\n---\n\nConfiguration peer\n",
        )
        .expect("actual peer content component");
        std::fs::write(
            package.join("plugin.yaml"),
            format!("schema_version: 1\nid: {CONFIGURATION_PEER}\nversion: 1.0.0\ndefault_enabled: false\ndisplay:\n  name: Configuration Peer\n  capability: Configuration fixture\npermissions: [config]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\n"),
        ).expect("declared peer configuration");
        std::fs::write(
            package.join("schema.json"),
            r#"{"type":"object","properties":{"mode":{"type":"string"}},"required":["mode"],"additionalProperties":false}"#,
        ).expect("peer schema");
        std::fs::write(package.join("defaults.json"), r#"{"mode":"default"}"#)
            .expect("peer defaults");
        // The peer is selected with its complete approval before the original A request exists.
        // 在原 A 请求存在之前，以完整批准选定该其他插件。
        vulcan_plugin_runtime::install_user_package_for_test(
            self.fixture.runtime.user_plugin_root(),
            &package,
        );
        self.fixture
            .runtime
            .refresh_catalog(
                &std::collections::BTreeSet::from([
                    PLUGIN.to_owned(),
                    CONFIGURATION_PEER.to_owned(),
                ]),
                &std::collections::BTreeSet::new(),
            )
            .expect("publish unrelated peer before reservation");
        self.fixture
            .runtime
            .update_plugin_config(CONFIGURATION_PEER, json!({"mode":"before"}))
            .expect("actual initial peer configuration persistence");
    }

    /// Create a content-only version-three candidate while preserving the actual version-one predecessor.
    /// 创建纯内容第三版候选，同时保留实际第一版前驱。
    /// No inputs; returns isolated paths and a full service publisher suitable for real request ownership checks.
    /// 无输入；返回隔离路径及完整服务发布器，用于真实请求所有权检查。
    pub(super) fn new() -> Self {
        // Existing fixture creates and installs the predecessor through the real package authorization path.
        // 既有夹具经真实插件包授权路径创建并安装前驱。
        let fixture = Fixture::new(false);
        // Source owns a temporary parent; the package directory must exactly match its declared plugin identity.
        // 来源拥有临时父目录；插件包目录必须精确匹配声明的插件身份。
        let source = tempfile::tempdir().expect("candidate source");
        // Keep the real package validator's directory identity contract in the fixture.
        // 在夹具中遵守真实插件包校验器的目录身份契约。
        let package_root = source.path().join(PLUGIN);
        std::fs::create_dir_all(package_root.join("skills/review-helper"))
            .expect("skill directory");
        std::fs::write(package_root.join("plugin.yaml"), format!("schema_version: 1\nid: {PLUGIN}\nversion: 3.0.0\ndefault_enabled: false\ndisplay:\n  name: Owner Fixture\n  capability: Read-only guidance\n")).expect("candidate manifest");
        std::fs::write(
            package_root.join("skills/review-helper/SKILL.md"),
            "---\nname: review-helper\ndescription: Review a change.\n---\n\nOwner fixture\n",
        )
        .expect("candidate skill");
        Self { fixture, source }
    }

    /// Declare a real contained native launcher whose actual operating-system startup must fail.
    /// 声明真实受目录约束原生启动器，其实际操作系统启动必然失败。
    /// No inputs; returns a complete production package with no Lua lifecycle callbacks.
    /// 无输入；返回完整生产插件包，不含 Lua 生命周期回调。
    pub(super) fn with_failed_native_candidate() -> Self {
        // Real invalid bytes exercise the native process boundary without starting an external service.
        // 真实无效字节使用原生进程边界，不启动外部服务。
        let harness = Self::new();
        std::fs::write(
            harness.source.path().join(PLUGIN).join("server.bin"),
            b"invalid executable fixture",
        )
        .expect("inert executable");
        // Every supported launcher target uses the same contained test executable.
        // 每个受支持启动器目标使用相同受目录约束测试可执行文件。
        let mut manifest =
            std::fs::read_to_string(harness.source.path().join(PLUGIN).join("plugin.yaml"))
                .expect("source manifest");
        manifest.push_str("permissions: [process_binary, process_session]\nexecution:\n  native_binaries:\n    - id: server\n      targets:\n        x86_64-pc-windows-msvc: server.bin\n        aarch64-pc-windows-msvc: server.bin\n        x86_64-unknown-linux-gnu: server.bin\n        aarch64-unknown-linux-gnu: server.bin\n        x86_64-apple-darwin: server.bin\n        aarch64-apple-darwin: server.bin\nmcp:\n  servers:\n    - id: local\n      name: Local ownership fixture\n      transport:\n        type: stdio\n        launcher: {type: native_binary, binary_id: server}\n");
        std::fs::write(
            harness.source.path().join(PLUGIN).join("plugin.yaml"),
            manifest,
        )
        .expect("actual native declaration");
        harness
    }

    /// Prepare and immediately retain the original UUID through the same production store and service registry.
    /// 经相同生产存储及服务注册表准备并立即保留原 UUID。
    /// Returns the actual strong slot and registry; the independent observer must see its real request-file lock.
    /// 返回实际强槽及注册表；独立观察者必须看到其真实请求文件锁。
    fn retain(
        &self,
    ) -> (
        Arc<PendingLocalPluginUpdateRegistry>,
        Arc<PendingLocalPluginUpdate>,
    ) {
        // Exact preview supplies fingerprint and complete approved permissions.
        // 精确预览提供指纹及完整已批准权限。
        let preview = self
            .fixture
            .runtime
            .inspect_local_user_plugin(&self.source.path().join(PLUGIN))
            .expect("candidate preview");
        // The UUID is allocated once before the actual preparation starts.
        // UUID 在实际准备开始前仅分配一次。
        let request_id = uuid::Uuid::new_v4();
        // Prepared is the actual non-cloneable transaction, not a synthetic status row.
        // prepared 为实际不可克隆事务，而非合成状态行。
        let prepared = prepare_update(
            &self.fixture.runtime,
            &provider_configuration_from_state(&self.fixture.state)
                .expect("configuration")
                .luaskills,
            &self.source.path().join(PLUGIN),
            &preview.content_fingerprint,
            preview.permissions,
            false,
            Vec::new(),
            request_id,
        )
        .expect("original preparation");
        // Registry is the exact field retained by ServiceBridgeState.
        // 注册表为 ServiceBridgeState 保留的精确字段。
        let registry = admitted_registry(&self.fixture.state).expect("admitted registry");
        let owner = registry
            .retain(self.fixture.runtime.clone(), prepared)
            .expect("retain original");
        (registry, owner)
    }

    /// Assert real independent OS exclusion for request_id, without borrowing its slot or relying on a counter.
    /// 对 request_id 断言真实独立操作系统互斥，不借用其槽，也不依赖计数器。
    /// No return value; the durable original remains visible while a second physical owner is rejected as Busy.
    /// 无返回值；第二个物理所有者以 Busy 被拒绝时，持久原请求仍可见。
    fn assert_busy(&self, request_id: uuid::Uuid) {
        // Independently opened store proves the actual file lease rather than a local mutex state.
        // 独立打开的存储证明实际文件租约，而非本地互斥锁状态。
        let observer =
            VulcanPluginGenerationStore::open_existing(self.fixture.runtime.user_plugin_root())
                .expect("observer")
                .expect("existing store");
        // The complete durable checkpoint selects the declared unfinished or terminal ownership probe.
        // 完整持久检查点选择明确的未完成或终态所有权探测入口。
        let original = observer
            .activation_transaction(PLUGIN, request_id)
            .expect("original evidence")
            .expect("actual original record");
        // Public resume rejects terminal phases before taking the original request file.
        // 公开恢复在取得原请求文件之前拒绝终态阶段。
        let resume_error = observer
            .resume_activation_transaction(PLUGIN, request_id)
            .expect_err("another physical owner cannot resume the retained original");
        if !matches!(original.phase(), Phase::Completed | Phase::Aborted) {
            assert_eq!(
                resume_error.code,
                Code::Busy,
                "unfinished original request must retain its actual operating-system lock"
            );
            return;
        }
        assert_eq!(resume_error.code, Code::Invalid);
        assert_eq!(
            resume_error.message,
            "Terminal activation requests cannot be resumed."
        );
        // These paths follow the actual store layout and journal writer, without opening or inventing a lease file.
        // 这些路径遵循实际存储布局及日志写入器，不打开或虚构租约文件。
        let journal_directory = self
            .fixture
            .runtime
            .user_plugin_root()
            .join(".vulcan-generations")
            .join("transactions")
            .join(PLUGIN);
        // Preserve exact original index and journal bytes across the competing real writer rejection.
        // 在真实竞争写入者被拒期间保留精确原索引及日志字节。
        let index_before =
            std::fs::read(journal_directory.join("current")).expect("actual terminal index bytes");
        let journal_before = std::fs::read(journal_directory.join(format!("{request_id}.json")))
            .expect("actual terminal journal bytes");
        // The exact current selection may be the completed target or the aborted predecessor.
        // 精确当前选择可能是已完成目标或已中止前驱。
        let selected = observer.activation(PLUGIN).expect("actual full selection");
        // Enumeration proves that rejection does not allocate a successor or replace an original index.
        // 枚举证明拒绝不会分配后继或替换原索引。
        let inventory_before = observer
            .activation_transaction_ids(PLUGIN)
            .expect("actual original inventory");
        // Inspect the real candidate again so fingerprint and approved permissions are current facts.
        // 再次检查真实候选，使指纹及批准权限来自当前事实。
        let preview = self
            .fixture
            .runtime
            .inspect_local_user_plugin(&self.source.path().join(PLUGIN))
            .expect("actual successor source preview");
        assert_eq!(preview.plugin_id, PLUGIN);
        // A new UUID tests cross-store successor exclusion, never a substitute owner of the original request.
        // 新 UUID 验证跨存储后继互斥，绝不是原请求的替换所有者。
        let successor_request = uuid::Uuid::new_v4();
        assert_ne!(successor_request, request_id);
        // This real API probes the terminal original file under the namespace fence before any durable allocation.
        // 此真实接口在任何持久分配之前，于命名空间屏障内探测终态原文件。
        let successor_error = observer
            .begin_activation_update_preparation(
                successor_request,
                &self.source.path().join(PLUGIN),
                &preview.content_fingerprint,
                selected.as_ref(),
                preview.permissions,
                *original.update_policy().expect("original update policy"),
            )
            .expect_err("terminal original live file must exclude another store's new UUID");
        assert_eq!(successor_error.code, Code::Busy);
        assert_eq!(
            successor_error.message,
            "Terminal activation still retains its original live owner."
        );
        assert_eq!(
            std::fs::read(journal_directory.join("current")).expect("unchanged exact index"),
            index_before
        );
        assert_eq!(
            std::fs::read(journal_directory.join(format!("{request_id}.json")))
                .expect("unchanged exact journal"),
            journal_before
        );
        assert_eq!(
            observer
                .activation_transaction(PLUGIN, request_id)
                .expect("unchanged complete original")
                .expect("retained original"),
            original
        );
        assert_eq!(
            observer.activation(PLUGIN).expect("unchanged selection"),
            selected
        );
        assert_eq!(
            observer
                .activation_transaction_ids(PLUGIN)
                .expect("unchanged inventory"),
            inventory_before
        );
        assert!(
            observer
                .activation_request(PLUGIN, successor_request)
                .expect("successor was not allocated")
                .is_none()
        );
    }
}

/// A retained original routing reservation excludes ordinary persistence and catalog replacement until exact retirement.
/// 保留原路由预约在精确退役之前排除普通持久化及目录替换。
/// No inputs or return value; uses the real registry, service Full writer, management refresh and terminal retirement.
/// 无输入或返回值；使用真实注册表、服务 Full 写入者、管理刷新及终态退役。
#[tokio::test]
async fn pending_update_full_publication_excludes_settings_until_retirement() {
    // The concrete preparation and actual File owner come from the existing isolated service fixture.
    // 具体准备及实际文件所有者来自既有隔离服务夹具。
    let harness = Harness::new();
    harness.install_configuration_peer();
    // Keep both original registry and owner allocations, rather than inventing a coordination-only record.
    // 保留原注册表及所有者分配，不创建仅用于协调的记录。
    let (registry, owner) = harness.retain();
    // Holding actual resources proves ordinary admission never tries to acquire this asynchronous mutex.
    // 持有实际资源证明普通准入绝不尝试获取此异步互斥锁。
    let resources = owner.resources.lock().await;
    owner
        .retain_routing_publication(&harness.fixture.state, &resources)
        .expect("retain actual original routing and publication exclusion");
    // An unpublished Full failure cannot remove the earlier closed-route reservation or its service admission pin.
    // 未发布 Full 失败不得移除更早的关闭路由预约及其服务准入固定状态。
    owner
        .release_unpublished_full_publication()
        .expect("no Full witness keeps actual routing reservation protected");
    registry
        .require_full_publication(Some(owner.request_id))
        .expect("same original coordinator remains admitted");
    assert!(registry.require_full_publication(None).is_err());
    assert!(
        registry
            .require_full_publication(Some(uuid::Uuid::new_v4()))
            .is_err()
    );
    // Capture the actual selected Full and core objects before attempting ordinary changes.
    // 尝试普通变更之前，捕获实际已选 Full 及核心对象。
    let original_full = {
        // The service mutex is released before any asynchronous management call.
        // 任一异步管理调用之前释放服务互斥锁。
        let service = harness.fixture.state.lock().expect("actual service");
        Arc::clone(
            &service
                .agent_runtime
                .as_ref()
                .expect("actual full publisher")
                .extension_generation,
        )
    };
    // Core identity must remain unchanged when the ordinary refresh is rejected before manager publication.
    // 普通刷新在管理器发布之前被拒绝时，核心身份必须保持不变。
    let original_catalog = harness.fixture.runtime.snapshot();
    // The supplied configuration is the actual current one; no guessed or fallback fields are introduced.
    // 所提交配置为实际当前配置；不引入猜测或回退字段。
    let configuration = provider_configuration_from_state(&harness.fixture.state)
        .expect("actual configuration before rejected persistence");
    // Direct Core callers share the same exclusion without reading the service registry or awaiting its resource slot.
    // 直接核心调用者共享相同排他，无须读取服务注册表或等待其资源槽。
    let core_refresh = harness
        .fixture
        .runtime
        .refresh_catalog(
            &configuration.luaskills.disabled_vulcan_plugins,
            &configuration.luaskills.enabled_vulcan_plugins,
        )
        .expect_err("actual Core reservation rejects ordinary routing replacement");
    assert!(
        core_refresh
            .to_string()
            .contains(&owner.request_id.to_string())
    );
    // A genuine other-namespace configuration writer must fail before replacing its existing persisted configuration.
    // 真实其他命名空间配置写入器必须在替换其既有持久配置之前失败。
    let config_error = harness
        .fixture
        .runtime
        .update_plugin_config(CONFIGURATION_PEER, json!({"mode":"blocked"}))
        .expect_err("Core reservation rejects unrelated configuration persistence");
    assert!(
        config_error
            .to_string()
            .contains(&owner.request_id.to_string())
    );
    assert_eq!(
        harness
            .fixture
            .runtime
            .plugin_config(CONFIGURATION_PEER)
            .expect("actual peer readback"),
        json!({"mode":"before"}),
    );
    // An isolated actual file would be written only if the production persistence callback was incorrectly reached.
    // 仅当生产持久化回调被错误触及时，才会写入隔离实际文件。
    let attempted_persistence = harness.source.path().join("rejected-settings-write.json");
    // Invoke the same synchronous Full writer used by MCP save/remove while the original resource mutex remains held.
    // 原资源互斥锁仍被持有时，调用 MCP 保存／删除共用的相同同步 Full 写入者。
    let error = crate::settings_runtime_projection::store_provider_and_mcp_configuration_after_change_with_persistence(
        &harness.fixture.state,
        configuration.clone(),
        original_full.mcp_runtime(),
        |_| std::fs::write(&attempted_persistence, b"unexpected persistence")
            .map_err(|error| error.to_string()),
    )
    .expect_err("retained original must reject settings before persistence");
    assert!(error.contains(&owner.request_id.to_string()));
    assert!(!attempted_persistence.exists());
    // A bounded observer verifies the real owned management task rejects without blocking on resources.
    // 有界观察者验证真实自有管理任务拒绝操作，而不阻塞等待资源。
    let refresh = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        crate::skills_services::service_refresh_luaskills_catalog(
            ServiceEventEmitter::new(Vec::new()),
            Arc::clone(&harness.fixture.state),
        ),
    )
    .await
    .expect("ordinary catalog admission must not await retained resources")
    .expect_err("ordinary catalog refresh must preserve the retained original");
    assert!(refresh.contains(&owner.request_id.to_string()));
    assert!(Arc::ptr_eq(
        &original_catalog,
        &harness.fixture.runtime.snapshot()
    ));
    assert!(Arc::ptr_eq(
        &original_full,
        &harness
            .fixture
            .state
            .lock()
            .expect("actual unchanged service")
            .agent_runtime
            .as_ref()
            .expect("actual unchanged publisher")
            .extension_generation
    ));
    drop(resources);
    // This unstarted real original can retire through its actual abort protocol; only registry removal ends exclusion.
    // 此未开始真实原请求可经实际中止协议退役；仅注册表移除结束排他。
    abort_owned(
        &harness.fixture.state,
        PLUGIN,
        &owner.request_id.to_string(),
        true,
    )
    .await
    .expect("actual original terminal retirement");
    registry
        .require_full_publication(None)
        .expect("ordinary publication reopens after actual registry retirement");
    assert!(registry.get(PLUGIN, owner.request_id).unwrap().is_none());
    // The executor still owns this service Arc; actual map retirement must already have released the Core reservation.
    // 执行器仍拥有此服务 Arc；实际映射退役必须已经释放核心预约。
    harness
        .fixture
        .runtime
        .refresh_catalog(
            &configuration.luaskills.disabled_vulcan_plugins,
            &configuration.luaskills.enabled_vulcan_plugins,
        )
        .expect("ordinary Core publication resumes before the retired owner Arc drops");
    // Retirement restores the same real configuration writer; no replacement request or issuer is constructed.
    // 退役恢复相同真实配置写入器；不构造替代请求或签发者。
    harness
        .fixture
        .runtime
        .update_plugin_config(CONFIGURATION_PEER, json!({"mode":"after"}))
        .expect("peer configuration write resumes after actual original retirement");
    assert_eq!(
        harness
            .fixture
            .runtime
            .plugin_config(CONFIGURATION_PEER)
            .expect("committed peer readback"),
        json!({"mode":"after"}),
    );
}

/// Cancelled foreground RPC observation cannot release the same original update during or after detached execution.
/// 已取消前台 RPC 观察不能在分离执行期间或之后释放相同原更新。
/// No inputs or return value; verifies the real service task, independent Busy lease, live query, and same UUID abort.
/// 无输入或返回值；验证真实服务任务、独立 Busy 租约、实时查询及相同 UUID 中止。
#[tokio::test]
async fn pending_update_owner_survives_cancelled_rpc_waiter() {
    let harness = Harness::new();
    let (registry, owner) = harness.retain();
    let request_id = owner.request_id;
    // Explicit barriers confirm the actual service-owned child began before its RPC observer disappears.
    // 明确屏障确认实际服务自有子任务在其 RPC 观察者消失前已经开始。
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let finished = Arc::new(tokio::sync::Notify::new());
    let child_owner = Arc::clone(&owner);
    let child_entered = Arc::clone(&entered);
    let child_release = Arc::clone(&release);
    let child_finished = Arc::clone(&finished);
    let waiter = tokio::spawn(run_luaskills_management_owned(
        "retained owner cancellation regression",
        move || async move {
            let resources = child_owner.resources.lock().await;
            assert_eq!(
                resources
                    .prepared()
                    .unwrap()
                    .transaction
                    .record()
                    .request_id(),
                request_id
            );
            child_entered.notify_one();
            child_release.notified().await;
            drop(resources);
            child_finished.notify_one();
            Ok(())
        },
    ));
    entered.notified().await;
    waiter.abort();
    assert!(
        waiter
            .await
            .expect_err("cancelled RPC observer")
            .is_cancelled()
    );
    harness.assert_busy(request_id);
    // Status must finish while the original child still holds its async resource borrow.
    // 原子任务仍持有异步资源借用期间，状态必须完成。
    let rows = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        local_plugin_update_records(Arc::clone(&harness.fixture.state)),
    )
    .await
    .expect("live status must not wait for owner")
    .expect("status");
    assert!(
        rows.iter()
            .find(|row| row.request_id == request_id.to_string())
            .expect("original row")
            .has_service_owner
    );
    release.notify_one();
    finished.notified().await;
    harness.assert_busy(request_id);
    let abandoned = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
        false,
    )
    .await
    .expect("dispatch same original owner");
    assert_eq!(abandoned.phase, Some(Phase::Aborted));
    assert!(
        registry
            .get(PLUGIN, request_id)
            .expect("terminal registry")
            .is_none()
    );
}

/// A true spawn_blocking panic drops only its borrowed guard and keeps the original physical request in service state.
/// 真实 spawn_blocking panic 仅销毁其借用守卫，并在服务状态中保留原物理请求。
/// No inputs or return value; service abort must reuse that original slot rather than reacquire its Busy request.
/// 无输入或返回值；服务中止必须复用原槽，而非重新取得其 Busy 请求。
#[tokio::test]
async fn pending_update_owner_survives_blocking_worker_panic() {
    let harness = Harness::new();
    let (registry, owner) = harness.retain();
    let request_id = owner.request_id;
    let worker_owner = Arc::clone(&owner);
    let failure = run_extension_management_blocking(
        "actual retained owner panic regression",
        move || -> Result<(), String> {
            let resources = worker_owner.resources.blocking_lock();
            assert_eq!(
                resources
                    .prepared()
                    .unwrap()
                    .transaction
                    .record()
                    .request_id(),
                request_id
            );
            panic!("controlled actual blocking worker failure");
        },
    )
    .await
    .expect_err("real JoinError");
    assert!(
        failure.contains("panic") || failure.contains("panicked"),
        "{failure}"
    );
    assert!(Arc::ptr_eq(
        &registry
            .get(PLUGIN, request_id)
            .expect("lookup")
            .expect("retained slot"),
        &owner
    ));
    harness.assert_busy(request_id);
    let abandoned = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
        false,
    )
    .await
    .expect("original owner abort");
    assert_eq!(abandoned.phase, Some(Phase::Aborted));
    assert_eq!(abandoned.request_id, request_id.to_string());
}

/// An actual service executor panic preserves original resources after its management lock is released.
/// 实际服务执行器 panic 在管理锁释放后保留原资源。
/// No inputs or return value; independent file exclusion and later same-owner dispatch prove physical retention.
/// 无输入或返回值；独立文件互斥及后续同所有者分派证明物理保留。
#[tokio::test]
async fn pending_update_owner_survives_service_executor_panic() {
    let harness = Harness::new();
    let (registry, owner) = harness.retain();
    let request_id = owner.request_id;
    let child_owner = Arc::clone(&owner);
    run_luaskills_management_owned(
        "actual retained service executor panic",
        move || async move {
            let resources = child_owner.resources.lock().await;
            assert_eq!(
                resources
                    .prepared()
                    .unwrap()
                    .transaction
                    .record()
                    .request_id(),
                request_id
            );
            panic!("controlled actual service executor failure");
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        },
    )
    .await
    .expect_err("actual service JoinError");
    harness.assert_busy(request_id);
    assert!(Arc::ptr_eq(
        &registry
            .get(PLUGIN, request_id)
            .expect("lookup")
            .expect("retained slot"),
        &owner
    ));
    let abandoned = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
        false,
    )
    .await
    .expect("abort retained executor owner");
    assert_eq!(abandoned.phase, Some(Phase::Aborted));
}

/// A retained unpublished original cannot be replayed by resume, and closed shutdown admission cannot start recovery.
/// 保留尚未发布原请求不能由恢复重放，且已关闭准入不能启动恢复。
/// No inputs or return value; verifies original phase, real request exclusion, and unchanged published pointer.
/// 无输入或返回值；验证原阶段、真实请求互斥及未改变已发布指针。
#[tokio::test]
async fn pending_update_owner_rejects_replay_and_shutdown_publication() {
    let harness = Harness::new();
    let (registry, owner) = harness.retain();
    let request_id = owner.request_id;
    let original = harness.fixture.runtime.snapshot();
    service_resume_local_plugin_update(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
    )
    .await
    .expect_err("unselected request must not replay");
    harness.assert_busy(request_id);
    assert!(Arc::ptr_eq(&original, &harness.fixture.runtime.snapshot()));
    harness
        .fixture
        .state
        .lock()
        .expect("state")
        .service_lifecycle_state
        .begin_shutdown_drain();
    service_resume_local_plugin_update(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
    )
    .await
    .expect_err("closed shutdown admission");
    registry
        .drain_for_shutdown()
        .await
        .expect("actual resource drain");
    harness.assert_busy(request_id);
    assert!(Arc::ptr_eq(&original, &harness.fixture.runtime.snapshot()));
    assert!(registry.get("another-namespace", request_id).is_err());
}

/// A real Completed writer followed by executor panic resumes the same original owner without replay or republishing.
/// 真实 Completed 写入器之后执行器 panic，恢复相同原所有者，不重放也不重新发布。
/// No inputs or return value; actual publisher, durable writer and independent OS lock precede lost observation.
/// 无输入或返回值；实际发布器、持久写入器及独立操作系统锁均先于丢失观察。
#[tokio::test]
async fn pending_update_owner_resumes_actual_completed_write_after_executor_loss() {
    let harness = Harness::new();
    let (registry, owner) = harness.retain();
    let request_id = owner.request_id;
    // The real content-only validation path creates no external callback or installation receipt.
    // 真实纯内容校验路径不创建外部回调或安装回执。
    validate_and_install_retained(&owner)
        .await
        .expect("actual original validation");
    // This executor owns the same non-cloneable transaction throughout real route and journal publication.
    // 此执行器在真实路由及日志发布期间始终拥有相同不可克隆事务。
    let child_owner = Arc::clone(&owner);
    let child_state = Arc::clone(&harness.fixture.state);
    run_luaskills_management_owned("lose actual Completed observer", move || async move {
        let mut resources = child_owner.resources.lock().await;
        let runtime = resources.runtime.clone();
        let prepared = resources.prepared_mut().unwrap();
        prepared
            .store
            .commit_activation_transaction(&mut prepared.transaction)
            .expect("actual selection writer");
        let configuration =
            provider_configuration_from_state(&child_state).expect("latest retained policy");
        let catalog = runtime
            .refresh_catalog_for_activation_recovery(
                &prepared.transaction,
                &configuration.luaskills.disabled_vulcan_plugins,
                &configuration.luaskills.enabled_vulcan_plugins,
            )
            .expect("original recovery catalog");
        validate_published_catalog(&runtime, &catalog, prepared.transaction.record().target())
            .expect("actual disabled projection");
        synchronize_service_vulcan_plugin_hooks(&child_state, runtime, &catalog)
            .expect("matching actual hooks");
        vulcan_plugin_lifecycle::publish_disabled_plugin_extension_generation(
            &child_state,
            PLUGIN,
            catalog,
        )
        .expect("actual sole service publisher");
        prepared
            .store
            .complete_activation_transaction(&mut prepared.transaction)
            .expect("actual Completed writer");
        panic!("controlled observer loss after actual Completed write");
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    })
    .await
    .expect_err("actual service observer loss");
    harness.assert_busy(request_id);
    // Snapshot the actual completed route before recovery; a second candidate publication would change this pointer.
    // 恢复前保存实际已完成路由；第二次候选发布会改变此指针。
    let completed_catalog = harness.fixture.runtime.snapshot();
    let store =
        VulcanPluginGenerationStore::open_existing(harness.fixture.runtime.user_plugin_root())
            .expect("observer")
            .expect("existing store");
    assert_eq!(
        store
            .activation_request(PLUGIN, request_id)
            .expect("canonical Completed readback")
            .expect("original durable record")
            .phase(),
        Phase::Completed
    );
    let effects = store
        .activation_effects(PLUGIN, request_id)
        .expect("original effects");
    let installation = store
        .installation_receipt(PLUGIN, request_id)
        .expect("original installation");
    assert!(
        effects.is_none() && installation.is_none(),
        "content-only validation must not invent callbacks"
    );
    // A fresh user update must not replace this terminal original index before its lost observer is reconciled.
    // 丢失观察者核对之前，新用户更新不得替换此终态原索引。
    let manifest = std::fs::read_to_string(harness.source.path().join(PLUGIN).join("plugin.yaml"))
        .expect("mutable source manifest");
    std::fs::write(
        harness.source.path().join(PLUGIN).join("plugin.yaml"),
        manifest.replace("version: 3.0.0", "version: 4.0.0"),
    )
    .expect("distinct new user candidate");
    let preview = harness
        .fixture
        .runtime
        .inspect_local_user_plugin(&harness.source.path().join(PLUGIN))
        .expect("fresh explicit approval");
    let rejected = update(
        &ServiceEventEmitter::new(Vec::new()),
        &harness.fixture.state,
        harness
            .source
            .path()
            .join(PLUGIN)
            .to_string_lossy()
            .into_owned(),
        preview.content_fingerprint,
        preview.permissions,
        false,
        Vec::new(),
    )
    .await
    .expect_err("retain original terminal index before a second update");
    assert!(rejected.contains(&request_id.to_string()), "{rejected}");
    assert_eq!(
        store
            .activation_transaction_ids(PLUGIN)
            .expect("unchanged original inventory"),
        BTreeSet::from([request_id])
    );
    service_resume_local_plugin_update(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
    )
    .await
    .expect("same retained Completed owner recovery");
    assert!(
        registry
            .get(PLUGIN, request_id)
            .expect("registry")
            .is_none()
    );
    assert!(
        Arc::ptr_eq(&completed_catalog, &harness.fixture.runtime.snapshot()),
        "completed owner recovery must not republish candidate routes"
    );
    assert!(
        store
            .activation_effects(PLUGIN, request_id)
            .expect("no callback replay")
            .is_none()
    );
    assert!(
        store
            .installation_receipt(PLUGIN, request_id)
            .expect("no installation replay")
            .is_none()
    );
}

/// A real native readiness attempt remains acknowledged independently from absent Lua callback receipts.
/// 真实原生就绪尝试在 Lua 回调回执缺失时仍需要独立确认。
/// No inputs or return value; actual invalid executable startup and retained original control fence same-UUID abort.
/// 无输入或返回值；实际无效可执行文件启动及保留原控制对象对相同 UUID 中止设立屏障。
#[tokio::test]
async fn pending_update_owner_retains_native_attempt_until_acknowledged_abort() {
    // The existing service fixture remains isolated from external providers and registries.
    // 既有服务夹具保持与外部供应商及注册表隔离。
    let harness = Harness::with_failed_native_candidate();
    // The actual request and sole slot retain startup control across the normal validation failure.
    // 实际请求及唯一槽在正常校验失败期间保留启动控制。
    let (registry, owner) = harness.retain();
    // Original identity comes only from the actual retained transaction.
    // 原身份仅来自实际保留事务。
    let request_id = owner.request_id;
    // Production validation reaches the actual invalid native executable and returns its failed readiness.
    // 生产校验到达实际无效原生可执行文件，并返回失败就绪。
    let failed = validate_and_install_retained(&owner)
        .await
        .expect_err("real native readiness failure");
    assert!(
        failed.starts_with("Candidate MCP validation remains incomplete:"),
        "{failed}"
    );
    {
        // Borrowed original control remains in the slot after validation has returned.
        // 校验返回后，借用的原控制对象仍在槽中。
        let resources = owner.resources.lock().await;
        // The real observation carries exact startup identity and actual native lifecycle diagnostics.
        // 真实观察携带精确启动身份及实际原生生命周期诊断。
        let observation = resources
            .candidate
            .as_ref()
            .expect("original real control retained")
            .observer();
        assert_eq!(observation.request_id(), request_id);
        assert!(!observation.server_states().is_empty());
        assert!(
            resources
                .prepared()
                .unwrap()
                .store
                .activation_effects(PLUGIN, request_id)
                .expect("content-only Lua effect inventory")
                .is_none()
        );
        assert!(
            resources
                .prepared()
                .unwrap()
                .store
                .installation_receipt(PLUGIN, request_id)
                .expect("installation not entered")
                .is_none()
        );
    }
    harness.assert_busy(request_id);
    // Acknowledgement cannot be bypassed simply because this package has no Lua lifecycle entry.
    // 不能仅因插件包没有 Lua 生命周期入口而绕过确认。
    let denied = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
        false,
    )
    .await
    .expect_err("native attempt requires explicit acknowledgement");
    assert!(denied.contains("MCP 启动尝试"), "{denied}");
    {
        // The rejected abort retains both the original journal observation and the same real control object.
        // 被拒中止保留原日志观察及相同真实控制对象。
        let resources = owner.resources.lock().await;
        assert_eq!(
            resources.prepared().unwrap().transaction.record().phase(),
            Phase::Prepared
        );
        assert_eq!(
            resources
                .candidate
                .as_ref()
                .expect("same original real controller remains")
                .observer()
                .request_id(),
            request_id
        );
    }
    // Explicit acknowledgement authorizes selection abandonment after actual transport release.
    // 明确确认在实际传输释放后授权选择放弃。
    let abandoned = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
        true,
    )
    .await
    .expect("acknowledged same original native attempt");
    assert_eq!(abandoned.phase, Some(Phase::Aborted));
    assert!(
        registry
            .get(PLUGIN, request_id)
            .expect("terminal registry")
            .is_none()
    );
}

/// The exact real Aborted writer survives observer loss and cleans up without asking again for an executed decision.
/// 精确真实 Aborted 写入器在观察者丢失后继续保留，并在清理时不再次询问已执行决定。
/// No inputs or return value; actual native startup, transport release, durable abort and executor panic precede recovery.
/// 无输入或返回值；实际原生启动、传输释放、持久中止及执行器 panic 均先于恢复。
#[tokio::test]
async fn pending_update_owner_resumes_actual_aborted_write_after_executor_loss() {
    // Real native startup is attempted without any Lua effect receipt, so its original controller carries necessary evidence.
    // 在没有 Lua 副作用回执时尝试真实原生启动，因此原控制对象携带必要证据。
    let harness = Harness::with_failed_native_candidate();
    // Actual request and physical lock remain held by the exact service registry slot.
    // 实际请求及物理锁继续由精确服务注册表槽持有。
    let (registry, owner) = harness.retain();
    // Original durable identity remains unchanged across the writer and its lost observer.
    // 原持久身份在写入器及其丢失观察者之间保持不变。
    let request_id = owner.request_id;
    validate_and_install_retained(&owner)
        .await
        .expect_err("actual invalid native startup");
    // This trusted host executor represents the already acknowledged abort decision and retains the same real control.
    // 此受信任宿主执行器代表已经确认的中止决定，并保留相同真实控制对象。
    let child_owner = Arc::clone(&owner);
    run_luaskills_management_owned(
        "lose acknowledged actual Aborted observer",
        move || async move {
            // Borrow the actual original owner, without reconstructing a transaction from copied UUIDs.
            // 借用实际原所有者，不从复制 UUID 重建事务。
            let mut resources = child_owner.resources.lock().await;
            // The actual original transport must release before the real journal writer can acknowledge abort.
            // 真实日志写入器能够确认中止之前，实际原传输必须释放。
            let candidate = resources
                .candidate
                .as_ref()
                .expect("retained actual original native control");
            assert!(!candidate.observer().server_states().is_empty());
            candidate.shutdown();
            candidate
                .observer()
                .wait_until_released(AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT)
                .await
                .expect("actual original native resource release");
            // Persist the real authorized terminal decision before the service observer fails.
            // 服务观察者失败之前，持久保存真实已授权终态决定。
            let prepared = resources.prepared_mut().unwrap();
            prepared
                .store
                .abort_activation_transaction(&mut prepared.transaction)
                .expect("actual acknowledged Aborted writer");
            panic!("controlled observer loss after actual Aborted write");
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        },
    )
    .await
    .expect_err("actual observer JoinError");
    harness.assert_busy(request_id);
    {
        // Original controller and canonical terminal decision still coexist after the executor disappeared.
        // 执行器消失后，原控制对象及规范终态决定仍同时存在。
        let resources = owner.resources.lock().await;
        assert_eq!(
            resources.prepared().unwrap().transaction.record().phase(),
            Phase::Aborted
        );
        assert_eq!(
            resources
                .candidate
                .as_ref()
                .expect("same actual native controller")
                .observer()
                .request_id(),
            request_id
        );
        assert_eq!(
            resources
                .prepared()
                .unwrap()
                .store
                .activation_request(PLUGIN, request_id)
                .expect("canonical readback")
                .expect("original record")
                .phase(),
            Phase::Aborted
        );
    }
    // Exact Aborted readback certifies the previous decision; this call only finishes original resource retirement.
    // 精确 Aborted 回读证明先前决定；本次调用仅终结原资源退役。
    let recovered = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.to_owned(),
        request_id.to_string(),
        false,
    )
    .await
    .expect("confirmed original abort needs no repeated acknowledgement");
    assert_eq!(recovered.phase, Some(Phase::Aborted));
    assert!(
        registry
            .get(PLUGIN, request_id)
            .expect("retired original slot")
            .is_none()
    );
}
