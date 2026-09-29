//! Real original journals retain user policy through early preparation, promotion and strict recovery.
//! 真实原日志在早期准备、晋级及严格恢复期间保留用户策略。

use super::*;

/// Replace a terminal legacy fixture request with a policy-bearing update and return its actual owner.
/// 将已终结旧格式夹具请求替换为携带策略的更新，并返回实际所有者。
/// The root isolates real files; policy is the exact host-approved snapshot.
/// root 隔离真实文件；policy 为宿主批准的精确快照。
fn update_fixture(
    root: &Path,
    policy: VulcanPluginUpdatePolicy,
) -> (
    VulcanPluginGenerationStore,
    PathBuf,
    VulcanPluginActivationPreparation,
) {
    // Keep a real managed predecessor and retire only the initial fixture request.
    // 保留真实受管前驱，仅终结初始夹具请求。
    let (store, source, legacy) = early_fixture(root);
    // The terminal older request remains readable alongside the new format.
    // 已终结较旧请求与新格式同时保持可读。
    let legacy_id = legacy.request_id();
    drop(legacy);
    store
        .abort_activation_preparation_request("sample-plugin", legacy_id)
        .expect("legacy abort");
    assert!(
        store
            .activation_request("sample-plugin", legacy_id)
            .expect("legacy query")
            .expect("legacy")
            .update_policy()
            .is_none()
    );
    // Exact predecessor and source approval are supplied to the production update entry.
    // 向生产更新入口提供精确前驱及来源批准。
    let selected = store.activation("sample-plugin").expect("selection");
    // Inspection fixes the actual complete source digest.
    // 检查固定实际完整来源摘要。
    let inspected = crate::inspect_user_plugin_package(&source).expect("source");
    // The new original owns both its policy and physical preparation identities.
    // 新原请求同时拥有策略及物理准备身份。
    let original = store
        .begin_activation_update_preparation(
            Uuid::new_v4(),
            &source,
            &inspected.content_fingerprint,
            selected.as_ref(),
            vec![],
            policy,
        )
        .expect("update intent");
    (store, source, original)
}

/// Promote one original after actual copying and parent-bound secure staging; return its configured owner.
/// 实际复制及父绑定安全暂存后晋级单个原请求；返回其已配置所有者。
fn promote_update(
    store: &VulcanPluginGenerationStore,
    source: &Path,
    preparation: VulcanPluginActivationPreparation,
) -> VulcanPluginActivationTransaction {
    // Copy and credential writes retain the exact original request throughout.
    // 复制及凭据写入全程保留精确原请求。
    let package = store
        .prepare_activation_package(&preparation, source)
        .expect("copy");
    // The empty inventory still exercises real parent-bound staging and schema validation.
    // 空清单仍验证真实父绑定暂存及模式校验。
    let mut writer = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("writer");
    writer
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("staged");
    drop(writer);
    store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("promote original")
}

/// Every override combination survives reopening and promotion without changing the original target or policy.
/// 每种覆盖组合在重新打开及晋级后保持原目标及策略。
#[test]
fn activation_update_policy_survives_original_promotion_and_reopen() {
    for explicitly_enabled in [false, true] {
        for explicitly_disabled in [false, true] {
            for predecessor_default_enabled in [false, true] {
                // Each original policy gets independent actual files and locks.
                // 每个原策略具有独立真实文件及锁。
                let temp = tempfile::tempdir().expect("root");
                // Preserve both memberships exactly, including enabled precedence before normalization.
                // 精确保留两个成员关系，包含归一化前启用优先语义。
                let policy = VulcanPluginUpdatePolicy {
                    explicitly_enabled,
                    explicitly_disabled,
                    predecessor_default_enabled,
                };
                // The policy-bearing request exists before physical copying.
                // 携带策略的请求先于物理复制存在。
                let (store, source, preparation) = update_fixture(temp.path(), policy);
                // Keep identity and a read-only baseline across real lock transfer.
                // 在实际锁转移期间保留身份及只读基线。
                let request_id = preparation.request_id();
                // No configured target exists at this point.
                // 此时没有已配置目标。
                let before = store
                    .activation_request("sample-plugin", request_id)
                    .expect("query")
                    .expect("record");
                assert_eq!(before.update_policy(), Some(&policy));
                assert!(before.target().is_none());
                assert_eq!(
                    store
                        .resume_activation_preparation("sample-plugin", request_id)
                        .expect_err("live owner")
                        .code,
                    Code::Busy
                );
                drop(preparation);
                // A new store opens the same real durable request and reacquires its original lock.
                // 新存储打开相同真实持久请求，并重新取得原锁。
                let reopened = VulcanPluginGenerationStore::open(temp.path()).expect("reopen");
                // Explicit resume never substitutes a new request or policy.
                // 明确恢复绝不替换为新请求或策略。
                let original = reopened
                    .resume_activation_preparation("sample-plugin", request_id)
                    .expect("original resume");
                assert_eq!(original.intent().update_policy(), Some(&policy));
                // Promotion transfers the same lease and persists the same version-three policy.
                // 晋级转交相同租约，并持久保存相同第三版策略。
                let mut transaction = promote_update(&reopened, &source, original);
                assert_eq!(transaction.record().update_policy(), Some(&policy));
                // The read-only baseline permits the one legal promotion while preserving policy.
                // 只读基线允许唯一合法晋级，同时保留策略。
                let after = reopened
                    .activation_request("sample-plugin", request_id)
                    .expect("query")
                    .expect("record");
                assert!(before.preserves_original(&after));
                reopened
                    .commit_activation_transaction(&mut transaction)
                    .expect("storage commit");
                reopened
                    .complete_activation_transaction(&mut transaction)
                    .expect("host acknowledgement");
                drop(transaction);
                assert_eq!(
                    store
                        .activation_request("sample-plugin", request_id)
                        .expect("historical query")
                        .expect("complete")
                        .update_policy(),
                    Some(&policy)
                );
            }
        }
    }
}

/// Both early and configured journals reject policy loss, wrong declared versions and malformed field shapes.
/// 早期及已配置日志均拒绝策略丢失、错误声明版本及畸形字段。
#[test]
fn activation_update_policy_strict_records_reject_loss_and_detect_replacement() {
    for complete in [false, true] {
        // Original policy is meaningful even when no explicit override was stored.
        // 即使未存储显式覆盖，原策略仍有明确含义。
        let policy = VulcanPluginUpdatePolicy {
            explicitly_enabled: false,
            explicitly_disabled: false,
            predecessor_default_enabled: true,
        };
        // All mutations below target only this test's original journal.
        // 以下变更仅针对本测试的原日志。
        let temp = tempfile::tempdir().expect("root");
        // Construct an actual original in the phase under test.
        // 构建被测阶段的实际原请求。
        let (store, source, preparation) = update_fixture(temp.path(), policy);
        // Original UUID stays fixed across all corruption attempts.
        // 全部损坏尝试期间原 UUID 保持固定。
        let request_id = preparation.request_id();
        if complete {
            drop(promote_update(&store, &source, preparation));
        } else {
            drop(preparation);
        }
        // Retain exact original bytes for restoration between independent malformed cases.
        // 保留精确原字节，以便在独立畸形用例间恢复。
        let path = record_path(temp.path(), request_id);
        // Reading fixture bytes cannot allocate or release execution resources.
        // 读取夹具字节不能分配或释放执行资源。
        let original_bytes = fs::read(&path).expect("original bytes");
        // This immutable snapshot must reject a semantically valid but replaced policy.
        // 此不可变快照必须拒绝语义有效但已替换的策略。
        let original = store
            .activation_request("sample-plugin", request_id)
            .expect("read")
            .expect("original");
        // One parsed fixture supports explicit independent corruption cases.
        // 单个已解析夹具支持明确独立损坏用例。
        let encoded: serde_json::Value = serde_json::from_slice(&original_bytes).expect("JSON");
        for case in 0..6 {
            // Every case starts from the exact original rather than accumulating corruption.
            // 每个用例从精确原记录开始，不累积损坏。
            let mut invalid = encoded.clone();
            match case {
                0 => {
                    invalid["preparation"]
                        .as_object_mut()
                        .expect("object")
                        .remove("update_policy");
                }
                1 => {
                    invalid["preparation"]["update_policy"] = serde_json::Value::Null;
                }
                2 => {
                    invalid["schema_version"] = serde_json::json!(2);
                }
                3 => {
                    invalid["preparation"]["update_policy"]["unknown"] = serde_json::json!(true);
                }
                4 => {
                    invalid["preparation"]["update_policy"]
                        .as_object_mut()
                        .expect("object")
                        .remove("explicitly_enabled");
                }
                5 => {
                    invalid["preparation"]["update_policy"]["explicitly_disabled"] =
                        serde_json::json!("false");
                }
                _ => unreachable!(),
            }
            fs::write(&path, serde_json::to_vec(&invalid).expect("encode")).expect("corrupt");
            assert_eq!(
                store
                    .activation_request("sample-plugin", request_id)
                    .expect_err("invalid record")
                    .code,
                Code::Invalid
            );
            assert!(store.activation_recovery_records("sample-plugin").is_err());
        }
        // A valid value replacement must fail the original-identity comparison even though its schema is legal.
        // 即使模式合法，有效值替换也必须无法通过原身份比较。
        let mut replaced = encoded;
        replaced["preparation"]["update_policy"]["predecessor_default_enabled"] =
            serde_json::json!(false);
        fs::write(&path, serde_json::to_vec(&replaced).expect("encode")).expect("replace policy");
        // Query can inspect a valid declared record but cannot turn it into the same original authority.
        // 查询可以检查有效声明记录，但不能将其变为相同原权威。
        let observed = store
            .activation_request("sample-plugin", request_id)
            .expect("valid shape")
            .expect("record");
        assert!(!original.preserves_original(&observed));
        fs::write(&path, original_bytes).expect("restore");
        assert_eq!(
            store
                .activation_request("sample-plugin", request_id)
                .expect("restored"),
            Some(original)
        );
    }
}
