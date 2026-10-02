//! Retained original owners confirm real promotion writes without replaying physical preparation.
//! 保留的原所有者确认真实晋级写入，不重放物理准备。

use super::*;

/// Prepare real immutable bytes and staged empty credentials under `root`; return the original exclusive owner.
/// 在 `root` 下准备真实不可变字节及已暂存空凭据；返回原排他所有者。
fn ready_fixture(
    root: &Path,
) -> (
    VulcanPluginGenerationStore,
    VulcanPluginActivationPreparation,
) {
    // The existing fixture records its full selected predecessor before allocating candidate resources.
    // 既有夹具在分配候选资源前记录完整已选前驱。
    let (store, source, preparation) = early_fixture(root);
    // Copy exactly the originally reserved package rather than discovering a replacement generation.
    // 仅复制原预留插件包，不发现替代代际。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("prepare original package");
    // This writer actually retains the parent's exclusive request lease until explicit retirement.
    // 此写入者实际保留父级排他请求租约，直到明确退役。
    let mut staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("begin original credentials");
    staging
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("finish original credentials");
    drop(staging);
    (store, preparation)
}

/// The returned transaction shares the same original physical owner until the old preparation is actually dropped.
/// 返回事务共享同一原物理所有者，直到旧准备对象实际释放。
#[test]
fn preparation_promotion_retained_transfers_same_owner_without_unlock_gap() {
    // Every lock and journal belongs to an isolated real store.
    // 全部锁及日志属于隔离真实存储。
    let temp = tempfile::tempdir().expect("fixture");
    // Complete resources do not imply that their request has already been promoted.
    // 完整资源不表示其请求已经晋级。
    let (store, mut preparation) = ready_fixture(temp.path());
    // Keep the canonical original identity across the temporary shared-owner window.
    // 跨临时共享所有者窗口保留规范原身份。
    let request = preparation.request_id();
    // The transaction borrows physical authority from the same request, not a newly acquired file lock.
    // 事务从同一请求借用物理权威，而非重新取得文件锁。
    let mut transaction = store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("promote retained owner");
    assert_eq!(transaction.record().request_id(), request);
    assert_eq!(
        store
            .resume_activation_transaction("sample-plugin", request)
            .expect_err("original physical owner remains exclusive")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .commit_activation_transaction(&mut transaction)
            .expect_err("old preparation must retire before transaction mutation")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .abort_activation_transaction(&mut transaction)
            .expect_err("old preparation also prevents transaction abandonment")
            .code,
        Code::Busy
    );
    drop(preparation);
    store
        .commit_activation_transaction(&mut transaction)
        .expect("same transaction proceeds after actual owner transfer");
}

/// A real successful write whose observation is lost is confirmed by the same owner without another write.
/// 观测丢失的真实成功写入，由同一所有者确认而不再次写入。
#[test]
fn preparation_promotion_retained_confirms_successful_write_after_observation_loss() {
    // Fault injection is scoped to this exact root and request after the actual atomic writer succeeds.
    // 故障注入仅作用于此精确根及请求，且发生在实际原子写入器成功之后。
    let temp = tempfile::tempdir().expect("fixture");
    // Original physical resources and credential inventory are genuinely complete.
    // 原物理资源及凭据清单确实完整。
    let (store, mut preparation) = ready_fixture(temp.path());
    // The same UUID remains owned even though the caller does not receive the first transaction.
    // 即使调用方未收到首个事务，同一 UUID 仍被持有。
    let request = preparation.request_id();
    // Retain the scoped loss and actual successful writer count for the whole retry sequence.
    // 在整个重试序列保留作用域丢失及实际成功写入次数。
    let _fault = store.observe_preparation_promotion_writes_for_test(request, Some(false));
    assert_eq!(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .expect_err("successful write lost its observation")
            .code,
        Code::Storage
    );
    assert_eq!(
        store
            .activation_request("sample-plugin", request)
            .expect("canonical readback is unaffected")
            .expect("original record")
            .phase(),
        Phase::Prepared
    );
    assert_eq!(
        store
            .resume_activation_transaction("sample-plugin", request)
            .expect_err("failed foreground observation cannot release original ownership")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::Document {
                    value: serde_json::Value::Null,
                },
            )
            .expect_err("retry cannot substitute a different complete configuration")
            .code,
        Code::Conflict
    );
    // Exact persisted bytes remain evidence while the original owner confirms the completed stage.
    // 原所有者确认已完成阶段时，精确持久字节继续作为证据。
    let path = record_path(temp.path(), request);
    // A read-only retry must preserve the original journal byte for byte.
    // 只读重试必须逐字节保留原日志。
    let bytes = fs::read(&path).expect("prepared bytes");
    // Windows permits canonical reads but denies the delete sharing needed for another atomic replacement.
    // Windows 允许规范读取，但拒绝再次原子替换所需的删除共享。
    #[cfg(windows)]
    let _replacement_blocker = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001 | 0x0000_0002)
            .open(&path)
            .expect("deny real journal replacement while permitting exact readback")
    };
    // The same original preparation remains available after every error above.
    // 上述每项错误后，同一原准备对象继续可用。
    let transaction = store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("confirm exact already-written Prepared");
    assert_eq!(transaction.record().request_id(), request);
    assert_eq!(fs::read(path).expect("unchanged bytes"), bytes);
    assert_eq!(store.preparation_promotion_write_count_for_test(request), 1);
}

/// Unwinding after the actual writer succeeds leaves the original preparation available for exact confirmation.
/// 实际写入器成功后发生展开，原准备对象仍可用于精确确认。
#[test]
fn preparation_promotion_retained_survives_post_write_worker_panic() {
    // Only an isolated synchronous writer observation is deliberately panicked.
    // 仅故意使隔离同步写入观测发生 panic。
    let temp = tempfile::tempdir().expect("fixture");
    // Service-like retained ownership stays outside the failing worker frame.
    // 类似服务的保留所有权处于失败工作帧之外。
    let (store, mut preparation) = ready_fixture(temp.path());
    // Stable identity must not be reconstructed after unwinding.
    // 展开后不得重建稳定身份。
    let request = preparation.request_id();
    // Panic once after the real Prepared write, retaining actual write counting across retry.
    // 在真实 Prepared 写入后仅 panic 一次，跨重试保留实际写入计数。
    let _fault = store.observe_preparation_promotion_writes_for_test(request, Some(true));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
        }))
        .is_err()
    );
    assert_eq!(
        store
            .resume_activation_transaction("sample-plugin", request)
            .expect_err("worker panic leaves original owner retained")
            .code,
        Code::Busy
    );
    store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("same owner confirms real post-panic journal");
    assert_eq!(store.preparation_promotion_write_count_for_test(request), 1);
}

/// Dropping an unobserved returned local transaction keeps its original preparation and never replays promotion.
/// 释放未被观察的已返回局部事务，会保留原准备对象且绝不重放晋级。
#[test]
fn preparation_promotion_retained_retries_after_returned_transaction_is_dropped() {
    // Actual local return-value loss models a worker result disappearing before service variant replacement.
    // 实际局部返回值丢失模拟工作结果在服务变体替换之前消失。
    let temp = tempfile::tempdir().expect("fixture");
    // The external owner remains independently alive through that return-value loss.
    // 外部所有者独立跨该返回值丢失存活。
    let (store, mut preparation) = ready_fixture(temp.path());
    // Observe only this real original writer without injecting an error.
    // 仅观察此真实原写入器，不注入错误。
    let request = preparation.request_id();
    // Keep exact real write counting until the second returned value is received.
    // 保留精确真实写入计数，直到收到第二个返回值。
    let _observation = store.observe_preparation_promotion_writes_for_test(request, None);
    drop(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .expect("first genuine returned transaction"),
    );
    assert_eq!(
        store
            .resume_activation_transaction("sample-plugin", request)
            .expect_err("local transaction drop preserves external original owner")
            .code,
        Code::Busy
    );
    // The second transaction has newly verified resource pins and the same original request lease.
    // 第二个事务具有重新校验的资源固定及同一原请求租约。
    let transaction = store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("confirm previously returned Prepared");
    assert_eq!(transaction.record().request_id(), request);
    assert_eq!(store.preparation_promotion_write_count_for_test(request), 1);
}

/// Finished credential staging still prevents promotion until its actual parent-lock owner is retired.
/// 已完成凭据暂存仍阻止晋级，直到其实际父锁所有者退役。
#[test]
fn preparation_promotion_retained_requires_actual_staging_retirement() {
    // A genuine staging owner remains alive after its inventory becomes durable.
    // 真实暂存所有者在其清单持久化后继续存活。
    let temp = tempfile::tempdir().expect("fixture");
    // Original preparation must remain usable when the live writer blocks promotion.
    // 活动写入者阻止晋级时，原准备对象必须继续可用。
    let (store, source, mut preparation) = early_fixture(temp.path());
    // This is the exact originally planned immutable candidate.
    // 这是精确原计划不可变候选。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy");
    // The writer retains the same original request lock even after successful secure writes.
    // 即使安全写入成功，写入者仍保留同一原请求锁。
    let mut staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("stage");
    staging
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("finish");
    assert_eq!(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .expect_err("actual finished writer must retire")
            .code,
        Code::Busy
    );
    drop(staging);
    store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("retired actual writer permits promotion");
}

/// Exact original store, index, selected predecessor and durable phase are mandatory during retained confirmation.
/// 保留式确认必须满足精确原存储、索引、已选前驱及持久阶段。
#[test]
fn preparation_promotion_retained_rejects_changed_authority_without_repair() {
    for corruption in [
        "store",
        "index",
        "selection",
        "phase",
        "intent",
        "predecessor",
    ] {
        // Each corruption has its own authentic original and unshared test-only root.
        // 每种损坏具有各自真实原请求及不共享的仅测试根目录。
        let temp = tempfile::tempdir().expect("fixture");
        // Full physical target exists so rejection cannot come from incomplete preparation.
        // 完整物理目标存在，使拒绝不能归因于不完整准备。
        let (store, mut preparation) = ready_fixture(temp.path());
        // The first local return may disappear while the original preparation survives.
        // 首个局部返回可能消失，而原准备对象存活。
        let request = preparation.request_id();
        drop(
            store
                .finish_activation_preparation_retained(
                    &mut preparation,
                    VulcanPluginConfigurationSelection::NotDeclared {},
                )
                .expect("persist exact Prepared"),
        );
        // Derive only canonical fixture paths whose authority is already fixed by the original UUID.
        // 仅派生权威已由原 UUID 固定的规范夹具路径。
        let path = record_path(temp.path(), request);
        // Preserve exact Prepared evidence before explicitly corrupting one authoritative field.
        // 明确损坏一个权威字段前保留精确 Prepared 证据。
        let bytes = fs::read(&path).expect("original Prepared bytes");
        if corruption == "store" {
            // A separate existing root cannot claim this original request even with identical plugin names.
            // 即使插件名称相同，独立既有根目录也不能声称持有此原请求。
            let foreign = tempfile::tempdir().expect("foreign root");
            // Opening a real foreign store must not modify the authentic original.
            // 打开真实外部存储不得修改真实原请求。
            let other = VulcanPluginGenerationStore::open(foreign.path()).expect("foreign store");
            assert_eq!(
                other
                    .finish_activation_preparation_retained(
                        &mut preparation,
                        VulcanPluginConfigurationSelection::NotDeclared {},
                    )
                    .expect_err("cross-store original rejected")
                    .code,
                Code::Conflict
            );
        } else {
            match corruption {
                "index" => {
                    fs::remove_file(path.with_file_name("current")).expect("remove exact index");
                }
                "selection" => {
                    fs::remove_file(store.root.join("active/sample-plugin.json"))
                        .expect("remove original selection");
                }
                "phase" | "intent" | "predecessor" => {
                    // Mutate a structurally valid full journal field instead of supplying a malformed decoder input.
                    // 修改结构有效完整日志字段，而非提供格式错误解码输入。
                    let mut wire: serde_json::Value = serde_json::from_slice(&bytes).expect("wire");
                    if corruption == "phase" {
                        wire["phase"] = serde_json::json!("committing");
                    } else if corruption == "intent" {
                        // Both target and intent name a valid different revision; structural validation alone cannot detect substitution.
                        // 目标及意图同时命名有效不同修订；仅结构校验不能检测替换。
                        let replacement =
                            serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                                .expect("valid replacement identity");
                        wire["preparation"]["credential_revision"] = replacement.clone();
                        wire["target"]["credential_revision"] = replacement;
                    } else {
                        wire["predecessor"]["configuration"] =
                            serde_json::to_value(VulcanPluginConfigurationSelection::Document {
                                value: serde_json::Value::Null,
                            })
                            .expect("valid different predecessor configuration");
                    }
                    fs::write(&path, serde_json::to_vec(&wire).expect("changed wire"))
                        .expect("inject explicit corruption");
                }
                _ => unreachable!("declared corruption"),
            }
            assert!(
                store
                    .finish_activation_preparation_retained(
                        &mut preparation,
                        VulcanPluginConfigurationSelection::NotDeclared {},
                    )
                    .is_err(),
                "corruption: {corruption}"
            );
        }
        if corruption != "phase" && corruption != "intent" && corruption != "predecessor" {
            assert_eq!(fs::read(&path).expect("original journal retained"), bytes);
        }
    }
}

/// The consuming compatibility API retires its old owner before callers mutate the returned transaction.
/// 消费式兼容接口在调用方变更返回事务之前，退役其旧所有者。
#[test]
fn preparation_promotion_consuming_wrapper_retires_original_preparation() {
    // This fixture exercises the existing API rather than directly dropping a borrowed preparation.
    // 此夹具验证既有接口，而非直接释放被借用准备对象。
    let temp = tempfile::tempdir().expect("fixture");
    // Package and credentials use the same original request binding as retained promotion.
    // 插件包及凭据使用与保留式晋级相同的原请求绑定。
    let (store, preparation) = ready_fixture(temp.path());
    // A normal compatibility return must contain the sole actual original request lease.
    // 正常兼容返回必须包含唯一实际原请求租约。
    let mut transaction = store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("consuming original API");
    store
        .commit_activation_transaction(&mut transaction)
        .expect("sole returned owner can commit");
}

/// Retained confirmation revalidates physical package bytes even after an earlier Prepared return was lost.
/// 即使较早 Prepared 返回丢失，保留式确认仍重新校验物理插件包字节。
#[test]
fn preparation_promotion_retained_revalidates_actual_target_after_return_loss() {
    // Real package corruption cannot be hidden by a correctly shaped durable Prepared journal.
    // 正确形状持久 Prepared 日志不能掩盖真实插件包损坏。
    let temp = tempfile::tempdir().expect("fixture");
    // The authentic original remains held while its first configured result disappears.
    // 真实原请求在首个已配置结果消失时仍被持有。
    let (store, mut preparation) = ready_fixture(temp.path());
    // Retain actual original identity for the subsequent real file-lock assertion.
    // 保留实际原身份，供后续真实文件锁断言使用。
    let request = preparation.request_id();
    // Capture the actual returned pinned package path before dropping only the local transaction.
    // 仅释放局部事务前捕获实际返回的固定插件包路径。
    let transaction = store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("first exact Prepared");
    // The package root is supplied by the actual pinned generation rather than a guessed storage layout.
    // 插件包根来自实际固定代际，而非猜测存储布局。
    let package_root = transaction.generation().package_root().to_path_buf();
    drop(transaction);
    fs::write(
        package_root.join("unexpected.txt"),
        b"changed immutable bytes",
    )
    .expect("corrupt actual target");
    assert!(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .is_err()
    );
    assert_eq!(
        store
            .resume_activation_transaction("sample-plugin", request)
            .expect_err("target validation error preserves original physical ownership")
            .code,
        Code::Busy
    );
}

/// Version-three promotion preserves the original user policy and rejects a valid policy replacement during readback.
/// 第三版晋级保留原用户策略，并在回读期间拒绝有效策略替换。
#[test]
fn preparation_promotion_retained_preserves_version_three_original_policy() {
    // A real selected predecessor exists before the policy-bearing update request begins.
    // 携带策略的更新请求开始前，真实已选前驱已经存在。
    let temp = tempfile::tempdir().expect("fixture");
    // Retire only the fixture's unallocated legacy intent before creating the actual version-three update.
    // 创建实际第三版更新之前，仅退役夹具未分配的旧意图。
    let (store, source, legacy_preparation) = early_fixture(temp.path());
    // Explicit abandonment uses the authentic old UUID after its actual physical owner retires.
    // 实际物理所有者退役后，明确放弃使用真实旧 UUID。
    let legacy_request = legacy_preparation.request_id();
    drop(legacy_preparation);
    store
        .abort_activation_preparation_request("sample-plugin", legacy_request)
        .expect("abandon only unallocated fixture intent");
    // This complete policy is captured from the predecessor, independently of candidate defaults.
    // 此完整策略从前驱捕获，独立于候选默认值。
    let policy = crate::VulcanPluginUpdatePolicy {
        explicitly_enabled: false,
        explicitly_disabled: true,
        predecessor_default_enabled: true,
    };
    // Approved fingerprint comes from the actual source inspected for this new original request.
    // 已批准指纹来自为此新原请求检查的实际来源。
    let fingerprint = crate::inspect_user_plugin_package(&source)
        .expect("approved source")
        .content_fingerprint;
    // The complete current selection is compared by the real update-preparation API.
    // 真实更新准备接口比较完整当前选择。
    let predecessor = store
        .activation("sample-plugin")
        .expect("original predecessor");
    // This is the production host-update version-three intent, not a rewritten version-two journal.
    // 这是生产宿主更新使用的第三版意图，而非改写后的第二版日志。
    let mut preparation = store
        .begin_activation_update_preparation(
            Uuid::new_v4(),
            &source,
            &fingerprint,
            predecessor.as_ref(),
            vec![],
            policy,
        )
        .expect("original policy-bearing update");
    // Stage genuine package and credentials under this exact policy-bearing parent.
    // 在此精确携带策略父级下暂存真实插件包及凭据。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy actual package");
    // Successful staging retains the original physical request until its writer is actually dropped.
    // 成功暂存在其写入者实际释放前保留原物理请求。
    let mut staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("original staged credentials");
    staging
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("finish actual staging");
    drop(staging);
    // Both the original policy and frozen configuration must survive loss of a local configured return value.
    // 原策略及固定配置都必须跨局部已配置返回值丢失存活。
    let request = preparation.request_id();
    // Count only real writes for this policy-bearing original.
    // 仅统计此携带策略原请求的真实写入。
    let _observation = store.observe_preparation_promotion_writes_for_test(request, None);
    drop(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .expect("first version-three Prepared"),
    );
    assert_eq!(
        store
            .activation_request("sample-plugin", request)
            .expect("canonical original")
            .expect("record")
            .update_policy(),
        Some(&policy)
    );
    // Preserve the actual complete Prepared bytes before injecting another structurally valid policy.
    // 注入另一结构有效策略之前，保留实际完整 Prepared 字节。
    let path = record_path(temp.path(), request);
    // Exact restoration does not allocate another request or replay package/credential preparation.
    // 精确还原不分配另一请求，也不重放插件包／凭据准备。
    let original_bytes = fs::read(&path).expect("original version-three journal");
    // Change only a valid policy member while retaining every identity and the complete frozen target.
    // 仅改变有效策略成员，同时保留每项身份及完整固定目标。
    let mut changed: serde_json::Value =
        serde_json::from_slice(&original_bytes).expect("version-three wire");
    changed["preparation"]["update_policy"]["explicitly_enabled"] = serde_json::json!(true);
    fs::write(
        &path,
        serde_json::to_vec(&changed).expect("valid changed policy"),
    )
    .expect("inject policy replacement");
    assert!(
        store.activation_request("sample-plugin", request).is_ok(),
        "policy replacement remains structurally valid"
    );
    assert_eq!(
        store
            .finish_activation_preparation_retained(
                &mut preparation,
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .expect_err("valid replacement cannot override the retained original policy")
            .code,
        Code::Conflict
    );
    assert_eq!(preparation.intent().update_policy(), Some(&policy));
    assert_eq!(
        store
            .resume_activation_transaction("sample-plugin", request)
            .expect_err("policy mismatch preserves original exclusive ownership")
            .code,
        Code::Busy
    );
    fs::write(&path, original_bytes).expect("restore exact original evidence");
    store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("confirm exact policy-bearing original");
    assert_eq!(store.preparation_promotion_write_count_for_test(request), 1);
}
