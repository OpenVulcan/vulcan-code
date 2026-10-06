//! Early-owner regressions use actual journal, package, revision files and a real isolated file credential backend.
//! 早期所有者回归使用实际日志、插件包、修订文件及真实隔离文件凭据后端。

use super::tests::Harness;
use super::*;
use crate::skills_services::local_plugin_update::tests::PLUGIN;
use preparation::observation::{self, Boundary};
use std::cell::Cell;
use vulcan_plugin_runtime::{
    VulcanPluginActivationRequestRecord as Record, VulcanPluginActivationTransactionPhase as Phase,
    VulcanPluginCredentialBackend, VulcanPluginCredentialPhase, VulcanPluginCredentialRevision,
    VulcanPluginStoreErrorCode as Code,
};

/// Register a validated original reservation before any actual journal or inventory exists.
/// 在任何实际日志或清单存在之前注册已校验原预留。
/// Harness supplies approved source; configured selects an explicit declared secret with no old-secret read.
/// harness 提供批准来源；configured 选择明确声明秘密，不读取旧秘密。
fn reserve(
    harness: &Harness,
    configured: bool,
) -> (
    Arc<PendingLocalPluginUpdateRegistry>,
    Arc<PendingLocalPluginUpdate>,
) {
    let source = harness.source.path().join(PLUGIN);
    let preview = harness
        .fixture
        .runtime
        .inspect_local_user_plugin(&source)
        .expect("source preview");
    let inputs = if configured {
        vec![VulcanPluginInitialSecretInput {
            server_id: "local".into(),
            secret_key: "first".into(),
            value: "original-value".into(),
        }]
    } else {
        Vec::new()
    };
    let reserved = reserve_update(
        &harness.fixture.runtime,
        &provider_configuration_from_state(&harness.fixture.state)
            .expect("original configuration")
            .luaskills,
        &source,
        &preview.content_fingerprint,
        preview.permissions,
        false,
        inputs,
        uuid::Uuid::new_v4(),
    )
    .expect("actual original reservation");
    let registry = admitted_registry(&harness.fixture.state).expect("registry");
    // Borrow the genuine bridge pair; the fixture must not register a new host for each request.
    // 借用真实桥配对；夹具不得为每个请求注册新宿主。
    let (runtime, authority) = service_vulcan_plugin_native_mcp_host(&harness.fixture.state)
        .expect("original service host pair");
    let owner = registry
        .retain_reserved(runtime, authority, reserved)
        .expect("register before journal");
    (registry, owner)
}

/// Assert actual independent original request exclusion at Preparing or Prepared checkpoints.
/// 在 Preparing 或 Prepared 检查点断言实际独立原请求互斥。
/// A retained reservation namespace is tested separately with ordinary query Busy before this request-level probe.
/// 保留预留命名空间在此请求级探测之前，使用普通查询 Busy 单独验证。
fn busy(harness: &Harness, owner: &PendingLocalPluginUpdate) {
    let store =
        VulcanPluginGenerationStore::open_existing(harness.fixture.runtime.user_plugin_root())
            .expect("independent store")
            .expect("existing store");
    let record = store
        .activation_request(PLUGIN, owner.request_id)
        .expect("actual record")
        .expect("original exists");
    let error = match record {
        Record::Preparation(_) => store
            .resume_activation_preparation(PLUGIN, owner.request_id)
            .expect_err("original file still owned"),
        Record::Activation(_) => store
            .resume_activation_transaction(PLUGIN, owner.request_id)
            .expect_err("original file still owned"),
    };
    assert_eq!(
        error.code,
        Code::Busy,
        "exact original request must retain its physical File"
    );
}

/// Add a manifest-declared local secret; preparation never starts its inert native executable.
/// 添加清单声明本地秘密；准备绝不启动其惰性原生可执行文件。
/// Returns a real inspected local package whose backend work is bound to one exact original key.
/// 返回实际已检查本地插件包，其后端工作绑定单个精确原键。
fn credential_harness() -> Harness {
    let harness = Harness::with_failed_native_candidate();
    let manifest = harness.source.path().join(PLUGIN).join("plugin.yaml");
    let mut body = std::fs::read_to_string(&manifest)
        .expect("manifest")
        .replace(
            "permissions: [process_binary, process_session]",
            "permissions: [process_binary, process_session, secrets_read]",
        );
    body.push_str("        env:\n          FIXTURE_TOKEN: {source: secret, key: first}\n");
    std::fs::write(manifest, body).expect("declared secret fixture");
    harness
}

/// Isolated actual file IO implements the existing backend contract and records completed writes and delete attempts.
/// 隔离实际文件输入输出实现既有后端契约，并记录已完成写入及删除尝试。
struct FileBackend {
    /// Temporary storage contains only this test's exact revision entries.
    /// 临时存储仅包含此测试精确修订条目。
    root: tempfile::TempDir,
    /// Read observations include actual file presence and value verification.
    /// 读取观察包含实际文件存在性及值校验。
    reads: Cell<usize>,
    /// Count only successful actual file writes.
    /// 仅统计成功实际文件写入。
    writes: Cell<usize>,
    /// Delete attempts reveal terminal retry without replaying credential creation.
    /// 删除尝试揭示终态重试，不重放凭据创建。
    deletes: Cell<usize>,
    /// A single explicit backend failure leaves the physical entry intact for original cleanup retry.
    /// 单次明确后端失败保留物理条目，供原清理重试。
    fail_delete: Cell<bool>,
}
impl FileBackend {
    /// Create isolated physical storage and zero observations; no production keyring account is addressed.
    /// 创建隔离物理存储及零观察；不寻址生产凭据库账户。
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("file backend"),
            reads: Cell::new(0),
            writes: Cell::new(0),
            deletes: Cell::new(0),
            fail_delete: Cell::new(false),
        }
    }
    /// Address exactly the core-supplied tuple from the fixture's declared canonical identifiers.
    /// 从夹具声明规范标识寻址精确核心提供元组。
    fn path(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> PathBuf {
        self.root.path().join(format!(
            "{plugin}-{}-{}-{}",
            revision.as_str(),
            key.server_id,
            key.secret_key
        ))
    }
}
impl VulcanPluginCredentialBackend for FileBackend {
    /// Read the actual tuple file; only NotFound is physical absence.
    /// 读取实际元组文件；仅 NotFound 为物理缺失。
    fn read(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        self.reads.set(self.reads.get() + 1);
        match std::fs::read_to_string(self.path(plugin, revision, key)) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
    /// Write the actual tuple file before counting a successful backend effect.
    /// 在统计成功后端副作用之前写入实际元组文件。
    fn write(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        std::fs::write(self.path(plugin, revision, key), value)
            .map_err(|error| error.to_string())?;
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
    /// Delete the actual tuple file, preserving it on a declared one-shot failure.
    /// 删除实际元组文件，在声明单次失败时保留它。
    fn delete(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        self.deletes.set(self.deletes.get() + 1);
        if self.fail_delete.replace(false) {
            return Err(String::from("declared file deletion failure"));
        }
        match std::fs::remove_file(self.path(plugin, revision, key)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Unpublished original reservations expose confirmed null phase and survive ordinary disk Busy until same-owner abort.
/// 未发布原预留公开已确认空阶段，并跨普通磁盘 Busy 存续至同所有者中止。
#[tokio::test]
async fn pending_update_early_reserved_query_and_original_abort() {
    let harness = Harness::new();
    let (registry, owner) = reserve(&harness, false);
    // The pre-journal strong slot must preserve the first service issuer, not merely another registration of its manager.
    // 日志前强槽必须保留首次服务签发者，而非仅同管理器的另一次注册。
    let (manager, authority) = service_vulcan_plugin_native_mcp_host(&harness.fixture.state)
        .expect("first actual service host");
    {
        // Borrow the actual reserved owner before inspecting or aborting its original physical files.
        // 检查或中止原物理文件之前，借用实际预留所有者。
        let resources = owner.resources.lock().await;
        assert!(resources.authority.same_issuer(&authority));
        assert!(resources.authority.matches_manager(&manager));
    }
    assert!(
        list(&harness.fixture.runtime, PLUGIN)
            .expect_err("actual retained namespace excludes disk query")
            .contains("still holds the requested lease")
    );
    let rows =
        service_local_plugin_update_requests(Arc::clone(&harness.fixture.state), PLUGIN.into())
            .await
            .expect("sealed original query");
    let row = rows
        .iter()
        .find(|row| row.request_id == owner.request_id.to_string())
        .expect("actual reserved original");
    assert_eq!(row.phase, None);
    assert!(row.has_service_owner);
    let original_source = std::fs::read(harness.source.path().join(PLUGIN).join("plugin.yaml"))
        .expect("source bytes");
    let result = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        false,
    )
    .await
    .expect("same borrowed reserved abort");
    assert_eq!(result.phase, Some(Phase::Aborted));
    assert!(
        registry
            .get(PLUGIN, owner.request_id)
            .expect("registry")
            .is_none()
    );
    assert_eq!(
        original_source,
        std::fs::read(harness.source.path().join(PLUGIN).join("plugin.yaml"))
            .expect("source remains")
    );
}

/// Actual first journal success followed by worker unwind leaves the registered original File and sealed history alive.
/// 实际首份日志成功后工作线程展开，让已注册原 File 及密封历史存活。
#[tokio::test]
async fn pending_update_early_first_journal_worker_panic_retains_owner() {
    let harness = Harness::new();
    let (registry, owner) = reserve(&harness, false);
    let worker_owner = Arc::clone(&owner);
    let error = run_extension_management_blocking("first original writer unwind", move || {
        let _fault = observation::arm(worker_owner.request_id, Boundary::Journal, true);
        materialize_preparation(&mut worker_owner.resources.blocking_lock())
    })
    .await
    .expect_err("real worker panic reported");
    assert!(!error.is_empty());
    assert!(Arc::ptr_eq(
        &registry
            .get(PLUGIN, owner.request_id)
            .expect("map")
            .expect("original retained"),
        &owner
    ));
    let rows =
        service_local_plugin_update_requests(Arc::clone(&harness.fixture.state), PLUGIN.into())
            .await
            .expect("sealed actual journal");
    assert_eq!(
        rows.iter()
            .find(|row| row.request_id == owner.request_id.to_string())
            .expect("original")
            .phase,
        Some(Phase::Preparing)
    );
    {
        let mut resources = owner.resources.lock().await;
        publish_reserved(&mut resources).expect("same original readback transfer");
    }
    busy(&harness, &owner);
    service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        true,
    )
    .await
    .expect("same original abort");
}

/// Actual completed copy with a lost observer recovers its strict marker after deleting the original source, without rewriting bytes.
/// 实际完整复制的观察者丢失后，删除原来源仍可恢复严格标记，不重写字节。
#[tokio::test]
async fn pending_update_early_completed_package_resumes_without_source_copy() {
    let harness = Harness::new();
    let (_, owner) = reserve(&harness, false);
    let (package, bytes, modified) = {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::Package, false);
        assert!(materialize_preparation(&mut resources).is_err());
        let PendingLocalPluginUpdateStage::Preparing(preparing) = &resources.stage else {
            panic!("actual early owner expected");
        };
        let file = preparing
            .generation
            .as_ref()
            .expect("real package pin retained")
            .package_root()
            .join("skills/review-helper/SKILL.md");
        let bytes = std::fs::read(&file).expect("copy bytes");
        let modified = std::fs::metadata(&file)
            .expect("copy metadata")
            .modified()
            .expect("mtime");
        (file, bytes, modified)
    };
    std::fs::remove_dir_all(harness.source.path().join(PLUGIN))
        .expect("delete original source within fixture root");
    {
        let mut resources = owner.resources.lock().await;
        recover_preparation(&mut resources).expect("recover completed marker, no source");
        assert_eq!(
            resources
                .prepared()
                .expect("actual promotion")
                .transaction
                .record()
                .request_id(),
            owner.request_id
        );
    }
    assert_eq!(std::fs::read(&package).expect("same bytes"), bytes);
    assert_eq!(
        std::fs::metadata(&package)
            .expect("same metadata")
            .modified()
            .expect("mtime"),
        modified
    );
    busy(&harness, &owner);
    service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        false,
    )
    .await
    .expect("original prepared abort without candidate attempt");
}

/// Missing package recovery refuses copying even with valid source, while the same original abort confirms absence.
/// 即使来源有效，缺失插件包恢复仍拒绝复制，而相同原中止确认缺失。
#[tokio::test]
async fn pending_update_early_missing_package_requires_original_abort() {
    let harness = Harness::new();
    let (_, owner) = reserve(&harness, false);
    {
        let mut resources = owner.resources.lock().await;
        publish_reserved(&mut resources).expect("actual journal");
        let error = recover_preparation(&mut resources).expect_err("recovery cannot copy");
        assert!(error.contains("不能重复制"), "{error}");
    }
    busy(&harness, &owner);
    let result = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        true,
    )
    .await
    .expect("same missing package abort");
    assert_eq!(result.phase, Some(Phase::Aborted));
}

/// Completed package corruption rejects recovery and cleanup while retaining the original Aborted owner until exact fixture bytes return.
/// 已完成插件包损坏时恢复及清理均被拒绝，并保留原 Aborted 所有者，直至精确夹具字节恢复。
#[tokio::test]
async fn pending_update_early_completed_package_corruption_retains_original_aborted_cleanup() {
    let harness = Harness::new();
    let (registry, owner) = reserve(&harness, false);
    let package = {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::Package, false);
        assert!(materialize_preparation(&mut resources).is_err());
        let PendingLocalPluginUpdateStage::Preparing(preparing) = &resources.stage else {
            panic!("actual preparation");
        };
        preparing
            .generation
            .as_ref()
            .expect("actual pin")
            .package_root()
            .to_path_buf()
    };
    // Preserve the exact successfully copied bytes before introducing a completed-package corruption.
    // 在引入已完成插件包损坏之前，保留精确成功复制字节。
    let skill = package.join("skills/review-helper/SKILL.md");
    let original_bytes = std::fs::read(&skill).expect("exact completed fixture bytes");
    let source_skill = harness
        .source
        .path()
        .join(PLUGIN)
        .join("skills/review-helper/SKILL.md");
    let source_bytes = std::fs::read(&source_skill).expect("original source bytes");
    let source_modified = std::fs::metadata(&source_skill)
        .expect("source metadata")
        .modified()
        .expect("source mtime");
    let corrupted_bytes = b"corrupted completed package bytes";
    std::fs::write(&skill, corrupted_bytes).expect("actual completed candidate byte corruption");
    {
        let mut resources = owner.resources.lock().await;
        let error = recover_preparation(&mut resources).expect_err("strict package rejects bytes");
        assert!(error.contains("不能重复制"), "{error}");
    }
    busy(&harness, &owner);
    let error = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        true,
    )
    .await
    .expect_err("strict Prepared collector must reject corrupted completed bytes");
    assert!(
        error.contains("SKILL.md must start with YAML frontmatter."),
        "{error}"
    );
    assert!(Arc::ptr_eq(
        &registry
            .get(PLUGIN, owner.request_id)
            .expect("registry")
            .expect("same retained original"),
        &owner
    ));
    assert!(package.exists());
    assert_eq!(
        std::fs::read(&skill).expect("corruption remains for inspection"),
        corrupted_bytes
    );
    // The terminal journal is real, while the same physical request File still excludes a second store's successor.
    // 终态日志为真实记录，同时相同物理请求 File 仍排除第二存储的后继。
    let observer =
        VulcanPluginGenerationStore::open_existing(harness.fixture.runtime.user_plugin_root())
            .expect("independent observer")
            .expect("store");
    let original = observer
        .activation_request(PLUGIN, owner.request_id)
        .expect("actual terminal record")
        .expect("original exists");
    assert_eq!(original.phase(), Phase::Aborted);
    assert_eq!(
        observer
            .generation_phase(
                original
                    .candidate()
                    .expect("original completed package identity")
            )
            .expect("actual retained marker"),
        vulcan_plugin_runtime::VulcanPluginGenerationPhase::Prepared
    );
    let rows =
        service_local_plugin_update_requests(Arc::clone(&harness.fixture.state), PLUGIN.into())
            .await
            .expect("terminal live owner query");
    let row = rows
        .iter()
        .find(|row| row.request_id == owner.request_id.to_string())
        .expect("original terminal row");
    assert_eq!(row.phase, Some(Phase::Aborted));
    assert!(row.has_service_owner);
    let journal_directory = harness
        .fixture
        .runtime
        .user_plugin_root()
        .join(".vulcan-generations")
        .join("transactions")
        .join(PLUGIN);
    let journal_path = journal_directory.join(format!("{}.json", owner.request_id));
    let journal = std::fs::read(&journal_path).expect("actual terminal journal bytes");
    let index =
        std::fs::read(journal_directory.join("current")).expect("actual terminal index bytes");
    let preview = harness
        .fixture
        .runtime
        .inspect_local_user_plugin(&harness.source.path().join(PLUGIN))
        .expect("unchanged approved source");
    let selected = observer.activation(PLUGIN).expect("full actual selection");
    let successor = uuid::Uuid::new_v4();
    let exclusion = observer
        .begin_activation_update_preparation(
            successor,
            &harness.source.path().join(PLUGIN),
            &preview.content_fingerprint,
            selected.as_ref(),
            preview.permissions,
            *original.update_policy().expect("original frozen policy"),
        )
        .expect_err("same terminal File excludes independent successor");
    assert_eq!(exclusion.code, Code::Busy);
    assert_eq!(
        exclusion.message,
        "Terminal activation still retains its original live owner."
    );
    assert_eq!(
        observer
            .activation_request(PLUGIN, owner.request_id)
            .expect("unchanged original"),
        Some(original.clone())
    );
    assert!(
        observer
            .activation_request(PLUGIN, successor)
            .expect("no successor journal")
            .is_none()
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("unchanged terminal journal"),
        journal
    );
    assert_eq!(
        std::fs::read(journal_directory.join("current")).expect("unchanged terminal index"),
        index
    );
    // Restore only these isolated fixture bytes; production performs no repair or source recopy.
    // 仅恢复这些隔离夹具字节；生产不执行修复或来源重复制。
    std::fs::write(&skill, original_bytes).expect("restore exact original fixture bytes");
    let result = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        false,
    )
    .await
    .expect("same Aborted original cleanup without new acknowledgement");
    assert_eq!(result.phase, Some(Phase::Aborted));
    assert!(!package.exists());
    assert!(
        registry
            .get(PLUGIN, owner.request_id)
            .expect("registry")
            .is_none()
    );
    assert!(harness.source.path().join(PLUGIN).exists());
    assert_eq!(
        std::fs::read(&source_skill).expect("source remains unchanged"),
        source_bytes
    );
    assert_eq!(
        std::fs::metadata(&source_skill)
            .expect("source metadata remains")
            .modified()
            .expect("source mtime"),
        source_modified
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("cleanup never rewrites terminal journal"),
        journal
    );
    assert_eq!(
        std::fs::read(journal_directory.join("current"))
            .expect("cleanup never replaces terminal index"),
        index
    );
}

/// A credential reservation's actual namespace supports sealed complete Preparing query while ordinary disk reads are Busy.
/// 凭据预留实际命名空间支持密封完整 Preparing 查询，同时普通磁盘读取为 Busy。
#[tokio::test]
async fn pending_update_early_credential_namespace_query_and_abort() {
    let harness = Harness::new();
    let (_, owner) = reserve(&harness, false);
    {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::CredentialReservation, false);
        assert!(materialize_preparation(&mut resources).is_err());
    }
    assert!(list(&harness.fixture.runtime, PLUGIN).is_err());
    let rows =
        service_local_plugin_update_requests(Arc::clone(&harness.fixture.state), PLUGIN.into())
            .await
            .expect("sealed original credential namespace");
    let row = rows
        .iter()
        .find(|row| row.request_id == owner.request_id.to_string())
        .expect("original");
    assert_eq!(row.phase, Some(Phase::Preparing));
    assert!(row.has_service_owner);
    service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        true,
    )
    .await
    .expect("original inventory-only publication and cleanup");
}

/// Real file backend keeps the frozen original value across first inventory and Staged result loss, then retries Aborted cleanup only.
/// 真实文件后端跨首清单及 Staged 结果丢失保持固定原值，随后仅重试 Aborted 清理。
#[tokio::test]
async fn pending_update_early_frozen_values_and_terminal_file_backend_cleanup_retry() {
    let harness = credential_harness();
    let (registry, owner) = reserve(&harness, true);
    let backend = FileBackend::new();
    {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::Inventory, false);
        assert!(materialize_preparation(&mut resources).is_err());
        assert_eq!(backend.writes.get(), 0);
        resources
            .source
            .as_mut()
            .expect("original approved inputs")
            .supplied
            .insert(("local".into(), "first".into()), "later-input".into());
    }
    std::fs::remove_dir_all(harness.source.path().join(PLUGIN))
        .expect("remove original source in fixture");
    let (revision, key, package) = {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::Staged, false);
        assert!(recover_preparation_with_backend(&mut resources, &backend).is_err());
        let record = resources
            .inventory
            .as_ref()
            .expect("actual staged inventory");
        assert_eq!(record.phase(), VulcanPluginCredentialPhase::Staged);
        let PendingLocalPluginUpdateStage::Preparing(preparing) = &resources.stage else {
            panic!("original owner remains");
        };
        (
            record.revision().clone(),
            record
                .keys()
                .iter()
                .find(|key| key.secret_key == "first")
                .expect("declared first key")
                .clone(),
            preparing
                .generation
                .as_ref()
                .expect("original package pin")
                .package_root()
                .to_path_buf(),
        )
    };
    assert_eq!(backend.writes.get(), 1);
    assert!(backend.reads.get() > 0);
    assert_eq!(
        std::fs::read_to_string(backend.path(PLUGIN, &revision, &key))
            .expect("actual frozen bytes"),
        "original-value"
    );
    backend.fail_delete.set(true);
    {
        let mut resources = owner.resources.lock().await;
        assert!(abort_preparation_with_backend(&mut resources, true, &backend).is_err());
    }
    let rows =
        service_local_plugin_update_requests(Arc::clone(&harness.fixture.state), PLUGIN.into())
            .await
            .expect("original terminal live owner");
    let row = rows
        .iter()
        .find(|row| row.request_id == owner.request_id.to_string())
        .expect("original");
    assert_eq!(row.phase, Some(Phase::Aborted));
    assert!(row.has_service_owner);
    let journal =
        VulcanPluginGenerationStore::open_existing(harness.fixture.runtime.user_plugin_root())
            .expect("observer")
            .expect("store")
            .activation_request(PLUGIN, owner.request_id)
            .expect("terminal record")
            .expect("exists");
    {
        let mut resources = owner.resources.lock().await;
        assert!(
            abort_preparation_with_backend(&mut resources, false, &backend)
                .expect("same terminal cleanup without new acknowledgement")
        );
        registry
            .retire(&owner, &resources)
            .expect("actual original retirement");
    }
    assert_eq!(backend.writes.get(), 1);
    assert_eq!(backend.deletes.get(), 2);
    assert!(!backend.path(PLUGIN, &revision, &key).exists());
    assert!(!package.exists());
    let observer =
        VulcanPluginGenerationStore::open_existing(harness.fixture.runtime.user_plugin_root())
            .expect("observer")
            .expect("store");
    assert_eq!(
        observer
            .activation_request(PLUGIN, owner.request_id)
            .expect("readonly original"),
        Some(journal)
    );
    assert!(
        observer
            .activation_effects(PLUGIN, owner.request_id)
            .expect("no callbacks")
            .is_none()
    );
    assert!(
        observer
            .installation_receipt(PLUGIN, owner.request_id)
            .expect("no installation")
            .is_none()
    );
}

/// Actual promotion writer success with observer panic retains the original preparation and claims only frozen configured target.
/// 实际晋级写入成功且观察者 panic 时保留原准备，并仅认领固定已配置目标。
#[tokio::test]
async fn pending_update_early_promotion_panic_claims_same_file_and_config() {
    let harness = Harness::new();
    let (_, owner) = reserve(&harness, false);
    let worker = Arc::clone(&owner);
    run_extension_management_blocking("actual original promotion panic", move || {
        let _fault = observation::arm(worker.request_id, Boundary::Promotion, true);
        materialize_preparation(&mut worker.resources.blocking_lock())
    })
    .await
    .expect_err("actual writer observer panic");
    std::fs::remove_dir_all(harness.source.path().join(PLUGIN))
        .expect("source removed within fixture");
    let original = {
        let resources = owner.resources.lock().await;
        let PendingLocalPluginUpdateStage::Preparing(preparing) = &resources.stage else {
            panic!("original preparation retained after lost promotion");
        };
        preparing
            .store
            .activation_request(PLUGIN, owner.request_id)
            .expect("actual target")
            .expect("actual Prepared")
    };
    {
        let mut resources = owner.resources.lock().await;
        recover_preparation(&mut resources).expect("same File borrowed readonly promotion");
        assert_eq!(
            Record::Activation(
                resources
                    .prepared()
                    .expect("same transaction")
                    .transaction
                    .record()
                    .clone()
            ),
            original
        );
    }
    busy(&harness, &owner);
    service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        false,
    )
    .await
    .expect("same promoted original abort");
}

/// Share only the actual file backend; each backend call releases its mutex before any original writer observer executes.
/// 仅共享实际文件后端；每次后端调用在任何原写入器观察器执行之前释放互斥量。
#[derive(Clone)]
struct SharedFileBackend {
    /// The original backend and its physical directory survive a genuine blocking-worker unwind.
    /// 原后端及其物理目录跨真实阻塞工作线程展开存续。
    actual: Arc<Mutex<FileBackend>>,
}
impl SharedFileBackend {
    /// Allocate one actual backend; cloning this wrapper never creates another credential namespace or file.
    /// 分配单个实际后端；克隆此包装绝不创建另一凭据命名空间或文件。
    fn new() -> Self {
        Self {
            actual: Arc::new(Mutex::new(FileBackend::new())),
        }
    }
    /// Read actual method observations after the worker borrow has retired; a poisoned test backend remains an error.
    /// 工作线程借用退役后读取实际方法观察；被污染的测试后端保持错误。
    fn counts(&self) -> (usize, usize, usize) {
        let actual = self
            .actual
            .lock()
            .expect("backend methods returned before original observer loss");
        (
            actual.reads.get(),
            actual.writes.get(),
            actual.deletes.get(),
        )
    }
    /// Return the exact existing tuple path, without performing another backend read or opening any file lease.
    /// 返回精确既有元组路径，不执行另一后端读取，也不打开任何文件租约。
    fn path(
        &self,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> PathBuf {
        self.actual
            .lock()
            .expect("unpoisoned original backend")
            .path(PLUGIN, revision, key)
    }
}
impl VulcanPluginCredentialBackend for SharedFileBackend {
    /// Forward one real file read under a method-local mutex; return exact physical absence or error.
    /// 在方法局部互斥量下转发单次真实文件读取；返回精确物理缺失或错误。
    fn read(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        self.actual
            .lock()
            .map_err(|error| error.to_string())?
            .read(plugin, revision, key)
    }
    /// Forward one real file write; its mutex is dropped before the caller's post-success observer runs.
    /// 转发单次真实文件写入；其互斥量在调用者成功后观察器运行之前释放。
    fn write(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        self.actual
            .lock()
            .map_err(|error| error.to_string())?
            .write(plugin, revision, key, value)
    }
    /// Forward one real file deletion and verification contract, never retaining the mutex across original journal callbacks.
    /// 转发单次真实文件删除及校验契约，绝不跨原日志回调保留互斥量。
    fn delete(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        self.actual
            .lock()
            .map_err(|error| error.to_string())?
            .delete(plugin, revision, key)
    }
}

/// Snapshot actual successful file bytes and modification time so readback retries cannot silently repeat a writer.
/// 快照实际成功文件字节及修改时间，使回读重试不能静默重复写入器。
fn file_observation(path: &Path) -> (Vec<u8>, std::time::SystemTime) {
    (
        std::fs::read(path).expect("actual checkpoint bytes"),
        std::fs::metadata(path)
            .expect("actual checkpoint metadata")
            .modified()
            .expect("actual checkpoint mtime"),
    )
}

/// Prove terminal original File exclusion with an actual new request reservation and preserve full original durable bytes.
/// 使用实际新请求预留证明终态原 File 互斥，并保留完整原持久字节。
/// Original is the exact decoded Aborted body; source approval and predecessor selection come from actual current observations.
/// original 为精确解码 Aborted 正文；来源批准及前驱选择来自实际当前观察。
fn terminal_file_exclusion(harness: &Harness, owner: &PendingLocalPluginUpdate, original: &Record) {
    let observer =
        VulcanPluginGenerationStore::open_existing(harness.fixture.runtime.user_plugin_root())
            .expect("independent terminal observer")
            .expect("actual store");
    let preview = harness
        .fixture
        .runtime
        .inspect_local_user_plugin(&harness.source.path().join(PLUGIN))
        .expect("actual successor authorization");
    let selected = observer
        .activation(PLUGIN)
        .expect("full original selection");
    let indexed = observer
        .activation_transaction_ids(PLUGIN)
        .expect("actual original index inventory");
    let successor = uuid::Uuid::new_v4();
    let error = observer
        .begin_activation_update_preparation(
            successor,
            &harness.source.path().join(PLUGIN),
            &preview.content_fingerprint,
            selected.as_ref(),
            preview.permissions,
            *original.update_policy().expect("frozen original policy"),
        )
        .expect_err("actual terminal original File must remain exclusive");
    assert_eq!(error.code, Code::Busy);
    assert_eq!(
        error.message,
        "Terminal activation still retains its original live owner."
    );
    assert_eq!(
        observer
            .activation_request(PLUGIN, owner.request_id)
            .expect("unchanged full original"),
        Some(original.clone())
    );
    assert_eq!(
        observer
            .activation(PLUGIN)
            .expect("unchanged complete selection"),
        selected
    );
    assert_eq!(
        observer
            .activation_transaction_ids(PLUGIN)
            .expect("unchanged original inventory"),
        indexed
    );
    assert!(
        observer
            .activation_request(PLUGIN, successor)
            .expect("successor absent")
            .is_none()
    );
}

/// Exercise one exact terminal observation loss after the real core writer or collector returns, then retry only the same original cleanup.
/// 在真实核心写入器或回收器返回后覆盖单个精确终态观察丢失，再仅重试相同原清理。
/// Boundary selects actual Aborted, Discarded or Collected; panic selects a genuine blocking-executor unwind.
/// boundary 选择实际 Aborted、Discarded 或 Collected；panic 选择真实阻塞执行器展开。
async fn terminal_cleanup_loss(boundary: Boundary, panic: bool) {
    let harness = credential_harness();
    let (registry, owner) = reserve(&harness, true);
    let backend = SharedFileBackend::new();
    {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::Inventory, false);
        materialize_preparation(&mut resources)
            .expect_err("first actual inventory retains frozen values before backend work");
    }
    let (store, package, revision, key) = {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::Staged, false);
        recover_preparation_with_backend(&mut resources, &backend)
            .expect_err("actual Staged success remains retained before promotion");
        let PendingLocalPluginUpdateStage::Preparing(preparing) = &resources.stage else {
            panic!("actual original preparation retained");
        };
        let inventory = resources
            .inventory
            .as_ref()
            .expect("actual Staged inventory");
        assert_eq!(inventory.phase(), VulcanPluginCredentialPhase::Staged);
        (
            preparing.store.clone(),
            preparing
                .generation
                .as_ref()
                .expect("actual package pin")
                .package_root()
                .to_path_buf(),
            inventory.revision().clone(),
            inventory
                .keys()
                .iter()
                .find(|key| key.secret_key == "first")
                .expect("actual declared key")
                .clone(),
        )
    };
    let entry = backend.path(&revision, &key);
    assert_eq!(
        std::fs::read_to_string(&entry).expect("actual original credential entry"),
        "original-value"
    );
    assert_eq!(backend.counts().1, 1);
    let worker_owner = Arc::clone(&owner);
    let worker_backend = backend.clone();
    let error = run_extension_management_blocking(
        "actual terminal original cleanup observer loss",
        move || {
            let _fault = observation::arm(worker_owner.request_id, boundary, panic);
            abort_preparation_with_backend(
                &mut worker_owner.resources.blocking_lock(),
                true,
                &worker_backend,
            )
        },
    )
    .await
    .expect_err("actual terminal success loses observer return");
    if !panic {
        assert_eq!(error, "原实际成功后的观察返回丢失。");
    } else {
        assert!(
            error.contains("original preparation observer lost after actual success"),
            "{error}"
        );
    }
    assert!(Arc::ptr_eq(
        &registry
            .get(PLUGIN, owner.request_id)
            .expect("registry")
            .expect("same terminal owner"),
        &owner
    ));
    let original = store
        .activation_request(PLUGIN, owner.request_id)
        .expect("actual terminal journal")
        .expect("original exists");
    assert_eq!(original.phase(), Phase::Aborted);
    let inventory = store
        .credential_record(PLUGIN, &revision)
        .expect("actual credential record")
        .expect("retained inventory");
    let counts = backend.counts();
    assert_eq!(counts.1, 1);
    match boundary {
        Boundary::Aborted => {
            assert_eq!(inventory.phase(), VulcanPluginCredentialPhase::Staged);
            assert!(entry.exists());
            assert!(package.exists());
            assert_eq!(counts.2, 0);
        }
        Boundary::Discarded => {
            assert_eq!(inventory.phase(), VulcanPluginCredentialPhase::Discarded);
            assert!(!entry.exists());
            assert!(package.exists());
            assert_eq!(counts.2, 1);
        }
        Boundary::Collected => {
            assert_eq!(inventory.phase(), VulcanPluginCredentialPhase::Discarded);
            assert!(!entry.exists());
            assert!(!package.exists());
            assert_eq!(counts.2, 1);
            assert_eq!(
                store
                    .generation_phase(original.candidate().expect("actual candidate"))
                    .expect("actual final marker"),
                vulcan_plugin_runtime::VulcanPluginGenerationPhase::Collected
            );
        }
        _ => panic!("only actual terminal cleanup checkpoints are accepted"),
    }
    let rows =
        service_local_plugin_update_requests(Arc::clone(&harness.fixture.state), PLUGIN.into())
            .await
            .expect("terminal original query without joining worker");
    let row = rows
        .iter()
        .find(|row| row.request_id == owner.request_id.to_string())
        .expect("terminal original row");
    assert_eq!(row.phase, Some(Phase::Aborted));
    assert!(row.has_service_owner);
    // Paths below are the actual source-defined store layout already used by the original terminal regression.
    // 下述路径是原终态回归已经使用的实际源码定义存储布局。
    let root = harness
        .fixture
        .runtime
        .user_plugin_root()
        .join(".vulcan-generations");
    let journal = root
        .join("transactions")
        .join(PLUGIN)
        .join(format!("{}.json", owner.request_id));
    let index = journal
        .parent()
        .expect("actual journal directory")
        .join("current");
    let journal_before = file_observation(&journal);
    let index_before = file_observation(&index);
    let inventory_path = root
        .join("credentials")
        .join(PLUGIN)
        .join(format!("{}.json", revision.as_str()));
    let discarded_before = if boundary != Boundary::Aborted {
        Some(file_observation(&inventory_path))
    } else {
        None
    };
    let marker = package
        .parent()
        .expect("actual generation parent")
        .join("generation.json");
    let marker_before = if boundary == Boundary::Collected {
        Some(file_observation(&marker))
    } else {
        None
    };
    terminal_file_exclusion(&harness, &owner, &original);
    assert_eq!(file_observation(&journal), journal_before);
    assert_eq!(file_observation(&index), index_before);
    let retry_owner = Arc::clone(&owner);
    let retry_backend = backend.clone();
    run_extension_management_blocking("readonly original terminal cleanup retry", move || {
        abort_preparation_with_backend(
            &mut retry_owner.resources.blocking_lock(),
            false,
            &retry_backend,
        )
    })
    .await
    .expect("same Aborted cleanup needs no new acknowledgement");
    let after = backend.counts();
    assert_eq!(after.1, 1);
    assert_eq!(after.2, 1);
    if boundary != Boundary::Aborted {
        assert_eq!(
            after, counts,
            "already Discarded and Collected retries must perform zero backend calls"
        );
    }
    assert!(!entry.exists());
    assert!(!package.exists());
    assert_eq!(file_observation(&journal), journal_before);
    assert_eq!(file_observation(&index), index_before);
    if let Some(marker_before) = marker_before {
        assert_eq!(
            file_observation(&marker),
            marker_before,
            "actual Collected marker must not be written twice"
        );
    }
    if let Some(discarded_before) = discarded_before {
        assert_eq!(
            file_observation(&inventory_path),
            discarded_before,
            "actual Discarded inventory must not be written twice"
        );
    }
    assert_eq!(
        store
            .activation_request(PLUGIN, owner.request_id)
            .expect("unchanged full Aborted body"),
        Some(original)
    );
    assert!(
        store
            .activation_effects(PLUGIN, owner.request_id)
            .expect("actual absent callbacks")
            .is_none()
    );
    assert!(
        store
            .installation_receipt(PLUGIN, owner.request_id)
            .expect("actual absent installation")
            .is_none()
    );
    {
        let resources = owner.resources.lock().await;
        registry
            .retire(&owner, &resources)
            .expect("actual full original terminal retirement");
    }
    assert!(
        registry
            .get(PLUGIN, owner.request_id)
            .expect("registry")
            .is_none()
    );
}

/// Actual Aborted journal return loss preserves the original File and admits only unfinished cleanup retry.
/// 实际 Aborted 日志返回丢失保留原 File，并仅接纳未完成清理重试。
#[tokio::test]
async fn pending_update_early_aborted_return_loss_retries_original_cleanup() {
    terminal_cleanup_loss(Boundary::Aborted, false).await;
}

/// Actual Aborted success followed by blocking-worker panic preserves original ownership and zero credential replay.
/// 实际 Aborted 成功后阻塞工作线程 panic 保留原所有权，且凭据零重放。
#[tokio::test]
async fn pending_update_early_aborted_worker_panic_retries_original_cleanup() {
    terminal_cleanup_loss(Boundary::Aborted, true).await;
}

/// Actual Discarded return loss keeps original terminal ownership and performs zero additional backend calls on retry.
/// 实际 Discarded 返回丢失保持原终态所有权，且重试执行零额外后端调用。
#[tokio::test]
async fn pending_update_early_discarded_return_loss_retries_original_cleanup() {
    terminal_cleanup_loss(Boundary::Discarded, false).await;
}

/// Actual Discarded success followed by blocking-worker panic cannot drop the original package or replay credential deletion.
/// 实际 Discarded 成功后阻塞工作线程 panic 不能丢失原插件包，也不能重放凭据删除。
#[tokio::test]
async fn pending_update_early_discarded_worker_panic_retries_original_cleanup() {
    terminal_cleanup_loss(Boundary::Discarded, true).await;
}

/// Actual Collected return loss retains the original File while all subsequent terminal operations remain read-only.
/// 实际 Collected 返回丢失保留原 File，同时全部后续终态操作保持只读。
#[tokio::test]
async fn pending_update_early_collected_return_loss_retries_original_cleanup() {
    terminal_cleanup_loss(Boundary::Collected, false).await;
}

/// Actual Collected success followed by blocking-worker panic cannot rewrite the final marker or recreate original resources.
/// 实际 Collected 成功后阻塞工作线程 panic 不能重写最终标记，也不能重新创建原资源。
#[tokio::test]
async fn pending_update_early_collected_worker_panic_retries_original_cleanup() {
    terminal_cleanup_loss(Boundary::Collected, true).await;
}

/// Replace the real service manager with a separately committed managed content generation, then permit only old-owner cleanup.
/// 将真实服务管理器替换为另一个已提交受管内容代次，随后仅允许旧所有者清理。
/// Prepared selects the existing transaction abort contract; false selects early package cleanup. No live MCP recovery is claimed.
/// prepared 选择既有事务中止契约；false 选择早期插件包清理。不声称实时 MCP 恢复。
async fn manager_replacement_cleanup(prepared: bool) {
    let harness = Harness::new();
    let (registry, owner) = reserve(&harness, false);
    if prepared {
        // Borrow the original slot only inside this branch; release its guard before any later service RPC awaits.
        // 仅在此分支内借用原槽；在任何后续服务 RPC 等待之前释放守卫。
        let mut resources = owner.resources.lock().await;
        materialize_preparation(&mut resources)
            .expect("same registered original Prepared pipeline");
    } else {
        let mut resources = owner.resources.lock().await;
        let _fault = observation::arm(owner.request_id, Boundary::Package, false);
        materialize_preparation(&mut resources)
            .expect_err("original completed package retained before further work");
    }
    let old_package = {
        let resources = owner.resources.lock().await;
        match &resources.stage {
            PendingLocalPluginUpdateStage::Preparing(preparing) => preparing
                .generation
                .as_ref()
                .expect("actual old package pin")
                .package_root()
                .to_path_buf(),
            PendingLocalPluginUpdateStage::Prepared(original) => original
                .store
                .inspect_activation_candidate(&original.transaction)
                .expect("actual original candidate")
                .package()
                .package_root
                .clone(),
            _ => panic!("actual Preparing or Prepared owner expected"),
        }
    };
    // A second complete production service commits its real package, full configured selection and extension generation.
    // 第二个完整生产服务提交真实插件包、完整已配置选择及扩展代次。
    let current = Harness::new();
    let source = current.source.path().join(PLUGIN);
    let preview = current
        .fixture
        .runtime
        .inspect_local_user_plugin(&source)
        .expect("current actual candidate authorization");
    service_update_local_plugin(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&current.fixture.state),
        source.to_string_lossy().into_owned(),
        preview.content_fingerprint,
        preview.permissions,
        false,
        Vec::new(),
    )
    .await
    .expect("actual separately managed current publisher");
    let current_catalog = current.fixture.runtime.snapshot();
    let full_catalog = format!("{current_catalog:?}");
    assert_ne!(
        current_catalog.service_epoch,
        harness.fixture.runtime.snapshot().service_epoch
    );
    let current_store =
        VulcanPluginGenerationStore::open_existing(current.fixture.runtime.user_plugin_root())
            .expect("current store")
            .expect("actual managed store");
    let current_selection = current_store
        .activation(PLUGIN)
        .expect("actual current full selection")
        .expect("managed current target is present");
    assert_eq!(
        current_selection
            .generation()
            .expect("actual selected immutable generation")
            .version,
        "3.0.0"
    );
    let current_package = &current_catalog
        .plugins
        .get(PLUGIN)
        .expect("actual current catalog plugin")
        .package_root;
    let current_manifest = file_observation(&current_package.join("plugin.yaml"));
    let current_skill = file_observation(&current_package.join("skills/review-helper/SKILL.md"));
    let current_extension = Arc::clone(
        &current
            .fixture
            .state
            .lock()
            .expect("current state")
            .agent_runtime
            .as_ref()
            .expect("current bridge")
            .extension_generation,
    );
    // Capture the complete replacement pair before acquiring another fixture's publication mutex.
    // 取得另一夹具发布互斥量之前，捕获完整替代配对。
    let (replacement_manager, replacement_authority) =
        service_vulcan_plugin_native_mcp_host(&current.fixture.state)
            .expect("replacement service host pair");
    {
        let mut service = harness
            .fixture
            .state
            .lock()
            .expect("original service bridge");
        let bridge = service
            .agent_runtime
            .as_mut()
            .expect("actual service bridge");
        bridge.vulcan_plugin_runtime = replacement_manager;
        bridge.vulcan_plugin_native_mcp_host = replacement_authority;
        bridge.extension_generation = Arc::clone(&current_extension);
    }
    let error = service_resume_local_plugin_update(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
    )
    .await
    .expect_err("original issuer cannot publish under replacement manager");
    assert!(error.contains("服务插件管理器已改变"), "{error}");
    assert!(Arc::ptr_eq(
        &registry
            .get(PLUGIN, owner.request_id)
            .expect("registry")
            .expect("same original retained"),
        &owner
    ));
    busy(&harness, &owner);
    let result = service_abort_local_plugin_update(
        Arc::clone(&harness.fixture.state),
        PLUGIN.into(),
        owner.request_id.to_string(),
        true,
    )
    .await
    .expect("old manager clean-only uses exact original owner");
    assert_eq!(result.phase, Some(Phase::Aborted));
    assert!(
        registry
            .get(PLUGIN, owner.request_id)
            .expect("registry")
            .is_none()
    );
    assert_eq!(
        old_package.exists(),
        prepared,
        "Preparing collects original package; Prepared keeps its existing explicit-abandonment contract"
    );
    let service = harness.fixture.state.lock().expect("final actual service");
    let bridge = service
        .agent_runtime
        .as_ref()
        .expect("current bridge remains");
    assert!(Arc::ptr_eq(
        &bridge.extension_generation,
        &current_extension
    ));
    assert!(Arc::ptr_eq(
        &bridge.vulcan_plugin_runtime.snapshot(),
        &current_catalog
    ));
    assert_eq!(
        format!("{:?}", bridge.vulcan_plugin_runtime.snapshot()),
        full_catalog
    );
    assert_eq!(
        current_store
            .activation(PLUGIN)
            .expect("current full selection remains"),
        Some(current_selection)
    );
    assert_eq!(
        file_observation(&current_package.join("plugin.yaml")),
        current_manifest
    );
    assert_eq!(
        file_observation(&current_package.join("skills/review-helper/SKILL.md")),
        current_skill
    );
}

/// Replaced current manager rejects early resume but permits exact old preparation cleanup without changing new managed routes.
/// 被替换当前管理器拒绝早期恢复，但允许精确旧准备清理，而不改变新受管路由。
#[tokio::test]
async fn pending_update_early_replaced_manager_preparing_cleanup_preserves_current_routes() {
    manager_replacement_cleanup(false).await;
}

/// Replaced current manager rejects configured resume but permits exact old transaction retirement without changing new managed routes.
/// 被替换当前管理器拒绝已配置恢复，但允许精确旧事务退役，而不改变新受管路由。
#[tokio::test]
async fn pending_update_early_replaced_manager_prepared_cleanup_preserves_current_routes() {
    manager_replacement_cleanup(true).await;
}
