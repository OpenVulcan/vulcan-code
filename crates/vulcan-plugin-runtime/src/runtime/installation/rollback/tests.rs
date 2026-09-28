//! Real filesystem failures must preserve the old package and exact rollback bytes.
//! 真实文件系统失败必须保留旧插件包及精确回滚字节。

use super::{
    ROLLBACK_RECORDS_FILE, UpdateRollbackRecords, preserve_rollback_records,
    rollback_failed_preparation,
};
use std::fs;
use std::path::PathBuf;

/// Isolated package and state paths for testing production rollback operations.
/// 用于测试生产回滚操作的隔离插件包与状态路径。
struct RollbackFixture {
    /// Own all fixture files until every assertion has completed.
    /// 持有全部夹具文件，直至所有断言完成。
    _root: tempfile::TempDir,
    /// Unique private directory retaining the original package and record snapshots.
    /// 保留原始插件包及记录快照的唯一私有目录。
    transaction: PathBuf,
    /// Formal package path whose publication or restoration may fail.
    /// 发布或恢复可能失败的正式插件包路径。
    destination: PathBuf,
    /// Formal authorization record path.
    /// 正式授权记录路径。
    authorization: PathBuf,
    /// Formal installation record path.
    /// 正式安装记录路径。
    installation: PathBuf,
    /// Original record bytes, including intentionally noncanonical JSON whitespace.
    /// 包含刻意保留的非规范 JSON 空白的原始记录字节。
    records: UpdateRollbackRecords,
    /// Original complete package identity frozen before any injected filesystem conflict.
    /// 在制造文件系统冲突前固定的原始完整插件包身份。
    previous_fingerprint: String,
}

impl RollbackFixture {
    /// Create real retained package files and persist exact state snapshots; returns their owner.
    /// 创建真实保留插件文件并持久保存精确状态快照；返回其所有者。
    fn new() -> Self {
        // Keep the fixture root alive after paths are moved into the returned owner.
        // 路径移入返回所有者后，继续保持夹具根目录存活。
        let root = tempfile::tempdir().expect("create rollback fixture");
        // This directory has the same internal layout as the production update transaction.
        // 此目录的内部布局与生产更新事务一致。
        let transaction = root.path().join(".update-fixture");
        fs::create_dir_all(transaction.join("previous")).expect("create previous package");
        fs::write(
            transaction.join("previous/original.txt"),
            b"original package",
        )
        .expect("write original package file");
        // Separate state paths permit deterministic failures without OS-specific permissions.
        // 独立状态路径允许构造确定性失败，无需依赖特定系统权限。
        let authorization = root.path().join("state/authorization.json");
        // Installation restoration is checked independently when authorization restoration fails.
        // 授权恢复失败时仍独立检查安装记录恢复。
        let installation = root.path().join("state/installation.json");
        // The exact bytes must survive dropping every in-memory transaction value.
        // 精确字节必须在所有内存事务值被丢弃后仍保留。
        let records = UpdateRollbackRecords {
            schema_version: 1,
            plugin_id: "local-plugin".into(),
            authorization: b"{ \"approved\": [] }\r\n".to_vec(),
            installation: b"{ \"version\": \"1.0.0\" }\n".to_vec(),
        };
        preserve_rollback_records(
            &transaction,
            &records.plugin_id,
            &records.authorization,
            &records.installation,
        )
        .expect("persist recovery snapshots");
        Self {
            previous_fingerprint: crate::catalog::compute_package_fingerprint(
                &transaction
                    .join("previous")
                    .canonicalize()
                    .expect("resolve original package"),
            )
            .expect("fingerprint original package"),
            destination: root.path().join("local-plugin"),
            _root: root,
            transaction,
            authorization,
            installation,
            records,
        }
    }

    /// Invoke the production failed-preparation path and return its exact restoration errors.
    /// 调用生产准备失败路径，返回其精确恢复错误。
    fn restore(&self) -> Vec<String> {
        rollback_failed_preparation(
            &self.transaction,
            &self.destination,
            &self.authorization,
            &self.installation,
            &self.records.authorization,
            &self.records.installation,
            &self.previous_fingerprint,
        )
    }

    /// Reload the snapshot from disk and compare every original byte without relying on the live transaction.
    /// 从磁盘重读快照并比较全部原始字节，不依赖存活事务。
    fn assert_retained_records(&self) {
        // Reading and decoding fresh bytes verifies retention outside the in-memory error path.
        // 读取并解码新取得的字节，验证内存错误路径之外的保留状态。
        let persisted: UpdateRollbackRecords = serde_json::from_slice(
            &fs::read(self.transaction.join(ROLLBACK_RECORDS_FILE))
                .expect("read retained rollback snapshot"),
        )
        .expect("decode retained rollback snapshot");
        assert_eq!(persisted, self.records);
    }
}

/// A conflicting candidate path must not delete the only old package; later successful restoration can clean up.
/// 候选路径冲突时不得删除唯一旧包；后续恢复成功才可清理。
#[test]
fn conflicting_candidate_path_retains_old_package_and_original_records() {
    // A file at the package destination makes the real filesystem validation fail on every platform.
    // 在插件目标位置放置文件，使真实文件系统校验在各平台均失败。
    let fixture = RollbackFixture::new();
    fs::write(&fixture.destination, b"blocking file").expect("block package restoration");
    // Retain the actual errors instead of replacing filesystem behavior with an injected error.
    // 保留实际错误，不以注入错误替代文件系统行为。
    let failures = fixture.restore();
    assert!(
        failures
            .iter()
            .any(|message| message.contains("must be a real directory"))
    );
    assert_eq!(
        fs::read(fixture.transaction.join("previous/original.txt"))
            .expect("read retained original package"),
        b"original package"
    );
    assert_eq!(
        fs::read(&fixture.destination).expect("read conflicting destination"),
        b"blocking file"
    );
    fixture.assert_retained_records();

    fs::remove_file(&fixture.destination).expect("remove exact fixture obstruction");
    assert!(fixture.restore().is_empty());
    assert_eq!(
        fs::read(fixture.destination.join("original.txt")).expect("read restored original package"),
        b"original package"
    );
    assert_eq!(
        fs::read(&fixture.authorization).expect("read restored authorization"),
        fixture.records.authorization
    );
    assert_eq!(
        fs::read(&fixture.installation).expect("read restored installation"),
        fixture.records.installation
    );
    assert!(!fixture.transaction.exists());
}

/// State restoration failure retains snapshots even when restoring the original package already succeeded.
/// 即使原插件包已恢复成功，状态恢复失败仍保留快照。
#[test]
fn failed_authorization_restoration_retains_record_bytes_after_package_restore() {
    // A directory at the exact state-file location is rejected by the production atomic writer.
    // 在精确状态文件位置放置目录，由生产原子写入器明确拒绝。
    let fixture = RollbackFixture::new();
    fs::create_dir_all(&fixture.authorization).expect("block authorization replacement");
    // Package and installation restoration still run, but they do not justify removing the transaction.
    // 插件包与安装记录仍执行恢复，但其成功不能成为删除事务的依据。
    let failures = fixture.restore();
    assert!(
        failures
            .iter()
            .any(|message| message.contains("restore previous plugin authorization"))
    );
    assert_eq!(
        fs::read(fixture.destination.join("original.txt")).expect("read restored original package"),
        b"original package"
    );
    assert_eq!(
        fs::read(&fixture.installation).expect("read restored installation"),
        fixture.records.installation
    );
    fixture.assert_retained_records();

    fs::remove_dir(&fixture.authorization).expect("remove empty state-file obstruction");
    assert!(fixture.restore().is_empty());
    assert_eq!(
        fs::read(&fixture.authorization).expect("read restored authorization"),
        fixture.records.authorization
    );
    assert!(!fixture.transaction.exists());
}

/// A missing or altered original generation must not be mistaken for successful restoration.
/// 不得把原始代次缺失或变化误判为恢复成功。
#[test]
fn missing_or_changed_original_package_retains_candidate_and_recovery_records() {
    // Cover both an absent retained package and a present package whose bytes changed.
    // 同时覆盖保留插件包缺失与其存在但字节变化。
    for original_missing in [true, false] {
        // Every case starts from an independently persisted original snapshot.
        // 每种情况均从独立持久保存的原始快照开始。
        let fixture = RollbackFixture::new();
        fs::create_dir(&fixture.destination).expect("create candidate directory");
        fs::write(
            fixture.destination.join("candidate.txt"),
            b"candidate remains",
        )
        .expect("write candidate contents");
        if original_missing {
            fs::remove_file(fixture.transaction.join("previous/original.txt"))
                .expect("remove original fixture file");
            fs::remove_dir(fixture.transaction.join("previous"))
                .expect("remove empty original directory");
        } else {
            fs::write(
                fixture.transaction.join("previous/original.txt"),
                b"changed original",
            )
            .expect("alter original fixture contents");
        }
        // The production path compares the original catalog identity before deleting any candidate.
        // 生产路径在删除任何候选前比较原始目录身份。
        let failures = fixture.restore();
        assert!(
            failures
                .iter()
                .any(|message| message.contains("differs from the original installed contents"))
        );
        assert_eq!(
            fs::read(fixture.destination.join("candidate.txt")).expect("read retained candidate"),
            b"candidate remains"
        );
        fixture.assert_retained_records();
        if !original_missing {
            assert_eq!(
                fs::read(fixture.transaction.join("previous/original.txt"))
                    .expect("read altered original contents"),
                b"changed original"
            );
        }
    }
}
