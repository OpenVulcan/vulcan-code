//! Actual mkdir/remove/marker successes and physical owners verify original-bound package cleanup.
//! 实际 mkdir、删除、标记成功及物理所有者验证原请求绑定插件包清理。

use super::*;
use crate::generation_store::{
    VulcanPluginGenerationPhase as GenerationPhase,
    VulcanPluginPreparationPackageCleanupObservation as Observation,
};
use std::collections::BTreeMap;

/// Actual snapshot contents distinguish directories and known lock-directory metadata from durable bytes.
/// 实际快照内容区分目录、已知锁目录元数据及持久字节。
#[derive(Debug, PartialEq, Eq)]
enum Contents {
    /// Actual directory whose entries are recorded separately.
    /// 实际目录，其条目单独记录。
    Directory,
    /// Actual canonical fixture lease in one of the four source-defined lock layouts.
    /// 四种源码定义锁布局之一的实际规范夹具租约。
    Lease,
    /// Exact actual durable bytes; every read error remains a test failure.
    /// 精确实际持久字节；每个读取错误均保持为测试失败。
    Durable(Vec<u8>),
}

/// Recognize only canonical non-nil identities in this fixture's four source-defined lease layouts.
/// 仅识别此夹具四种源码定义租约布局中的规范非空身份。
/// Return true only for the exact namespace or canonical request/generation/credential lease under its real lock directory.
/// 仅对实际锁目录内精确命名空间或规范请求、代际、凭据租约返回真。
fn is_fixture_lease(path: &Path, lock_directory: &Path) -> bool {
    if path.parent() != Some(lock_directory) {
        return false;
    }
    // The fixture uses the actual sample-plugin namespace declared by the production fixture writers.
    // 夹具使用生产夹具写入器声明的实际 sample-plugin 命名空间。
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("fixture UTF-8 filename");
    if name == "sample-plugin.lock" {
        return true;
    }
    // Transaction, credential and generation operations define these three exact UUID-bearing layouts.
    // 事务、凭据及代际操作定义这三种精确含 UUID 布局。
    for (prefix, suffix) in [
        ("sample-plugin.transaction.", ".lock"),
        ("sample-plugin-credential-", ".lease"),
        ("sample-plugin-", ".lease"),
    ] {
        if let Some(identity) = name
            .strip_prefix(prefix)
            .and_then(|name| name.strip_suffix(suffix))
            && let Ok(identity_uuid) = Uuid::parse_str(identity)
            && !identity_uuid.is_nil()
            && identity_uuid.to_string() == identity
        {
            return true;
        }
    }
    false
}

/// Snapshot all actual fixture entries, times and lengths without attempting to read Windows-locked lease byte ranges.
/// 捕获全部实际夹具条目、时间及长度，不尝试读取 Windows 加锁租约字节范围。
/// Return exact durable bytes for every file except the four canonical fixture lease layouts, including unknown lock-directory files.
/// 除四种规范夹具租约布局之外，每份文件均返回精确持久字节，包括未知锁目录文件。
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (Contents, std::time::SystemTime, u64)> {
    // Namespace, generation, credential and request Files all have one actual lease-only parent.
    // 命名空间、代际、凭据及请求 File 均具有唯一实际仅租约父目录。
    let locks = root.join(STORE_DIRECTORY).join("locks");
    // Traverse only this real isolated fixture, never installed user plugins.
    // 仅遍历此真实隔离夹具，绝不遍历已安装用户插件。
    let mut pending = vec![root.to_path_buf()];
    // Stable ordering is evidence comparison, not authority selection.
    // 稳定排序用于证据比较，而非权威选择。
    let mut entries = BTreeMap::new();
    while let Some(path) = pending.pop() {
        // Capture literal metadata and fail instead of following unexpected redirected entries.
        // 捕获字面元数据，遇到意外重定向条目报错而非跟随。
        let metadata = fs::symlink_metadata(&path).expect("actual entry metadata");
        assert!(
            !crate::metadata_is_link_or_reparse_point(&metadata),
            "fixture snapshot must not follow redirected entries"
        );
        // Every durable artifact is read exactly; lock classification requires the exact parent and complete canonical filename.
        // 精确读取每份持久产物；锁分类要求精确父目录及完整规范文件名。
        let contents = if metadata.is_dir() {
            for entry in fs::read_dir(&path).expect("actual directory entries") {
                pending.push(entry.expect("actual entry").path());
            }
            Contents::Directory
        } else if is_fixture_lease(&path, &locks) {
            Contents::Lease
        } else {
            Contents::Durable(fs::read(&path).expect("actual durable bytes"))
        };
        entries.insert(
            path,
            (
                contents,
                metadata.modified().expect("actual modification time"),
                metadata.len(),
            ),
        );
    }
    entries
}

/// Resolve the exact original generation root from production identity fields, without directory enumeration or fallback.
/// 从生产身份字段解析精确原代际根，不枚举目录或使用兜底。
fn generation_root(
    store: &VulcanPluginGenerationStore,
    preparation: &VulcanPluginActivationPreparation,
) -> PathBuf {
    store
        .root
        .join("packages")
        .join(&preparation.intent().generation().plugin_id)
        .join(&preparation.intent().generation().generation_id)
}

/// Build an actual Prepared package and explicitly confirm its original Aborted phase; return the same retained preparation.
/// 构建实际 Prepared 插件包并明确确认其原 Aborted 阶段；返回同一保留准备对象。
fn collected_fixture(
    root: &Path,
) -> (
    VulcanPluginGenerationStore,
    VulcanPluginActivationPreparation,
) {
    // Full early intent and selected predecessor come from production fixture writers.
    // 完整早期意图及已选前驱来自生产夹具写入器。
    let (store, source, mut preparation) = early_fixture(root);
    drop(
        store
            .prepare_activation_package(&preparation, &source)
            .expect("actual Prepared package"),
    );
    store
        .abort_activation_preparation_retained(&mut preparation)
        .expect("actual local Aborted");
    (store, preparation)
}

/// Open a competing handle to the sole existing original lease and prove it remains physically busy.
/// 打开唯一既有原租约的竞争句柄，证明其物理仍繁忙。
fn assert_request_busy(
    store: &VulcanPluginGenerationStore,
    preparation: &VulcanPluginActivationPreparation,
) {
    // This sole production path resolver does not manufacture an original File.
    // 此唯一生产路径解析器不伪造原 File。
    let competitor = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(store.transaction_lease_path("sample-plugin", preparation.request_id()))
        .expect("existing original File");
    assert!(
        matches!(
            competitor.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ),
        "actual original request remains held"
    );
}

/// Pin actual collected metadata without Windows delete sharing; return a physical blocker against accidental replacement.
/// 不授予 Windows 删除共享地固定实际已回收元数据；返回阻挡意外替换的物理句柄。
fn block_marker(path: &Path) -> std::fs::File {
    // Existing metadata is opened only for reading; no create or truncation is permitted.
    // 既有元数据仅以读取打开；不允许创建或截断。
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1 | 2);
    }
    options.open(path).expect("pin actual collected marker")
}

/// A truly unallocated original yields DirectoryAbsent without creating any generation lease or marker.
/// 真正未分配原请求返回 DirectoryAbsent，不创建任何代际租约或标记。
#[test]
fn preparation_package_cleanup_observes_directory_absence_without_writes() {
    // No package preparation call has ever allocated this original's generation.
    // 任何插件包准备调用均未分配此原请求代际。
    let temp = tempfile::tempdir().expect("fixture");
    // The old managed predecessor remains independent of the absent original candidate.
    // 旧受管前驱独立于缺失原候选。
    let (store, _source, mut preparation) = early_fixture(temp.path());
    store
        .abort_activation_preparation_retained(&mut preparation)
        .expect("actual Aborted");
    assert!(!generation_root(&store, &preparation).exists());
    // An unknown ordinary lock-directory file must remain byte-observable despite sharing a parent with actual leases.
    // 未知锁目录普通文件虽与实际租约共享父目录，仍须可观察字节。
    let unknown = store.root.join("locks").join("unknown-evidence.lock");
    fs::write(&unknown, b"original").expect("unknown ordinary file");
    // Every key uses the same actual canonical spelling as the store, including Windows verbatim prefixes.
    // 每个键均使用与存储一致的实际规范拼写，包括 Windows verbatim 前缀。
    let snapshot_root = temp
        .path()
        .canonicalize()
        .expect("actual canonical fixture root");
    // Whole-root entry/length/time/bytes comparison detects manufactured lock files as well as marker repair.
    // 全根条目、长度、时间、字节比较检测伪造锁文件及标记修复。
    let before = snapshot(&snapshot_root);
    // Preserve the original timestamp to prove equal-length changes are detected from bytes rather than metadata drift.
    // 保留原时间戳，证明等长变化通过字节而非元数据漂移检测。
    let modified = before.get(&unknown).expect("unknown file entry").1;
    fs::write(&unknown, b"modified").expect("same-length changed bytes");
    fs::File::options()
        .write(true)
        .open(&unknown)
        .expect("actual ordinary file")
        .set_times(fs::FileTimes::new().set_modified(modified))
        .expect("restore actual modification time");
    // Exact snapshot bytes must expose both equal-length and different-length mutations.
    // 精确快照字节须暴露等长及不同长度修改。
    let changed = snapshot(&snapshot_root);
    assert_eq!(
        changed.get(&unknown).expect("changed ordinary file").0,
        Contents::Durable(b"modified".to_vec())
    );
    assert_ne!(changed, before);
    fs::write(&unknown, b"different length").expect("changed length");
    assert_ne!(snapshot(&snapshot_root), before);
    fs::write(&unknown, b"original").expect("restore original bytes");
    fs::File::options()
        .write(true)
        .open(&unknown)
        .expect("actual restored ordinary file")
        .set_times(fs::FileTimes::new().set_modified(modified))
        .expect("restore actual original modification time");
    assert_eq!(snapshot(&snapshot_root), before);
    assert_eq!(
        store
            .collect_activation_preparation_package_retained(&preparation)
            .expect("true directory absence"),
        Observation::DirectoryAbsent
    );
    assert_eq!(snapshot(&snapshot_root), before);
    assert_request_busy(&store, &preparation);
}

/// Actual mkdir success before the first marker creates only an unmarked absent-child observation, preserving unknown siblings.
/// 首标记前实际 mkdir 成功仅产生无标记且子目录缺失观察，保留未知同级条目。
#[test]
fn preparation_package_cleanup_observes_true_unmarked_mkdir_without_repair() {
    // Faults observe the existing real allocator after success, never delete a successful marker to synthesize absence.
    // 故障在成功后观察既有真实分配器，绝不删除成功标记来合成缺失。
    let temp = tempfile::tempdir().expect("fixture");
    // The actual original lease survives allocator acknowledgement loss.
    // 实际原租约跨分配器确认丢失存续。
    let (store, source, mut preparation) = early_fixture(temp.path());
    // Full generation identity binds the actual post-mkdir observer.
    // 完整代际身份绑定实际 mkdir 后观察器。
    let identity = preparation.intent().generation().clone();
    // The shared allocator hook runs before the first marker is attempted.
    // 共享分配器钩子在尝试首标记之前运行。
    let _fault = store.observe_package_preparation_for_test(
        &identity,
        GenerationPhase::Preparing,
        Some(false),
    );
    assert_eq!(
        store
            .prepare_activation_package(&preparation, &source)
            .expect_err("true mkdir acknowledgement loss")
            .code,
        Code::Storage
    );
    assert_eq!(
        store.package_preparation_checkpoint_counts_for_test(&identity),
        (1, 0)
    );
    // Root exists because of the true allocator success; the child and first marker never existed.
    // 根目录因真实分配器成功而存在；子目录及首标记从未存在。
    let root = generation_root(&store, &preparation);
    assert!(root.is_dir());
    assert!(!root.join("generation.json").exists());
    assert!(!root.join("sample-plugin").exists());
    fs::write(root.join("unknown-evidence"), b"preserve unknown sibling")
        .expect("actual unknown sibling");
    store
        .abort_activation_preparation_retained(&mut preparation)
        .expect("actual Aborted");
    // Every existing entry must remain intact, including unowned evidence and the real lease directory.
    // 每个既有条目均须保持完整，包括无主证据及真实租约目录。
    let before = snapshot(temp.path());
    assert_eq!(
        store
            .collect_activation_preparation_package_retained(&preparation)
            .expect("true unmarked absence"),
        Observation::UnmarkedPackageAbsent
    );
    assert!(
        store.collect(&identity).is_err(),
        "ordinary collect still requires its valid marker"
    );
    assert_eq!(snapshot(temp.path()), before);
}

/// An actual unmarked directory containing unproven bytes cannot acquire deletion authority from the original preparation.
/// 包含未经证明字节的实际无标记目录不能从原准备对象取得删除权。
#[test]
fn preparation_package_cleanup_rejects_unmarked_child_present() {
    // Actual allocation is stopped before its first durable marker.
    // 实际分配在首份持久标记之前停止。
    let temp = tempfile::tempdir().expect("fixture");
    // This original remains the same real retained owner across failure and explicit abort.
    // 此原请求跨失败及明确中止保持同一真实保留所有者。
    let (store, source, mut preparation) = early_fixture(temp.path());
    // The exact full original identity is observed by the real allocator hook.
    // 真实分配器钩子观察精确完整原身份。
    let identity = preparation.intent().generation().clone();
    // Only successful mkdir precedes the injected allocator error.
    // 注入分配器错误之前仅发生成功 mkdir。
    let _fault = store.observe_package_preparation_for_test(
        &identity,
        GenerationPhase::Preparing,
        Some(false),
    );
    store
        .prepare_activation_package(&preparation, &source)
        .expect_err("real premarker failure");
    // Explicit corrupt child evidence is never evidence that a successful marker was deleted.
    // 明确损坏子目录证据绝不作为成功标记被删除的证据。
    let child = generation_root(&store, &preparation).join("sample-plugin");
    fs::create_dir(&child).expect("actual unproven child");
    fs::write(child.join("unknown"), b"unproven bytes").expect("actual unproven content");
    store
        .abort_activation_preparation_retained(&mut preparation)
        .expect("actual Aborted");
    // Rejection must retain all unproven bytes and actual owner metadata.
    // 拒绝须保留全部未经证明字节及实际所有者元数据。
    let before = snapshot(temp.path());
    assert_eq!(
        store
            .collect_activation_preparation_package_retained(&preparation)
            .expect_err("unmarked child is not delete authority")
            .message,
        "Unmarked original generation still contains unproven package bytes."
    );
    assert_eq!(snapshot(temp.path()), before);
}

/// True remove/final-marker acknowledgement loss and panic retry through the same original without repeating deletion.
/// 真实删除、最终标记确认丢失及 panic 通过同一原请求重试，不重复删除。
#[test]
fn preparation_package_cleanup_retries_actual_remove_and_collected_writer_loss() {
    for phase in [GenerationPhase::Collecting, GenerationPhase::Collected] {
        for panic in [false, true] {
            // Each actual write checkpoint and failure mode owns an isolated real package tree.
            // 每个实际写入检查点及失败模式持有隔离真实插件包目录树。
            let temp = tempfile::tempdir().expect("fixture");
            // Package and Aborted journal were durably written by their actual production writers.
            // 插件包及 Aborted 日志由实际生产写入器持久保存。
            let (store, preparation) = collected_fixture(temp.path());
            // The exact root preserves sibling evidence beside the only removable package child.
            // 精确根在唯一可删除插件包子目录旁保留同级证据。
            let root = generation_root(&store, &preparation);
            fs::write(root.join("unknown-evidence"), b"preserve sibling").expect("actual sibling");
            // Fault binding includes actual original request, full generation and store root.
            // 故障绑定包括实际原请求、完整代际及存储根。
            let _fault = store.observe_preparation_package_collection_for_test(
                &preparation,
                phase,
                Some(panic),
            );
            // The original owner survives either returned Storage or unwind outside the worker.
            // 原所有者在工作器之外跨返回 Storage 或展开存续。
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                store.collect_activation_preparation_package_retained(&preparation)
            }));
            if panic {
                assert!(outcome.is_err());
            } else {
                assert_eq!(
                    outcome
                        .expect("returned loss")
                        .expect_err("real success lost acknowledgement")
                        .code,
                    Code::Storage
                );
            }
            assert!(
                !root.join("sample-plugin").exists(),
                "true remove succeeded before any injected loss"
            );
            assert_eq!(
                store
                    .generation_phase(preparation.intent().generation())
                    .expect("actual marker"),
                phase
            );
            assert_eq!(
                store.preparation_package_collection_counts_for_test(&preparation),
                (1, usize::from(phase == GenerationPhase::Collected))
            );
            assert_request_busy(&store, &preparation);
            assert_eq!(
                store
                    .collect_activation_preparation_package_retained(&preparation)
                    .expect("same original completion"),
                Observation::Collected
            );
            assert_eq!(
                store.preparation_package_collection_counts_for_test(&preparation),
                (1, 1)
            );
            // Collected confirmation must remain read-only even when Windows cannot replace its actual marker.
            // 即使 Windows 不能替换实际标记，Collected 确认也须保持只读。
            let _blocker = block_marker(&root.join("generation.json"));
            // Capture the complete original fixture after the last required final marker writer.
            // 在最后必需最终标记写入器之后捕获完整原夹具。
            let before = snapshot(temp.path());
            assert_eq!(
                store
                    .collect_activation_preparation_package_retained(&preparation)
                    .expect("readonly Collected readback"),
                Observation::Collected
            );
            assert_eq!(
                store.preparation_package_collection_counts_for_test(&preparation),
                (1, 1)
            );
            assert_eq!(snapshot(temp.path()), before);
            assert_eq!(
                fs::read(root.join("unknown-evidence")).expect("retained sibling"),
                b"preserve sibling"
            );
            assert!(
                !store
                    .collect(preparation.intent().generation())
                    .expect("ordinary Collected bool unchanged")
            );
        }
    }
}

/// Actual package pins prevent deletion while credential-only pins remain independent and never imply package retention.
/// 实际插件包固定阻止删除，纯凭据固定保持独立，绝不暗示插件包保留。
#[test]
fn preparation_package_cleanup_respects_actual_package_pins_not_credential_pins() {
    // Prepare real package bytes and stage empty credentials through their original-bound writer.
    // 通过原请求绑定写入器准备真实插件包字节并暂存空凭据。
    let temp = tempfile::tempdir().expect("fixture");
    // The original Prepared pin owns its actual shared generation File.
    // 原 Prepared 固定持有实际共享代际 File。
    let (store, source, mut preparation) = early_fixture(temp.path());
    // This actual package owner survives explicit abort until deliberately retired.
    // 此实际插件包所有者跨明确中止存续，直到主动退役。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("actual Prepared pin");
    // The original reserved credential revision is preserved independently of package collection.
    // 原预留凭据修订独立于插件包收集保留。
    let revision = preparation.intent().credential_revision().clone();
    // Real staging retains the same original request until its writer exits.
    // 真实暂存保留同一原请求，直到其写入者退出。
    let mut writer = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("actual writer");
    writer
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("actual staged credentials");
    drop(writer);
    // Credential reader retains only its actual revision File, not the package pin or original request File.
    // 凭据读取者仅保留实际修订 File，不保留插件包固定或原请求 File。
    let credentials = store
        .pin_credentials(&package, &revision)
        .expect("actual credential-only reader");
    store
        .abort_activation_preparation_retained(&mut preparation)
        .expect("actual Aborted");
    assert_eq!(
        store
            .collect_activation_preparation_package_retained(&preparation)
            .expect_err("actual generation pin")
            .code,
        Code::Busy
    );
    drop(package);
    assert_eq!(
        store
            .collect_activation_preparation_package_retained(&preparation)
            .expect("credential File does not own package"),
        Observation::Collected
    );
    assert_eq!(credentials.record().revision(), &revision);
    assert!(
        store
            .credential_record("sample-plugin", &revision)
            .expect("credential inventory unchanged")
            .is_some()
    );
}

/// Exact metadata identity/schema and actual Collected child absence are mandatory before any delete operation.
/// 任何删除操作之前必须满足精确元数据身份、模式及实际 Collected 子目录缺失。
#[test]
fn preparation_package_cleanup_rejects_foreign_or_malformed_metadata() {
    for corruption in ["schema", "identity", "malformed", "collected-with-child"] {
        // Each corruption starts from a genuinely completed immutable package and exact local Aborted owner.
        // 每种损坏均从真实完成不可变插件包及精确本地 Aborted 所有者开始。
        let temp = tempfile::tempdir().expect("fixture");
        // No successful marker is deleted to obtain a weaker cleanup classification.
        // 不删除成功标记以取得更弱清理分类。
        let (store, preparation) = collected_fixture(temp.path());
        // Marker path comes from the actual original immutable identity.
        // 标记路径来自实际原不可变身份。
        let marker = generation_root(&store, &preparation).join("generation.json");
        // Production-written bytes are the sole source for controlled invalid replacements.
        // 生产写入字节是受控无效替代的唯一来源。
        let mut body: serde_json::Value =
            serde_json::from_slice(&fs::read(&marker).expect("actual marker"))
                .expect("actual JSON");
        match corruption {
            "schema" => body["schema_version"] = serde_json::json!(u32::MAX),
            "identity" => {
                body["identity"]["generation_id"] = serde_json::json!(Uuid::new_v4().to_string())
            }
            "collected-with-child" => body["state"] = serde_json::json!("collected"),
            "malformed" => {}
            _ => unreachable!("declared corruptions"),
        }
        fs::write(
            &marker,
            if corruption == "malformed" {
                b"not JSON".to_vec()
            } else {
                serde_json::to_vec(&body).expect("changed marker")
            },
        )
        .expect("controlled marker corruption");
        // Invalid metadata must leave complete package and foreign evidence byte-identical.
        // 无效元数据须使完整插件包及外来证据保持字节相同。
        let before = snapshot(temp.path());
        assert!(
            store
                .collect_activation_preparation_package_retained(&preparation)
                .is_err(),
            "{corruption} cannot authorize deletion"
        );
        assert_eq!(snapshot(temp.path()), before);
    }
}

/// Complete original binding, selected predecessor, sidecars and sole request ownership cannot be bypassed by cleanup.
/// 清理不能绕过完整原绑定、已选前驱、附属文件及唯一请求所有权。
#[test]
fn preparation_package_cleanup_rejects_changed_original_and_unaborted_owner() {
    for corruption in [
        "preparing",
        "foreign-store",
        "intent",
        "index",
        "selected",
        "sidecar",
        "shared-writer",
    ] {
        // Full physical candidate resources prevent rejection from being confused with missing package state.
        // 完整物理候选资源防止将拒绝与缺失插件包状态混淆。
        let temp = tempfile::tempdir().expect("fixture");
        // Actual local Preparing is retained until its explicit terminal writer is requested.
        // 实际本地 Preparing 保留到请求其明确终态写入器。
        let (store, source, mut preparation) = early_fixture(temp.path());
        drop(
            store
                .prepare_activation_package(&preparation, &source)
                .expect("actual Prepared package"),
        );
        if corruption == "shared-writer" {
            // A real credential reservation clones the original File before first inventory publication.
            // 真实凭据预留在首份清单发布之前克隆原 File。
            let package = store
                .inspect_activation_preparation_package(&preparation)
                .expect("actual pin");
            // This actual reservation survives the attempted cleanup and must not be treated as retired.
            // 此实际预留跨尝试清理存续，不得视为已退役。
            let reservation = store
                .reserve_activation_credential_staging(
                    &preparation,
                    &package,
                    &VulcanPluginCredentialValues::new(),
                )
                .expect("real shared original");
            assert_eq!(
                store
                    .collect_activation_preparation_package_retained(&preparation)
                    .expect_err("actual shared original request")
                    .code,
                Code::Busy
            );
            drop(reservation);
            continue;
        }
        if corruption != "preparing" {
            store
                .abort_activation_preparation_retained(&mut preparation)
                .expect("actual Aborted");
        }
        // Each path is derived from the original request rather than a replacement recovery identity.
        // 每个路径均派生自原请求，而非替代恢复身份。
        let journal = record_path(temp.path(), preparation.request_id());
        if corruption == "foreign-store" {
            // A separate real root cannot claim another store's retained owner.
            // 独立真实根不能声称持有另一存储保留所有者。
            let foreign_root = tempfile::tempdir().expect("foreign root");
            // Opening this foreign store confers no access to the original generation.
            // 打开此外来存储不授予原代际访问。
            let foreign =
                VulcanPluginGenerationStore::open(foreign_root.path()).expect("foreign store");
            assert_eq!(
                foreign
                    .collect_activation_preparation_package_retained(&preparation)
                    .expect_err("foreign owner")
                    .code,
                Code::Conflict
            );
            continue;
        }
        match corruption {
            "intent" => {
                // Valid alternative revision differs from the whole frozen original intent.
                // 有效替代修订与整个固定原意图不同。
                let mut body: serde_json::Value =
                    serde_json::from_slice(&fs::read(&journal).expect("actual journal"))
                        .expect("wire");
                body["preparation"]["credential_revision"] =
                    serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                        .expect("valid revision");
                fs::write(&journal, serde_json::to_vec(&body).expect("changed intent"))
                    .expect("fixture changed intent");
            }
            "index" => {
                fs::remove_file(journal.with_file_name("current")).expect("fixture missing index");
            }
            "selected" => {
                fs::remove_file(store.root.join("active/sample-plugin.json"))
                    .expect("fixture missing selected predecessor");
            }
            "sidecar" => {
                fs::write(journal.with_extension("EFFECTS"), b"{}").expect("actual sidecar alias");
            }
            "preparing" => {}
            _ => unreachable!("declared corruptions"),
        }
        // All rejecting paths preserve every original entry, byte, length and modification time.
        // 全部拒绝路径保留每个原条目、字节、长度及修改时间。
        let before = snapshot(temp.path());
        assert!(
            store
                .collect_activation_preparation_package_retained(&preparation)
                .is_err(),
            "{corruption} must reject retained cleanup"
        );
        assert_eq!(snapshot(temp.path()), before);
        assert_request_busy(&store, &preparation);
    }
}

/// Shared collection admission preserves actual selected generations and independent pending predecessor handoffs.
/// 共享回收准入保留实际已选代际及独立待完成前驱交接。
#[test]
fn preparation_package_cleanup_common_admission_respects_selection_and_handoff() {
    // Real configured predecessor and target packages supply independent actual leases and secure revisions.
    // 真实已配置前驱及目标插件包提供独立实际租约及安全修订。
    let temp = tempfile::tempdir().expect("fixture");
    // The ordinary collector must keep its original selected-generation exclusion after extraction.
    // 抽取之后，普通回收器须保留原已选代际排除。
    let (store, first, second, _first_credentials, second_credentials) = fixture(temp.path());
    assert_eq!(
        store
            .collect(first.identity())
            .expect_err("actual selected package")
            .code,
        Code::Busy
    );
    // Actual candidate request provides the complete original for independent private handoff ownership.
    // 实际候选请求为独立私有交接所有权提供完整原请求。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // Full original identity is frozen before actual Pending and Aborted writers.
    // 实际 Pending 及 Aborted 写入器之前固定完整原身份。
    let original = transaction.record().clone();
    // Private handoff owns actual old resources independently of the original activation phase.
    // 私有交接独立于原激活阶段持有实际旧资源。
    let mut handoff = store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("actual handoff owner");
    store
        .persist_predecessor_handoff_reservation(&mut handoff)
        .expect("actual Pending writer");
    store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("actual configured Aborted writer");
    // Pending predecessor evidence protects the exact old generation, independently of candidate retirement.
    // 待完成前驱证据保护精确旧代际，不依赖候选退役。
    let predecessor = first.identity().clone();
    drop(transaction);
    drop(first);
    drop(second);
    // The precise retention reason proves that selected-generation or package-pin rejection did not mask handoff admission.
    // 精确保留原因证明已选代际或包固定拒绝没有掩盖交接准入。
    let error = store
        .collect(&predecessor)
        .expect_err("actual Pending handoff retains its original predecessor");
    assert_eq!(error.code, Code::Busy);
    assert_eq!(
        error.message,
        "Package is retained by the original predecessor handoff."
    );
    assert!(
        store
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect("actual handoff record")
            .is_some()
    );
}

/// Redirected generation roots are rejected without touching any target bytes or mistaking them for missing directories.
/// 重定向代际根被拒绝，不触及任何目标字节或误当作缺失目录。
#[cfg(any(windows, unix))]
#[test]
fn preparation_package_cleanup_rejects_actual_redirected_generation_root() {
    // The original target was never physically allocated, so no valid marker is removed for this fixture.
    // 原目标从未物理分配，因此此夹具不删除任何有效标记。
    let temp = tempfile::tempdir().expect("fixture");
    // The retained local Aborted owner still binds the exact planned generation root.
    // 保留本地 Aborted 所有者仍绑定精确计划代际根。
    let (store, _source, mut preparation) = early_fixture(temp.path());
    store
        .abort_activation_preparation_retained(&mut preparation)
        .expect("actual Aborted");
    // The actual redirect points to an isolated sibling directory containing unrelated evidence.
    // 实际重定向指向包含无关证据的隔离同级目录。
    let target = temp.path().join("redirect-target");
    fs::create_dir(&target).expect("isolated redirect target");
    fs::write(target.join("evidence"), b"untouched redirected bytes").expect("target evidence");
    // Source-defined plugin namespace already exists because the genuine predecessor was allocated.
    // 源码定义插件命名空间已存在，因为真实前驱已被分配。
    let link = generation_root(&store, &preparation);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &link).expect("actual directory redirect");
    }
    #[cfg(windows)]
    {
        // Junction creation needs no symlink privilege and creates only this explicit isolated fixture link.
        // 联接创建无需符号链接权限，仅创建此明确隔离夹具链接。
        let output = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .expect("create actual fixture junction");
        assert!(
            output.status.success(),
            "actual fixture junction creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(crate::metadata_is_link_or_reparse_point(
        &fs::symlink_metadata(&link).expect("actual redirect metadata")
    ));
    assert_eq!(
        store
            .collect_activation_preparation_package_retained(&preparation)
            .expect_err("real redirected entry")
            .code,
        Code::Invalid
    );
    assert_eq!(
        fs::read(target.join("evidence")).expect("target bytes preserved"),
        b"untouched redirected bytes"
    );
    #[cfg(windows)]
    {
        fs::remove_dir(&link).expect("remove only fixture junction");
    }
    #[cfg(unix)]
    {
        fs::remove_file(&link).expect("remove only fixture symlink");
    }
}
