//! Original user-root validation is read-only and the actual guard remains exclusive across thread ownership.
//! 原用户根验证保持只读，实际守卫跨线程所有权保持排他。

use super::*;
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

/// Exact fixture content distinguishes actual directories, explicitly identified physical lease Files and durable bytes.
/// 精确夹具内容区分实际目录、明确识别物理租约 File 及持久字节。
#[derive(Debug, PartialEq, Eq)]
enum Contents {
    /// Entries are represented separately without inventing directory contents.
    /// 条目单独表示，不虚构目录内容。
    Directory,
    /// One exact caller-supplied actual locked File is observed by metadata without Windows locked-byte reads.
    /// 单个精确调用方提供实际加锁 File 通过元数据观察，不读取 Windows 加锁字节。
    Lease,
    /// Every other ordinary file retains actual bytes and every read failure is explicit.
    /// 每份其他普通文件保留实际字节，每次读取失败均明确暴露。
    Durable(Vec<u8>),
}

/// Snapshot canonical `root` without writes, excluding byte reads only for the exact physical `leases` supplied by this fixture.
/// 不写入地捕获规范 `root`，仅对夹具提供精确物理 `leases` 排除字节读取。
/// Return all literal entries, bytes, modification times and lengths; unexpected redirects remain failures.
/// 返回全部字面条目、字节、修改时间及长度；意外重定向仍保持为失败。
fn snapshot(
    root: &Path,
    leases: &BTreeSet<PathBuf>,
) -> BTreeMap<PathBuf, (Contents, std::time::SystemTime, u64)> {
    // Canonical keys match the original Store's exact Windows spelling without candidate path matching.
    // 规范键匹配原 Store 精确 Windows 拼写，不进行候选路径匹配。
    let mut pending = vec![root.to_path_buf()];
    // Every actual tree entry contributes independently to no-write evidence.
    // 每个实际树条目独立贡献无写入证据。
    let mut result = BTreeMap::new();
    while let Some(path) = pending.pop() {
        // No source artifact is silently skipped on a read or metadata failure.
        // 任何来源产物均不因读取或元数据失败而静默跳过。
        let metadata = fs::symlink_metadata(&path).expect("actual fixture metadata");
        assert!(
            !crate::metadata_is_link_or_reparse_point(&metadata),
            "snapshot must not follow redirects"
        );
        // Byte-read exclusions identify actual known locked paths, not suffix guesses or arbitrary files in locks.
        // 字节读取排除识别实际已知加锁路径，而非后缀猜测或 locks 内任意文件。
        let contents = if metadata.is_dir() {
            for entry in fs::read_dir(&path).expect("actual directory entries") {
                pending.push(entry.expect("actual entry").path());
            }
            Contents::Directory
        } else if leases.contains(&path) {
            Contents::Lease
        } else {
            Contents::Durable(fs::read(&path).expect("actual durable bytes"))
        };
        result.insert(
            path,
            (
                contents,
                metadata.modified().expect("actual modified time"),
                metadata.len(),
            ),
        );
    }
    result
}

/// Existing root spelling is accepted while foreign, missing, wrong-type and redirected roots cannot create or adopt authority.
/// 接受既有根拼写，同时外来、缺失、错误类型及重定向根不能创建或接管权威。
#[test]
fn legacy_mutation_user_root_validation_is_exact_and_read_only() {
    // The original package and namespace retain real Files throughout every validation.
    // 原插件包及命名空间在每次验证期间保留真实 File。
    let root = tempfile::tempdir().expect("read-only root fixture");
    // Canonical fixture paths use the same spelling as the actual Store, including Windows verbatim prefixes.
    // 规范夹具路径使用与实际 Store 相同拼写，包括 Windows verbatim 前缀。
    let canonical = root.path().canonicalize().expect("canonical fixture");
    // Original full selection is produced by the existing real fixture writer.
    // 原完整选择由既有真实夹具写入器产生。
    let (store, package, _, original) = fixture(&canonical);
    // Literal user-root spelling is the manager's actual store input, never a guessed parent of the private root.
    // 字面用户根拼写是管理器实际存储输入，绝不是私有根的猜测父路径。
    let user = canonical.join("user");
    // This exact guard owns the original namespace while root checks must remain lock-free reads.
    // 此精确守卫持有原命名空间，同时根检查必须保持不取得锁的读取。
    let guard = store
        .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
        .expect("original root guard");
    // Only these source-defined actual namespace/package lease paths may be unreadable on Windows.
    // Windows 上仅这些源码定义实际命名空间及插件包租约路径可能无法读取。
    let leases = BTreeSet::from([
        store.root.join("locks/legacy-plugin.lock"),
        store.root.join("locks").join(format!(
            "{}-{}.lease",
            package.identity().plugin_id,
            package.identity().generation_id
        )),
    ]);
    // A real foreign directory and ordinary file exist before the complete no-write snapshot.
    // 真实外来目录及普通文件在完整无写入快照之前存在。
    let foreign = canonical.join("foreign-user");
    fs::create_dir(&foreign).expect("foreign actual root");
    // The wrong-type input has real ordinary bytes which root validation must preserve.
    // 错误类型输入具有根验证必须保留的真实普通字节。
    let ordinary = canonical.join("ordinary-user-file");
    fs::write(&ordinary, b"unchanged file").expect("actual wrong-type file");
    // Unknown files in the actual lock directory must remain ordinary byte-observed evidence.
    // 实际锁目录内未知文件须保持为普通字节观察证据。
    fs::write(
        store.root.join("locks/unknown-evidence"),
        b"unchanged unknown bytes",
    )
    .expect("ordinary lock-directory evidence");
    // Every entry and byte is compared after validation, detecting manufactured private layouts or lock files.
    // 验证后比较每个条目及字节，检测伪造私有布局或锁文件。
    let before = snapshot(&canonical, &leases);
    guard
        .validate_user_root(&root.path().join("user"), "legacy-plugin")
        .expect("literal host spelling");
    guard
        .validate_user_root(
            &user.canonicalize().expect("actual canonical user root"),
            "legacy-plugin",
        )
        .expect("canonical host spelling");
    guard
        .validate_user_root(&user.join("."), "legacy-plugin")
        .expect("same existing lexical root");
    assert_eq!(
        guard
            .validate_user_root(&foreign, "legacy-plugin")
            .expect_err("foreign root")
            .code,
        Code::Conflict
    );
    assert_eq!(
        guard
            .validate_user_root(&canonical.join("missing-user"), "legacy-plugin")
            .expect_err("missing cannot initialize")
            .code,
        Code::Conflict
    );
    assert_eq!(
        guard
            .validate_user_root(&ordinary, "legacy-plugin")
            .expect_err("ordinary file is not root")
            .code,
        Code::Invalid
    );
    assert_eq!(
        guard
            .validate_user_root(&user, "foreign-plugin")
            .expect_err("original plugin exact")
            .code,
        Code::Conflict
    );
    assert_eq!(snapshot(&canonical, &leases), before);
    #[cfg(any(windows, unix))]
    {
        // Literal redirect rejection must preserve the actual target's complete entries and bytes.
        // 字面重定向拒绝须保留实际目标完整条目及字节。
        let target_before = snapshot(&user, &leases);
        // A literal redirected root must be rejected before canonicalization can turn it into the original actual root.
        // 字面重定向根须在规范化可将其变为原实际根之前被拒绝。
        let redirected = canonical.join("redirected-user");
        #[cfg(windows)]
        {
            // This isolated actual junction needs no symlink privilege and never touches an installed user root.
            // 此隔离实际联接不需要符号链接权限，绝不触及已安装用户根。
            let output = std::process::Command::new("cmd.exe")
                .args(["/C", "mklink", "/J"])
                .arg(&redirected)
                .arg(&user)
                .output()
                .expect("actual root junction");
            assert!(
                output.status.success(),
                "isolated root junction creation must succeed"
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&user, &redirected).expect("actual root symlink");
        assert_eq!(
            guard
                .validate_user_root(&redirected, "legacy-plugin")
                .expect_err("literal redirect cannot authorize original root")
                .code,
            Code::Invalid
        );
        assert_eq!(snapshot(&user, &leases), target_before);
        #[cfg(windows)]
        fs::remove_dir(&redirected).expect("remove only fixture junction");
        #[cfg(unix)]
        fs::remove_file(&redirected).expect("remove only fixture symlink");
        // Junction creation/removal changes only the fixture parent time; actual target entries must remain unchanged.
        // 联接创建及删除仅改变夹具父目录时间；实际目标条目须保持未变。
        assert!(
            fs::read_dir(&foreign)
                .expect("actual foreign root")
                .next()
                .is_none()
        );
        assert!(!canonical.join("missing-user").exists());
        guard
            .validate_user_root(&user, "legacy-plugin")
            .expect("same original after redirect rejection");
    }
}

/// Root equality must still enforce the complete original and actual no-pending authority, including originally absent selection.
/// 根相等仍须执行完整原请求及实际无待处理权威，包括原缺失选择。
#[test]
fn legacy_mutation_user_root_validation_rejects_changed_original_and_pending() {
    // A real first preparation writer supplies Preparing bytes before its original terminal transition.
    // 真实首准备写入器在原终态转换之前提供 Preparing 字节。
    let root = tempfile::tempdir().expect("root original authority fixture");
    // Actual original selection and source come from the canonical producer-backed fixture.
    // 实际原选择及来源来自规范生产者支持夹具。
    let (store, _package, source, original) = fixture(root.path());
    // Unique inspector output freezes the real request's approved source digest.
    // 唯一检查器输出固定真实请求的已批准来源摘要。
    let inspected = crate::inspect_user_plugin_package(&source).expect("actual source");
    // This UUID identifies the real producer request and is never used to reconstruct its owner.
    // 此 UUID 标识真实生产者请求，绝不用于重建其所有者。
    let request = Uuid::new_v4();
    // Actual preparation creates journal/index authority before the legacy guard can be acquired.
    // 实际准备在旧格式守卫可取得之前创建日志及索引权威。
    let preparation = store
        .begin_activation_preparation(
            request,
            &source,
            &inspected.content_fingerprint,
            Some(&original),
            vec!["config".into(), "network_https".into()],
        )
        .expect("actual request writer");
    // The only request journal path follows the actual writer's plugin and request arguments.
    // 唯一请求日志路径遵循实际写入器插件及请求参数。
    let journal = store
        .root
        .join("transactions/legacy-plugin")
        .join(format!("{request}.json"));
    // Preserve producer-written bytes rather than synthesize a Preparing phase.
    // 保留生产者写入字节，而非合成 Preparing 阶段。
    let preparing = fs::read(&journal).expect("true Preparing bytes");
    drop(preparation);
    store
        .abort_activation_preparation_request("legacy-plugin", request)
        .expect("true original abort");
    // Restore only these actual terminal bytes after the external-corruption fixture.
    // 外部损坏夹具之后仅恢复这些实际终态字节。
    let aborted = fs::read(&journal).expect("true terminal bytes");
    // This root is the exact literal manager input used by the real store fixture.
    // 此根为真实存储夹具所用精确字面管理器输入。
    let user = root.path().join("user");
    // The complete original no-pending state is frozen before any controlled external replacement.
    // 任何受控外部替换之前，固定完整原无待处理状态。
    let guard = store
        .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
        .expect("actual original guard");
    // Same revision with a changed full approved set must not be accepted by a root-only match.
    // 相同修订及已变完整批准集合不得通过仅根匹配被接受。
    let active_path = store.root.join("active/legacy-plugin.json");
    // Exact original selection bytes provide explicit fixture restoration with no fallback.
    // 精确原选择字节提供明确夹具恢复，不使用回退。
    let original_bytes = fs::read(&active_path).expect("actual selected bytes");
    // This controlled changed field remains well-formed selection metadata.
    // 此受控已变字段保持格式合法选择元数据。
    let mut changed = original.clone();
    changed.approved_permissions = vec!["config".into()];
    fs::write(
        &active_path,
        serde_json::to_vec(&changed).expect("changed complete approval"),
    )
    .expect("external full selection replacement");
    assert_eq!(
        guard
            .validate_user_root(&user, "legacy-plugin")
            .expect_err("full original required")
            .code,
        Code::Conflict
    );
    fs::write(&active_path, original_bytes).expect("restore actual selected bytes");
    fs::write(&journal, preparing).expect("external actual prior Preparing replacement");
    assert_eq!(
        guard
            .validate_user_root(&user, "legacy-plugin")
            .expect_err("actual Pending required to reject")
            .code,
        Code::Busy
    );
    fs::write(&journal, aborted).expect("restore actual original terminal body");
    guard
        .validate_user_root(&user, "legacy-plugin")
        .expect("same original restored");
    // Actual absent selection must also traverse full original and pending validation rather than shortcutting on None.
    // 实际缺失选择也须经过完整原请求及待处理验证，不能遇 None 即跳过。
    let absent = store
        .acquire_legacy_mutation_guard("absent-plugin", None)
        .expect("actual absent original namespace");
    absent
        .validate_user_root(&user, "absent-plugin")
        .expect("validated true original absence");
}

/// The same Arc-held guard is Send + Sync and retains its original exclusive File until the last actual thread owner retires.
/// 同一 Arc 持有守卫为 Send + Sync，并保留原排他 File 直到最后实际线程所有者退役。
#[test]
fn legacy_mutation_user_root_arc_preserves_actual_namespace_across_threads() {
    /// Require the real generic type to satisfy both auto traits at compile time; no unsafe implementation is added.
    /// 编译时要求真实泛型类型满足两个自动特征；不增加任何 unsafe 实现。
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<VulcanPluginLegacyMutationGuard>();
    assert_send_sync::<Arc<VulcanPluginLegacyMutationGuard>>();
    // Distinct store objects use the same actual canonical OS lock files.
    // 不同存储对象使用相同实际规范操作系统锁文件。
    let root = tempfile::tempdir().expect("cross-thread original namespace fixture");
    // This full original remains stable across both actual thread owners.
    // 此完整原请求跨两个实际线程所有者保持稳定。
    let (store, _package, _, original) = fixture(root.path());
    // The worker uses the actual host user input, not a reconstructed private-root parent.
    // 工作线程使用实际宿主用户输入，而非重建私有根父路径。
    let user = root.path().join("user");
    // An independent real Store competes without a shared process mutex.
    // 独立真实 Store 在没有共享进程互斥锁情况下竞争。
    let observer = VulcanPluginGenerationStore::open_existing(&user)
        .expect("independent observer")
        .expect("existing store");
    // Arc shares one real guard allocation and cannot duplicate the underlying namespace File.
    // Arc 共享一个真实守卫分配，不能复制底层命名空间 File。
    let guard = Arc::new(
        store
            .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
            .expect("original File owner"),
    );
    // The worker receives a reference-counted owner of the same actual guard.
    // 工作线程接收同一实际守卫的引用计数所有者。
    let worker_guard = Arc::clone(&guard);
    // Channels order actual ownership and retirement deterministically; no sleep substitutes for a live-owner proof.
    // 通道确定性排序实际所有权及退役；不以 sleep 替代存活所有者证明。
    let (ready_tx, ready_rx) = mpsc::channel();
    // The last worker must retain its Arc until explicitly released after the competing Busy observations.
    // 最后工作线程须保留其 Arc，直到竞争 Busy 观察之后明确释放。
    let (release_tx, release_rx) = mpsc::channel();
    // Worker state contains only immutable original authority and real independent observer paths.
    // 工作线程状态仅包含不可变原权威及真实独立观察者路径。
    let worker_store = observer.clone();
    // Both root inputs identify the same real canonical directory.
    // 两个根输入标识相同真实规范目录。
    let worker_user = user.clone();
    // Worker-side typed validation and pins must succeed without reacquiring the self-busy namespace.
    // 工作线程类型化验证及固定须成功，不能重新取得自身繁忙命名空间。
    let worker = std::thread::spawn(move || {
        worker_guard
            .validate_user_root(&worker_user, "legacy-plugin")
            .expect("cross-thread original validation");
        drop(
            worker_guard
                .pin_active_for_catalog(&worker_store, "legacy-plugin")
                .expect("cross-thread original pins"),
        );
        ready_tx.send(()).expect("actual worker ownership ready");
        release_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("explicit actual owner release");
        drop(worker_guard);
    });
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("actual worker reached owned checkpoint");
    drop(guard);
    assert_eq!(
        observer
            .pin_active("legacy-plugin")
            .expect_err("remaining thread owns original File")
            .code,
        Code::Busy
    );
    assert_eq!(
        observer
            .deactivate("legacy-plugin", original.revision)
            .expect_err("main Arc retirement is not final File retirement")
            .code,
        Code::Busy
    );
    release_tx.send(()).expect("release last actual Arc owner");
    worker.join().expect("actual worker guard retired");
    observer
        .deactivate("legacy-plugin", original.revision)
        .expect("writer after last real guard owner retired");
}
