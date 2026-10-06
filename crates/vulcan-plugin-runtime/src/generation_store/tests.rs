//! Real package, activation, cross-process lease, and cleanup regressions.
//! 真实插件包、激活、跨进程租约及清理回归。

use super::*;
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

mod approval;
mod configuration;
mod credentials;
mod deactivation;
mod settings;
mod transactions;

/// Write a valid content plugin beneath `root` and return its path; `body` distinguishes real package bytes.
/// 在 `root` 下写入有效内容插件并返回路径；`body` 区分真实插件包字节。
fn write_package(root: &Path, plugin_id: &str, body: &str) -> PathBuf {
    // Match the production catalog's exact directory-name and manifest identity contract.
    // 匹配生产目录的精确目录名及清单身份契约。
    let package = root.join(plugin_id);
    fs::create_dir_all(package.join("skills/review-helper"))
        .expect("create package skill directory");
    fs::write(
        package.join("skills/review-helper/SKILL.md"),
        format!("---\nname: review-helper\ndescription: Review a change.\n---\n\n{body}\n"),
    )
    .expect("write package skill");
    fs::write(package.join("plugin.yaml"), format!("schema_version: 1\nid: {plugin_id}\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Generation Fixture\n  capability: Read-only review guidance\n")).expect("write package manifest");
    package
}

/// Prepare one package using the real inspected authorization identity; returns its actual lease.
/// 使用真实检查的授权身份准备插件包；返回其实际租约。
fn prepare(store: &VulcanPluginGenerationStore, source: &Path) -> PinnedVulcanPluginGeneration {
    // The expected digest comes from the same production inspection path used by the host.
    // 预期摘要来自宿主使用的相同生产检查路径。
    let inspected = crate::inspect_user_plugin_package(source).expect("inspect generation fixture");
    store
        .prepare(source, &inspected.content_fingerprint)
        .expect("prepare real generation")
}

/// Activation switches only selection; real pins and persisted tombstones preserve lifecycle ownership.
/// 激活仅切换选择；实际固定句柄及持久停用记录保留生命周期所有权。
#[test]
fn immutable_generations_keep_original_paths_and_real_clone_ownership() {
    // Own both source packages and the separately rooted generation store.
    // 持有两个来源插件包及独立根目录的代次存储。
    let temp = tempfile::tempdir().expect("create immutable generation fixture");
    // Explicit store initialization uses an existing host-selected root.
    // 显式存储初始化使用现有宿主选定根目录。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open generation store");
    // First and second sources have one plugin identity but different content fingerprints.
    // 两个来源具有相同插件身份，但内容指纹不同。
    let first_source = write_package(
        &temp.path().join("first"),
        "sample-plugin",
        "first generation",
    );
    // The second source must never overwrite bytes used by the first pinned generation.
    // 第二来源绝不能覆盖第一固定代次使用的字节。
    let second_source = write_package(
        &temp.path().join("second"),
        "sample-plugin",
        "second generation",
    );
    // Retain a cloned real lease after dropping the first handle.
    // 第一固定句柄丢弃后，继续保留克隆实际租约。
    let first = prepare(&store, &first_source);
    // Identity and path copies alone do not own a lease.
    // 仅身份及路径副本不拥有租约。
    let first_identity = first.identity().clone();
    // Preserve the immutable original path for post-switch and post-collection assertions.
    // 保存不可变原路径，用于切换及回收后的断言。
    let first_path = first.package_root().to_owned();
    // A real handle clone retains shared filesystem ownership.
    // 实际句柄克隆保留共享文件系统所有权。
    let first_clone = first.clone();
    assert!(
        store
            .activation("sample-plugin")
            .expect("read empty selection")
            .is_none()
    );
    assert_eq!(
        store
            .activate_for_test(&first, 0, first.required_permissions.clone())
            .expect("activate first generation")
            .revision,
        1
    );
    // Prepared second content does not alter the existing selection.
    // 已准备的第二份内容不改变既有选择。
    let second = prepare(&store, &second_source);
    assert_eq!(
        store
            .activation("sample-plugin")
            .expect("read original selection")
            .expect("original selection")
            .generation(),
        Some(&first_identity)
    );
    assert_ne!(first_path, second.package_root());
    assert_eq!(
        store
            .collect(&first_identity)
            .expect_err("selected generation is retained")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        store
            .activate_for_test(&second, 1, second.required_permissions.clone())
            .expect("activate second generation")
            .revision,
        2
    );
    assert!(
        fs::read_to_string(first_path.join("skills/review-helper/SKILL.md"))
            .expect("read original pinned bytes")
            .contains("first generation")
    );
    drop(first);
    assert_eq!(
        store
            .collect(&first_identity)
            .expect_err("clone still owns real lease")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(first_clone);
    assert!(
        store
            .collect(&first_identity)
            .expect("collect released old generation")
    );
    assert!(!first_path.exists());
    assert_eq!(
        store
            .generation_phase(&first_identity)
            .expect("read collected generation phase"),
        VulcanPluginGenerationPhase::Collected
    );
    assert!(
        !store
            .collect(&first_identity)
            .expect("observe completed collection")
    );
    assert_eq!(
        store
            .pin(&first_identity)
            .expect_err("collected generation cannot reopen")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );

    // Reopening the store reads persisted selection rather than reconstructing it from available directories.
    // 重新打开存储读取持久选择，不从现存目录重构选择。
    let reopened = VulcanPluginGenerationStore::open(temp.path()).expect("reopen store");
    // This snapshot acquires a second independent real lease on the selected generation.
    // 此快照在选定代次上取得第二个独立实际租约。
    let selected = reopened
        .pin_active("sample-plugin")
        .expect("pin persisted activation");
    assert_eq!(
        selected
            .generation
            .as_ref()
            .expect("selected package")
            .identity(),
        second.identity()
    );
    assert_eq!(
        reopened
            .deactivate("sample-plugin", 2)
            .expect("write deactivation tombstone")
            .revision,
        3
    );
    assert_eq!(
        reopened
            .activate_for_test(&second, 0, second.required_permissions.clone())
            .expect_err("revision zero cannot be reused")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(
        reopened
            .pin_active("sample-plugin")
            .expect("read tombstone")
            .generation
            .is_none()
    );
    assert_eq!(
        reopened
            .collect(second.identity())
            .expect_err("active snapshot still owns bytes")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    // Copy only the identity before releasing the final selected package owners.
    // 释放最终选定插件包所有者前，仅复制身份。
    let second_identity = second.identity().clone();
    drop(second);
    drop(selected);
    assert!(
        reopened
            .collect(&second_identity)
            .expect("collect fully released selection")
    );
}

/// Independent managers cannot publish two successors for one revision, while another plugin remains available.
/// 独立管理器不能为同一修订发布两个后继，且其他插件仍可用。
#[test]
fn competing_store_managers_use_real_namespace_locks_and_revision_cas() {
    // Independent store objects share only persisted files and actual OS locks.
    // 独立存储对象仅共享持久文件及实际操作系统锁。
    let temp = tempfile::tempdir().expect("create concurrent store fixture");
    // Construct two independent store owners for the same exact root.
    // 为相同精确根目录构造两个独立存储所有者。
    let first_store = VulcanPluginGenerationStore::open(temp.path()).expect("open first store");
    // This owner has no in-memory synchronization object in common with the first owner.
    // 此所有者与第一所有者不共用任何内存同步对象。
    let second_store = VulcanPluginGenerationStore::open(temp.path()).expect("open second store");
    // Prepare independent same-plugin candidates before racing their activation.
    // 竞争激活前准备独立同插件候选。
    let first = prepare(
        &first_store,
        &write_package(&temp.path().join("first"), "sample-plugin", "first"),
    );
    // The other candidate owns a distinct immutable directory and lease.
    // 另一候选拥有不同的不可变目录及租约。
    let second = prepare(
        &second_store,
        &write_package(&temp.path().join("second"), "sample-plugin", "second"),
    );
    // Credential inventories are staged up front so the race covers only selection publication.
    // 凭据清单预先暂存，使竞争只覆盖选择发布。
    let first_credentials = first_store.stage_unconfigured_credentials_for_test(&first);
    let second_credentials = second_store.stage_unconfigured_credentials_for_test(&second);
    // Start both explicit CAS operations from one barrier.
    // 从同一屏障启动两个显式 CAS 操作。
    let barrier = Arc::new(Barrier::new(2));
    // Retain outcomes from actual concurrent threads rather than emulating an outdated revision.
    // 保留真实并发线程结果，而非模拟过期修订。
    let outcomes = std::thread::scope(|scope| {
        // This first actual publisher owns no global runtime registration lock.
        // 此实际首个发布者不持有运行时全局注册锁。
        let first_thread = scope.spawn(|| {
            barrier.wait();
            first_store.activate_with_credentials_for_test(
                &first,
                0,
                first.required_permissions.clone(),
                first_credentials,
            )
        });
        // This second publisher competes through the filesystem namespace lock.
        // 此第二发布者经文件系统命名空间锁竞争。
        let second_thread = scope.spawn(|| {
            barrier.wait();
            second_store.activate_with_credentials_for_test(
                &second,
                0,
                second.required_permissions.clone(),
                second_credentials,
            )
        });
        [
            first_thread.join().expect("join first publisher"),
            second_thread.join().expect("join second publisher"),
        ]
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert!(
        outcomes
            .iter()
            .filter_map(|outcome| outcome.as_ref().err())
            .all(|error| matches!(
                error.code,
                VulcanPluginStoreErrorCode::Busy | VulcanPluginStoreErrorCode::Conflict
            ))
    );
    assert_eq!(
        first_store
            .activation("sample-plugin")
            .expect("read raced activation")
            .expect("one activation")
            .revision,
        1
    );
    assert_eq!(
        second_store
            .activate_for_test(&second, 0, second.required_permissions.clone())
            .expect_err("stale predecessor stays rejected")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(first.package_root().is_dir() && second.package_root().is_dir());

    // Hold one plugin's actual namespace lock while activating a different plugin.
    // 持有一个插件的实际命名空间锁，同时激活另一个插件。
    let _held_namespace = files::lock(&first_store.root.join("locks/sample-plugin.lock"), false)
        .expect("hold first namespace");
    assert_eq!(
        second_store
            .activation("sample-plugin")
            .expect_err("first namespace is busy")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    // The unrelated plugin must not queue behind the held namespace.
    // 无关插件不得在所持命名空间之后排队。
    let unrelated = prepare(
        &second_store,
        &write_package(&temp.path().join("other"), "other-plugin", "independent"),
    );
    assert_eq!(
        second_store
            .activate_for_test(&unrelated, 0, unrelated.required_permissions.clone())
            .expect("publish unrelated namespace")
            .revision,
        1
    );
}

/// Corrupt metadata, moved ownership, and changed bytes never redirect activation or erase evidence.
/// 元数据损坏、所有权错配及字节变化从不重定向激活或擦除证据。
#[test]
fn generation_validation_rejects_tampering_and_retains_exact_failure_bytes() {
    // All failure cases operate on real files inside one isolated store.
    // 所有失败情况均操作同一隔离存储内的真实文件。
    let temp = tempfile::tempdir().expect("create invalid generation fixture");
    // Open the sole authoritative store root.
    // 打开唯一权威存储根目录。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open store");
    // Construct and inspect a valid source before testing a stale authorization digest.
    // 测试过期授权摘要前构造并检查有效来源。
    let source = write_package(&temp.path().join("source"), "sample-plugin", "original");
    assert_eq!(
        store
            .prepare(&source, &"0".repeat(64))
            .expect_err("reject stale inspection")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );
    // The valid handle keeps its original identity even when the files are later corrupted.
    // 文件稍后被损坏时，有效句柄仍保持原身份。
    let generation = prepare(&store, &source);
    // Credentials are staged before tampering so only package verification meets the change.
    // 篡改前暂存凭据，使只有插件包校验遇到变化。
    let credentials = store.stage_unconfigured_credentials_for_test(&generation);
    fs::write(generation.package_root().join("foreign.txt"), b"changed")
        .expect("change prepared content");
    assert_eq!(
        store
            .activate_with_credentials_for_test(
                &generation,
                0,
                generation.required_permissions.clone(),
                credentials.clone(),
            )
            .expect_err("reject changed package")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );
    assert!(
        store
            .activation("sample-plugin")
            .expect("read unchanged activation")
            .is_none()
    );
    fs::remove_file(generation.package_root().join("foreign.txt"))
        .expect("restore exact fixture bytes");
    store
        .activate_with_credentials_for_test(
            &generation,
            0,
            generation.required_permissions.clone(),
            credentials,
        )
        .expect("activate verified generation");
    // Caller-supplied identities cannot select an unchecked traversal path.
    // 调用方提供的身份不能选择未经检查的遍历路径。
    let mut invalid_identity = generation.identity().clone();
    invalid_identity.generation_id = "../outside".into();
    assert_eq!(
        store
            .pin(&invalid_identity)
            .expect_err("reject path traversal identity")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );
    // A foreign store rejects a real handle even when the underlying package identity is valid.
    // 即使底层插件身份有效，其他存储也拒绝实际句柄。
    let other_root = tempfile::tempdir().expect("create unrelated store root");
    // Reusing the generation's root would violate explicit store ownership.
    // 复用该代次根目录将违反显式存储所有权。
    let other_store =
        VulcanPluginGenerationStore::open(other_root.path()).expect("open unrelated store");
    assert_eq!(
        other_store
            .activate_with_credentials_for_test(
                &generation,
                0,
                generation.required_permissions.clone(),
                VulcanPluginCredentialRevision::allocate(),
            )
            .expect_err("reject cross-store handle")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );

    // Keep the original record bytes for independent corruption cases.
    // 保留原始记录字节，用于相互独立的损坏情况。
    let activation_path = store.root.join("active/sample-plugin.json");
    // Mutations below are external corruption, not normal store writes.
    // 下述变更为外部损坏，不是正常存储写入。
    let original = fs::read(&activation_path).expect("read original activation bytes");
    // Build invalid records with valid JSON so semantic validation must reject them.
    // 构造合法 JSON 的无效记录，使语义校验必须拒绝。
    let base: serde_json::Value =
        serde_json::from_slice(&original).expect("decode activation fixture");
    for field in ["schema_version", "plugin_id", "revision", "package"] {
        // Every corruption starts from the same original record rather than a previous damaged record.
        // 每次损坏从相同原始记录开始，而非沿用前次损坏记录。
        let mut corrupted = base.clone();
        if field == "package" {
            corrupted
                .as_object_mut()
                .expect("activation object")
                .remove(field);
        } else {
            corrupted[field] = match field {
                "schema_version" => serde_json::json!(99),
                "plugin_id" => serde_json::json!("other-plugin"),
                "revision" => serde_json::json!(0),
                _ => unreachable!(),
            };
        }
        // Preserve the exact bytes used to demonstrate read-only rejection.
        // 保留用于证明只读拒绝的精确字节。
        let bytes = serde_json::to_vec(&corrupted).expect("encode corrupt activation");
        fs::write(&activation_path, &bytes).expect("write corrupt activation");
        assert_eq!(
            store
                .pin_active("sample-plugin")
                .expect_err("reject corrupt selection")
                .code,
            VulcanPluginStoreErrorCode::Invalid
        );
        assert_eq!(
            fs::read(&activation_path).expect("read rejected bytes"),
            bytes
        );
    }
    // Revision exhaustion is explicit and cannot wrap back to an uninitialized namespace.
    // 修订耗尽明确暴露，不能回绕至未初始化命名空间。
    let mut exhausted = base;
    exhausted["revision"] = serde_json::json!(u64::MAX);
    fs::write(
        &activation_path,
        serde_json::to_vec(&exhausted).expect("encode exhausted activation"),
    )
    .expect("write exhausted activation");
    assert_eq!(
        store
            .deactivate("sample-plugin", u64::MAX)
            .expect_err("reject revision overflow")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );
    fs::write(&activation_path, &original).expect("restore original activation");
    assert_eq!(
        store
            .activation("sample-plugin")
            .expect("read restored activation")
            .expect("original record")
            .revision,
        1
    );
}

/// Reopening an interrupted preparation cannot publish it, but exact identity cleanup remains possible.
/// 重新打开被中断准备时不能发布它，但仍可按精确身份清理。
#[test]
fn incomplete_generation_intent_is_not_ready_and_can_be_explicitly_collected() {
    // Build a real valid identity and retain only disk state for the restarted store.
    // 构造真实有效身份，并仅为重开存储保留磁盘状态。
    let temp = tempfile::tempdir().expect("create incomplete generation fixture");
    // The first owner produces the same immutable identity used by the interrupted preparation fixture.
    // 第一所有者产生被中断准备夹具使用的相同不可变身份。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open incomplete store");
    // Prepare once to obtain an independently validated content identity and actual copied files.
    // 准备一次，以取得独立校验的内容身份及实际复制文件。
    let generation = prepare(
        &store,
        &write_package(&temp.path().join("source"), "sample-plugin", "partial"),
    );
    // Freeze the exact identity before discarding all actual package leases.
    // 丢弃全部实际插件包租约前固定精确身份。
    let identity = generation.identity().clone();
    // Construct the documented on-disk boundary after intent persistence but before copying finishes.
    // 构造已记录的磁盘边界：意图已持久保存，但复制尚未完成。
    let generation_root = generation
        .package_root()
        .parent()
        .expect("generation parent")
        .to_owned();
    fs::remove_file(generation.package_root().join("plugin.yaml"))
        .expect("leave incomplete copied package");
    files::write_record(
        &generation_root.join("generation.json"),
        &GenerationRecord {
            schema_version: RECORD_VERSION,
            identity: identity.clone(),
            state: GenerationState::Preparing,
        },
    )
    .expect("persist interrupted preparation boundary");
    drop(generation);
    drop(store);
    // This is an independent reopen of exact disk state, not a power-loss or injected crash proof.
    // 此为精确磁盘状态的独立重开，不是断电或注入崩溃证明。
    let reopened =
        VulcanPluginGenerationStore::open(temp.path()).expect("reopen interrupted preparation");
    assert_eq!(
        reopened
            .generation_phase(&identity)
            .expect("read interrupted preparation phase"),
        VulcanPluginGenerationPhase::Preparing
    );
    assert_eq!(
        reopened
            .pin(&identity)
            .expect_err("incomplete intent cannot become ready")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );
    assert!(
        reopened
            .activation("sample-plugin")
            .expect("read unchanged namespace")
            .is_none()
    );
    assert!(
        reopened
            .collect(&identity)
            .expect("explicitly collect exact incomplete candidate")
    );
    assert!(!generation_root.join("sample-plugin").exists());
    assert!(
        !reopened
            .collect(&identity)
            .expect("observe collected identity")
    );
}

/// Own and join the real child even when an assertion fails, preventing fixture cleanup from racing its leases.
/// 即使断言失败也持有并等待真实子进程，避免夹具清理与其租约竞争。
struct LeaseChild(Child);

impl Drop for LeaseChild {
    /// Stop and join an unfinished child on scope exit; the underlying filesystem lease ends with the process.
    /// 作用域退出时停止并等待未结束子进程；底层文件系统租约随进程结束。
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// A real second process pins package bytes after selection changes and blocks collection until actual exit.
/// 真实第二进程在选择变化后仍固定插件字节，并阻止回收直至实际退出。
#[test]
fn generation_pins_survive_selection_changes_across_real_processes() {
    // Own the store and process-control files independently of package contents.
    // 独立于插件包内容持有存储及进程控制文件。
    let temp = tempfile::tempdir().expect("create process lease fixture");
    // The parent and child open independent store objects for this root.
    // 父子进程为此根目录打开独立存储对象。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open parent store");
    // Freeze one complete generation for the actual child to acquire.
    // 固定一个完整代次，供真实子进程取得。
    let generation = prepare(
        &store,
        &write_package(
            &temp.path().join("source"),
            "sample-plugin",
            "process pinned",
        ),
    );
    // An identity copy does not itself retain ownership after the parent drops its handle.
    // 父进程丢弃句柄后，身份副本自身不保留所有权。
    let identity = generation.identity().clone();
    store
        .activate_for_test(&generation, 0, generation.required_permissions.clone())
        .expect("activate process fixture");
    // The test helper acquires an actual public pin before writing its readiness marker.
    // 测试辅助入口在写入就绪标记前取得实际公开固定句柄。
    let mut child = LeaseChild(
        Command::new(std::env::current_exe().expect("resolve test binary"))
            .args([
                "--exact",
                "generation_store::tests::generation_lease_process_helper",
                "--ignored",
                "--test-threads=1",
            ])
            .env("VULCAN_GENERATION_STORE_TEST_ROOT", temp.path())
            .env(
                "VULCAN_GENERATION_STORE_TEST_IDENTITY",
                serde_json::to_string(&identity).expect("encode child identity"),
            )
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn actual lease owner"),
    );
    // Bound readiness while verifying the process remains actually alive.
    // 在验证进程实际存活的同时限制就绪等待。
    let deadline = Instant::now() + Duration::from_secs(10);
    while !temp.path().join("lease-ready").exists() {
        assert!(
            child
                .0
                .try_wait()
                .expect("observe child liveness")
                .is_none(),
            "lease helper exited before acquiring its pin"
        );
        assert!(
            Instant::now() < deadline,
            "lease helper did not become ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    store
        .deactivate("sample-plugin", 1)
        .expect("deactivate while child retains old bytes");
    drop(generation);
    assert_eq!(
        store
            .collect(&identity)
            .expect_err("real child still owns generation")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert!(
        child
            .0
            .try_wait()
            .expect("confirm blocked collection owner")
            .is_none()
    );
    fs::write(temp.path().join("release-lease"), b"release").expect("release child pin");
    assert!(child.0.wait().expect("join actual child").success());
    assert!(
        store
            .collect(&identity)
            .expect("collect after real process exit")
    );
}

/// Child-only entry; acquires a public pin and retains it until the parent releases the test barrier.
/// 仅供子进程使用的入口；取得公开固定句柄并保持至父进程释放测试屏障。
#[test]
#[ignore = "started by generation_pins_survive_selection_changes_across_real_processes"]
fn generation_lease_process_helper() {
    // These exact test-only values are supplied by the spawning parent.
    // 这些精确测试专用值由启动父进程提供。
    let root = PathBuf::from(
        std::env::var_os("VULCAN_GENERATION_STORE_TEST_ROOT").expect("child test root"),
    );
    // Decode the same complete identity the parent actually prepared.
    // 解码父进程实际准备的相同完整身份。
    let identity: VulcanPluginGenerationIdentity = serde_json::from_str(
        &std::env::var("VULCAN_GENERATION_STORE_TEST_IDENTITY").expect("child identity"),
    )
    .expect("decode child identity");
    // Independent construction prevents shared Rust memory from providing the cross-process exclusion.
    // 独立构造排除共享 Rust 内存提供跨进程互斥的可能。
    let store = VulcanPluginGenerationStore::open(&root).expect("open child store");
    // This actual filesystem lease must remain live until the barrier is released.
    // 此实际文件系统租约必须保持至屏障释放。
    let generation = store.pin(&identity).expect("acquire child generation pin");
    fs::write(root.join("lease-ready"), b"pinned").expect("publish actual child readiness");
    // A bounded child timeout ensures a failed parent cannot strand this test process indefinitely.
    // 有界子进程超时确保父进程失败不会使测试进程永久滞留。
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.join("release-lease").exists() {
        assert!(
            Instant::now() < deadline,
            "parent did not release child pin"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(generation.package_root().is_dir());
    drop(generation);
}

/// Windows sharing denial leaves a durable collection intent and permits an explicit retry after the handle closes.
/// Windows 共享拒绝留下持久清理意图，并允许句柄关闭后显式重试。
#[cfg(windows)]
#[test]
fn occupied_windows_package_keeps_cleanup_identity_until_actual_handle_release() {
    use std::os::windows::fs::OpenOptionsExt;
    // Isolate a real Windows handle that denies deletion of one package file.
    // 隔离一个拒绝删除插件文件的真实 Windows 句柄。
    let temp = tempfile::tempdir().expect("create occupied package fixture");
    // No VM or callback is involved in this storage-level sharing violation.
    // 此存储层共享冲突不涉及 VM 或回调。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open occupied store");
    // Prepare complete bytes before opening the independent deletion-denying handle.
    // 打开独立拒绝删除句柄前准备完整字节。
    let generation = prepare(
        &store,
        &write_package(&temp.path().join("source"), "sample-plugin", "occupied"),
    );
    // Keep the complete identity after dropping the store's shared generation lease.
    // 丢弃存储共享代次租约后仍保留完整身份。
    let identity = generation.identity().clone();
    // FILE_SHARE_READ | FILE_SHARE_WRITE excludes FILE_SHARE_DELETE deliberately.
    // FILE_SHARE_READ | FILE_SHARE_WRITE 刻意排除 FILE_SHARE_DELETE。
    let occupied = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(generation.package_root().join("plugin.yaml"))
        .expect("open deletion-denying package handle");
    drop(generation);
    assert_eq!(
        store
            .collect(&identity)
            .expect_err("actual Windows handle blocks cleanup")
            .code,
        VulcanPluginStoreErrorCode::Storage
    );
    assert_eq!(
        store
            .generation_phase(&identity)
            .expect("read retained collection phase"),
        VulcanPluginGenerationPhase::Collecting
    );
    assert_eq!(
        store
            .pin(&identity)
            .expect_err("retiring package cannot become reusable")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );
    drop(occupied);
    assert!(
        store
            .collect(&identity)
            .expect("resume original collection intent")
    );
    assert!(!store.collect(&identity).expect("read collected tombstone"));
}
