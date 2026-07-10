//! Atomic segmented file publication tests.
//! 分段文件原子发布测试。

use super::*;
use serde::ser::Error as _;

/// Serialization input that always fails before filesystem publication begins.
/// 在文件系统发布开始前始终序列化失败的输入。
struct SerializationFailure;

impl Serialize for SerializationFailure {
    /// Returns an intentional serialization error without emitting any bytes.
    /// 返回刻意构造的序列化错误，且不输出任何字节。
    ///
    /// # Parameters
    /// 参数
    /// `_serializer` is unused because this test value must fail deterministically.
    /// `_serializer` 不使用，因为该测试值必须确定性失败。
    ///
    /// # Returns
    /// 返回值
    /// Always returns the serializer's custom error type.
    /// 始终返回序列化器的自定义错误类型。
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Err(S::Error::custom("intentional atomic writer failure"))
    }
}

/// Verifies serialization failure creates neither the target parent nor a staging file.
/// 验证序列化失败既不会创建目标父目录，也不会创建暂存文件。
#[test]
fn atomic_json_serialization_failure_does_not_touch_filesystem() {
    // root isolates the intentionally absent target parent.
    // root 隔离刻意保持不存在的目标父目录。
    let root = test_root("atomic_json_serialization_failure_does_not_touch_filesystem");
    // target_parent must remain absent because serialization precedes all filesystem work.
    // target_parent 必须保持不存在，因为序列化先于全部文件系统操作。
    let target_parent = root.join("missing-parent");
    // target_path is the final document that must never be created.
    // target_path 是绝不能被创建的最终文档。
    let target_path = target_parent.join("metadata.json");

    // result carries the intentional serializer failure from the production atomic writer.
    // result 承载生产原子 writer 返回的刻意序列化失败。
    let result = write_json_file_atomic(&target_path, &SerializationFailure);
    // target_parent_exists_after_failure captures filesystem state before test cleanup.
    // target_parent_exists_after_failure 在测试清理前捕获文件系统状态。
    let target_parent_exists_after_failure = target_parent.exists();
    // target_exists_after_failure proves the final document was not partially created.
    // target_exists_after_failure 证明最终文档没有被部分创建。
    let target_exists_after_failure = target_path.exists();
    let _ = fs::remove_dir_all(&root);

    let error = result.expect_err("serialization should fail");
    assert!(error.contains("intentional atomic writer failure"));
    assert!(!target_parent_exists_after_failure);
    assert!(!target_exists_after_failure);
}

/// Verifies concurrent JSONL replacements publish complete candidates without sharing staging files.
/// 验证并发 JSONL 替换会发布完整候选内容，且不会共享暂存文件。
#[test]
fn concurrent_atomic_jsonl_rewrites_publish_complete_candidates() {
    /// Number of synchronized replacements issued by each writer.
    /// 每个 writer 执行的同步替换次数。
    const REWRITE_COUNT_PER_WRITER: usize = 16;

    // root isolates the shared target and all sibling staging files.
    // root 隔离共享目标与全部同级暂存文件。
    let root = test_root("concurrent_atomic_jsonl_rewrites_publish_complete_candidates");
    // target_path is deliberately shared without an outer segmented append lock.
    // target_path 在没有外层分段追加锁的情况下被刻意共享。
    let target_path = root.join("s001.jsonl");
    // first_values form one distinguishable complete candidate file.
    // first_values 构成一份可区分的完整候选文件。
    let first_values = vec![
        json!({ "producer": "first", "row": 1 }),
        json!({ "producer": "first", "row": 2 }),
    ];
    // second_values form another complete candidate with a different row count.
    // second_values 构成另一份记录数不同的完整候选文件。
    let second_values = vec![
        json!({ "producer": "second", "row": 1 }),
        json!({ "producer": "second", "row": 2 }),
        json!({ "producer": "second", "row": 3 }),
    ];
    // first_expected remains available after first_values moves into its worker.
    // first_expected 在 first_values 移入工作线程后仍用于最终断言。
    let first_expected = first_values.clone();
    // second_expected remains available after second_values moves into its worker.
    // second_expected 在 second_values 移入工作线程后仍用于最终断言。
    let second_expected = second_values.clone();
    // barrier synchronizes every replacement iteration to maximize staging collision pressure.
    // barrier 同步每轮替换，以最大化暂存碰撞压力。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    // first_target is the first writer's owned target handle.
    // first_target 是第一个 writer 独占的目标句柄。
    let first_target = target_path.clone();
    // first_barrier participates in every paired release.
    // first_barrier 参与每次成对释放。
    let first_barrier = std::sync::Arc::clone(&barrier);
    // first_writer repeatedly publishes the first complete candidate.
    // first_writer 重复发布第一份完整候选内容。
    let first_writer = std::thread::spawn(move || {
        for _ in 0..REWRITE_COUNT_PER_WRITER {
            first_barrier.wait();
            write_json_lines_file_atomic(&first_target, &first_values)
                .expect("first atomic JSONL rewrite");
        }
    });
    // second_target is the second writer's owned target handle.
    // second_target 是第二个 writer 独占的目标句柄。
    let second_target = target_path.clone();
    // second_writer repeatedly publishes the second complete candidate.
    // second_writer 重复发布第二份完整候选内容。
    let second_writer = std::thread::spawn(move || {
        for _ in 0..REWRITE_COUNT_PER_WRITER {
            barrier.wait();
            write_json_lines_file_atomic(&second_target, &second_values)
                .expect("second atomic JSONL rewrite");
        }
    });
    first_writer.join().expect("join first atomic writer");
    second_writer.join().expect("join second atomic writer");

    // published_values must match one whole candidate rather than mixed or truncated rows.
    // published_values 必须匹配某一完整候选内容，不能出现混合或截断记录。
    let published_values = read_json_lines::<Value>(&target_path).expect("read published JSONL");
    // staging_entries capture any unique sibling files leaked after successful renames.
    // staging_entries 捕获成功重命名后可能泄漏的唯一同级文件。
    let staging_entries = fs::read_dir(&root)
        .expect("read atomic writer root")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect atomic writer entries")
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".vulcan-stage-")
        })
        .collect::<Vec<_>>();
    let _ = fs::remove_dir_all(&root);

    assert!(published_values == first_expected || published_values == second_expected);
    assert!(staging_entries.is_empty());
}
