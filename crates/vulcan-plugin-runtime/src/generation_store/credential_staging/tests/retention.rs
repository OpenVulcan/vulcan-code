//! Real reader ownership protects retired credentials across clones, catalogs and process termination.
//! 实际读者所有权跨克隆、目录及进程终止保护退役凭据。

use super::*;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Finish one fixture revision and release its actual writer before returning its immutable identity.
/// 完成单个夹具修订并释放实际写入者，然后返回其不可变身份。
fn stage(
    store: &VulcanPluginGenerationStore,
    package: &PinnedVulcanPluginGeneration,
    backend: &Backend,
) -> VulcanPluginCredentialRevision {
    // Each caller receives a distinct physical secure-store namespace.
    // 每个调用方取得独立的物理安全存储命名空间。
    let revision = VulcanPluginCredentialRevision::allocate();
    // The writer remains exclusive until this function returns.
    // 写入者在此函数返回前保持排他。
    let mut writer = store
        .begin_credential_staging(package, revision.clone(), &values())
        .expect("persist retention fixture inventory");
    writer
        .write_and_finish(backend, &values())
        .expect("stage retention fixture credentials");
    assert_eq!(
        store
            .pin_credentials(package, &revision)
            .expect_err("live staging owner must block readers")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    revision
}

/// Verify candidate pins, live selections, actual cloned ownership and resumable retired cleanup.
/// 验证候选固定、有效选择、实际克隆所有权及可恢复退役清理。
#[test]
fn credential_retention_blocks_collection_until_last_reader_and_recovers_cleanup() {
    // Real journal and package bytes remain isolated throughout the test.
    // 实际日志及插件包字节在整个测试期间保持隔离。
    let temp = tempfile::tempdir().expect("create credential retention fixture");
    // One real package supplies every credential key and approval.
    // 单个实际插件包提供全部凭据键及批准权限。
    let (store, package) = fixture(temp.path());
    // Only secure backend effects are simulated in memory.
    // 仅安全后端副作用以内存模拟。
    let backend = Backend::default();
    // Pinning before activation protects readiness checks for an unpublished candidate.
    // 激活前固定保护未发布候选的就绪检查。
    let revision = stage(&store, &package, &backend);
    // This clone remains authoritative after the original reader is released.
    // 此克隆在原读者释放后仍持有权威所有权。
    let reader = store
        .pin_credentials(&package, &revision)
        .expect("pin staged candidate");
    assert_eq!(reader.record().revision(), &revision);
    assert_eq!(reader.record().package(), package.identity());
    // A second independently opened reader also shares the actual filesystem lock.
    // 第二个独立打开的读者也共享实际文件系统锁。
    let independent = store
        .pin_credentials(&package, &revision)
        .expect("pin independent reader");
    // Arc ownership must outlive the first wrapper.
    // Arc 所有权必须比第一个包装对象更长寿。
    let retained = reader.clone();
    drop(reader);
    drop(independent);
    assert_eq!(
        store
            .collect_credentials("staging-plugin", &revision, &backend)
            .expect_err("last cloned reader must block candidate collection")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(retained);
    store
        .activate_with_credentials(
            &package,
            0,
            package.required_permissions.clone(),
            revision.clone(),
        )
        .expect("select retained revision");
    assert!(
        store
            .collect_credentials("staging-plugin", &revision, &backend)
            .is_err()
    );
    // Active selection pinning must acquire credential ownership inside the same namespace fence.
    // 有效选择固定必须在同一命名空间屏障内取得凭据所有权。
    let active = store
        .pin_active("staging-plugin")
        .expect("pin active credential revision");
    assert!(active.credentials.is_some());
    store
        .deactivate("staging-plugin", 1)
        .expect("deactivate retained credentials");
    assert_eq!(
        store
            .collect_credentials("staging-plugin", &revision, &backend)
            .expect_err("old active observation must still retain credentials")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(active);
    *backend.fail_delete.borrow_mut() = Some("second".into());
    assert_eq!(
        store
            .collect_credentials("staging-plugin", &revision, &backend)
            .expect_err("partial backend cleanup must remain recoverable")
            .code,
        VulcanPluginStoreErrorCode::Storage
    );
    assert_eq!(
        store
            .credential_record("staging-plugin", &revision)
            .expect("read interrupted retired cleanup")
            .expect("retain cleanup inventory")
            .phase(),
        VulcanPluginCredentialPhase::Discarding
    );
    assert!(store.pin_credentials(&package, &revision).is_err());
    assert!(
        store
            .activate_with_credentials(
                &package,
                2,
                package.required_permissions.clone(),
                revision.clone()
            )
            .is_err()
    );
    // Reopening creates no in-memory recovery authority; the original journal owns the remaining keys.
    // 重开不创建内存恢复权威；原日志持有剩余键。
    let reopened = VulcanPluginGenerationStore::open(temp.path()).expect("reopen retired cleanup");
    *backend.fail_delete.borrow_mut() = None;
    assert!(
        reopened
            .collect_credentials("staging-plugin", &revision, &backend)
            .expect("finish retired cleanup")
    );
    assert!(
        !reopened
            .collect_credentials("staging-plugin", &revision, &backend)
            .expect("confirm terminal cleanup")
    );
    assert!(backend.values.borrow().is_empty());
    assert!(
        reopened
            .begin_credential_staging(&package, revision, &values())
            .is_err()
    );
    assert!(
        reopened
            .pin_active("staging-plugin")
            .expect("tombstone remains readable")
            .credentials
            .is_none()
    );
    // A staged readiness failure may also be reclaimed without ever publishing a selection.
    // 已暂存的就绪失败也可在从未发布选择时回收。
    let abandoned = stage(&reopened, &package, &backend);
    assert!(
        reopened
            .collect_credentials("staging-plugin", &abandoned, &backend)
            .expect("collect unpublished staged candidate")
    );
}

/// Load a real managed catalog from `root`, explicitly enabling the fixture plugin.
/// 从 `root` 加载实际受管目录，显式启用夹具插件。
fn catalog(root: &Path) -> crate::VulcanPluginCatalogSnapshot {
    crate::load_vulcan_plugin_catalog(
        &root.join("bundled"),
        Some(root),
        &BTreeSet::new(),
        &BTreeSet::from(["staging-plugin".into()]),
        1,
    )
}

/// Verify a detached MCP descriptor and managed package each retain the old revision independently.
/// 验证脱离目录的 MCP 描述符及受管插件包各自独立保留旧修订。
#[test]
fn credential_retention_follows_catalog_and_detached_mcp_descriptor_clones() {
    // Source, catalog and immutable selection share one isolated filesystem fixture.
    // 来源、目录及不可变选择共用单个隔离文件系统夹具。
    let temp = tempfile::tempdir().expect("create catalog retention fixture");
    fs::create_dir(temp.path().join("bundled")).expect("create empty bundled root");
    // The complete package is validated through production discovery.
    // 完整插件包通过生产发现进行校验。
    let (store, package) = fixture(temp.path());
    // Two revisions share a package but never a credential namespace.
    // 两个修订共用插件包，但绝不共用凭据命名空间。
    let backend = Backend::default();
    // First revision becomes the old routing snapshot.
    // 第一个修订成为旧路由快照。
    let first = stage(&store, &package, &backend);
    // Second revision replaces only the selected credentials.
    // 第二个修订仅替换选定凭据。
    let second = stage(&store, &package, &backend);
    store
        .activate_with_credentials(
            &package,
            0,
            package.required_permissions.clone(),
            first.clone(),
        )
        .expect("activate first catalog revision");
    // Production loading must retain the real lease in both owning structures.
    // 生产加载必须在两个所有权结构内保留实际租约。
    let old = catalog(temp.path());
    // Keep the package without keeping its original containing catalog.
    // 保留插件包，不保留其原所属目录。
    let managed = old.plugins["staging-plugin"]
        .managed_package()
        .expect("load managed package")
        .clone();
    // Keep only the exact remote declaration which can materialize secrets later.
    // 仅保留后续可物化秘密的精确远端声明。
    let server = old.plugins["staging-plugin"]
        .mcp_servers
        .iter()
        .find(|server| server.id == "remote")
        .expect("find retained remote descriptor")
        .clone();
    // A further independent descriptor clone must keep the lock after its parent is dropped.
    // 进一步独立描述符克隆必须在父对象释放后保持锁。
    let last_server = server.clone();
    store
        .activate_with_credentials(
            &package,
            1,
            package.required_permissions.clone(),
            second.clone(),
        )
        .expect("activate replacement catalog revision");
    // New catalog owns a different secure namespace from the old descriptors.
    // 新目录持有与旧描述符不同的安全命名空间。
    let current = catalog(temp.path());
    drop(old);
    drop(server);
    assert_eq!(
        store
            .collect_credentials("staging-plugin", &first, &backend)
            .expect_err("old managed package and descriptor must retain credentials")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(managed);
    assert_eq!(
        store
            .collect_credentials("staging-plugin", &first, &backend)
            .expect_err("detached descriptor alone must retain credentials")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        backend
            .read("staging-plugin", &first, &key("first"))
            .expect("read retained old value"),
        values()[&key("first")]
    );
    drop(last_server);
    assert!(
        store
            .collect_credentials("staging-plugin", &first, &backend)
            .expect("collect after final descriptor")
    );
    assert_eq!(
        backend
            .read("staging-plugin", &second, &key("first"))
            .expect("read replacement value"),
        values()[&key("first")]
    );
    store
        .deactivate("staging-plugin", 2)
        .expect("deactivate replacement");
    assert_eq!(
        store
            .collect_credentials("staging-plugin", &second, &backend)
            .expect_err("current catalog still owns replacement")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(current);
    assert!(
        store
            .collect_credentials("staging-plugin", &second, &backend)
            .expect("collect released replacement")
    );
}

/// Always terminate and reap the isolated helper, including when a parent assertion fails.
/// 始终终止并回收隔离辅助进程，包括父进程断言失败时。
struct ReaderChild(Child);

impl Drop for ReaderChild {
    /// Release the actual process owner and wait for exit; no credential mutation occurs here.
    /// 释放实际进程所有者并等待退出；此处不发生凭据变更。
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Prove that an actual child reader blocks reclamation until forced process exit releases the OS lock.
/// 证明实际子进程读者阻止回收，直至强制进程退出释放 OS 锁。
#[test]
fn credential_retention_survives_selection_change_until_real_process_exit() {
    // The child has its own store and no shared Rust heap with this parent.
    // 子进程具有自己的存储，且不与父进程共享 Rust 堆。
    let temp = tempfile::tempdir().expect("create process credential fixture");
    // Real package and journal state are established before the helper starts.
    // 辅助进程启动前建立实际插件包及日志状态。
    let (store, package) = fixture(temp.path());
    // Backend values stay in the parent; the child exercises only production lease acquisition.
    // 后端值留在父进程；子进程仅操作生产租约取得。
    let backend = Backend::default();
    // The selected revision is known to both processes through the real selection record.
    // 两个进程通过实际选择记录获知选定修订。
    let revision = stage(&store, &package, &backend);
    store
        .activate_with_credentials(
            &package,
            0,
            package.required_permissions.clone(),
            revision.clone(),
        )
        .expect("select child fixture revision");
    // A guarded real subprocess owns the public active-selection pin.
    // 受守卫保护的实际子进程持有公开有效选择固定。
    let mut child = ReaderChild(Command::new(std::env::current_exe().expect("resolve test executable"))
        .args(["--exact", "generation_store::credential_staging::tests::retention::credential_retention_process_helper",
            "--ignored", "--test-threads=1"])
        .env("VULCAN_CREDENTIAL_READER_TEST_ROOT", temp.path())
        .stdout(Stdio::null()).spawn().expect("spawn actual credential reader"));
    // Parent readiness is bounded and continuously verifies that the reader remains alive.
    // 父进程就绪等待有界，并持续验证读者仍存活。
    let deadline = Instant::now() + Duration::from_secs(10);
    while !temp.path().join("credential-reader-ready").exists() {
        assert!(
            child
                .0
                .try_wait()
                .expect("observe reader liveness")
                .is_none(),
            "reader exited before readiness"
        );
        assert!(
            Instant::now() < deadline,
            "reader did not acquire its actual lease"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    store
        .deactivate("staging-plugin", 1)
        .expect("deactivate child-owned credentials");
    assert_eq!(
        store
            .collect_credentials("staging-plugin", &revision, &backend)
            .expect_err("real child retains credential revision")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    child.0.kill().expect("terminate actual reader process");
    child.0.wait().expect("reap terminated reader");
    assert!(
        store
            .collect_credentials("staging-plugin", &revision, &backend)
            .expect("collect after actual process exit")
    );
}

/// Child-only helper acquires the selected credentials and holds them until termination or its bounded timeout.
/// 仅子进程使用的辅助入口取得选定凭据，并持有至终止或有界超时。
#[test]
#[ignore = "spawned by the real credential-retention process test"]
fn credential_retention_process_helper() {
    // The parent supplies one isolated root, never a production user path.
    // 父进程提供单个隔离根目录，绝不使用生产用户路径。
    let root = std::path::PathBuf::from(
        std::env::var_os("VULCAN_CREDENTIAL_READER_TEST_ROOT").expect("read isolated child root"),
    );
    // This independently opened store proves filesystem ownership rather than shared-memory coordination.
    // 此独立打开存储证明文件系统所有权，而非共享内存协调。
    let store = VulcanPluginGenerationStore::open(&root).expect("open child credential store");
    // Keep the full active observation alive throughout the wait.
    // 整个等待期间保持完整有效观察存活。
    let selected = store
        .pin_active("staging-plugin")
        .expect("pin child credentials");
    assert!(selected.credentials.is_some());
    fs::write(root.join("credential-reader-ready"), b"pinned").expect("publish child readiness");
    std::thread::sleep(Duration::from_secs(20));
    drop(selected);
    panic!("parent failed to terminate credential lease helper");
}
