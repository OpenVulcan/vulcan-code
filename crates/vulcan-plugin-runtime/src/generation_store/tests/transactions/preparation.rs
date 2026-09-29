//! Real original journals cover allocation-free intent, retained credential writers and stage promotion.
//! 真实原日志覆盖无分配意图、保留凭据写入者及阶段晋级。

use super::*;
use crate::{
    VulcanPluginActivationPreparation, VulcanPluginActivationRequestRecord as Request,
    VulcanPluginRuntimeManager, VulcanPluginRuntimeState,
};
use std::collections::BTreeSet;

/// An abandoned reservation cannot be recycled through standalone staging after a newer request replaces the index.
/// 较新请求替换索引后，已放弃预留不能通过独立暂存再次使用。
#[test]
fn activation_preparation_unallocated_historical_credentials_cannot_be_reused() {
    // The first original reserves credentials but never allocates their inventory.
    // 首个原请求预留凭据，但从未分配其清单。
    let temp = tempfile::tempdir().expect("fixture");
    // Retain a real package so the later rejection cannot be attributed to missing bytes.
    // 保留真实插件包，使后续拒绝不能归因于缺失字节。
    let (store, source, preparation) = early_fixture(temp.path());
    // Historical UUID and revision remain evidence after the original owner is dropped.
    // 原所有者释放后，历史 UUID 及修订继续作为证据。
    let request_id = preparation.request_id();
    // The revision has no inventory tombstone to provide an accidental alternative rejection.
    // 此修订没有清单停用记录，不会意外提供另一种拒绝原因。
    let revision = preparation.intent().credential_revision().clone();
    // Package preparation is complete while credentials have not begun.
    // 插件包准备完成，而凭据尚未开始。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy");
    drop(preparation);
    store
        .abort_activation_preparation_request("sample-plugin", request_id)
        .expect("first abandon");
    // A genuinely newer request changes the index and then terminates without writing credentials.
    // 真正较新请求变更索引，随后终结且不写凭据。
    let selected = store.activation("sample-plugin").expect("selected");
    // Reusing source content still allocates different immutable resource identities.
    // 复用来源内容仍分配不同的不可变资源身份。
    let newer = store
        .begin_activation_preparation(
            Uuid::new_v4(),
            &source,
            &package.identity().content_fingerprint,
            selected.as_ref(),
            vec![],
        )
        .expect("new original");
    // Store only the exact newer ID before dropping actual ownership.
    // 释放实际所有权前仅保存精确较新标识。
    let newer_id = newer.request_id();
    drop(newer);
    store
        .abort_activation_preparation_request("sample-plugin", newer_id)
        .expect("second abandon");
    assert!(
        store
            .credential_record("sample-plugin", &revision)
            .expect("no inventory")
            .is_none()
    );
    assert_eq!(
        store
            .begin_credential_staging(&package, revision, &VulcanPluginCredentialValues::new())
            .expect_err("historical reservation remains immutable")
            .code,
        Code::Conflict
    );
    assert!(
        store
            .begin_credential_staging(
                &package,
                VulcanPluginCredentialRevision::allocate(),
                &VulcanPluginCredentialValues::new()
            )
            .is_ok()
    );
}

/// A real failed Windows index replacement leaves an original that fences writes despite the older terminal pointer.
/// Windows 索引真实替换失败留下的原请求，在较旧终态指针存在时仍阻止写入。
#[cfg(windows)]
#[test]
fn activation_preparation_failed_index_replacement_preserves_unindexed_original() {
    use std::os::windows::fs::OpenOptionsExt;
    // Real OS sharing restrictions reject index replacement without blocking journal reads.
    // 实际操作系统共享限制拒绝索引替换，但不阻止日志读取。
    let temp = tempfile::tempdir().expect("fixture");
    // The first early request creates a genuine older terminal pointer.
    // 首个早期请求创建真实较旧终态指针。
    let (store, source, preparation) = early_fixture(temp.path());
    // Keep the first historical UUID independently of any live lock.
    // 独立于任何活动锁保留首个历史 UUID。
    let previous_id = preparation.request_id();
    drop(preparation);
    store
        .abort_activation_preparation_request("sample-plugin", previous_id)
        .expect("terminal predecessor request");
    // The original current file is opened without delete sharing, which atomic replacement requires.
    // 原 current 文件以不共享删除方式打开，原子替换需要该共享权限。
    let index = store.root.join("transactions/sample-plugin/current");
    // Preserve exact index bytes to verify that failure retained the historical pointer.
    // 保留精确索引字节，以验证失败保留历史指针。
    let original_index = fs::read(&index).expect("old index");
    // Share read and write, but deliberately deny deletion in this isolated fixture.
    // 在此隔离夹具中共享读写，但故意拒绝删除。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&index)
        .expect("hold original index");
    // Source approval and selected predecessor are still exact and unchanged.
    // 来源批准及已选前驱仍精确且未变。
    let descriptor = crate::inspect_user_plugin_package(&source).expect("source");
    // The incomplete handoff gets a UUID before either journal publication attempt.
    // 不完整交接在任一日志发布尝试前取得 UUID。
    let failed_id = Uuid::new_v4();
    // Preserve the complete selected authority, not only its numeric revision.
    // 保留完整已选权威，而非仅数值修订。
    let selected = store.activation("sample-plugin").expect("selected");
    assert!(
        store
            .begin_activation_preparation(
                failed_id,
                &source,
                &descriptor.content_fingerprint,
                selected.as_ref(),
                vec![]
            )
            .is_err()
    );
    assert_eq!(fs::read(&index).expect("still old index"), original_index);
    // The new durable body exists even though the current pointer still names the old aborted request.
    // 新持久正文已存在，即使当前指针仍指向旧已中止请求。
    let failed = store
        .activation_request("sample-plugin", failed_id)
        .expect("read original")
        .expect("new durable intent");
    assert_eq!(failed.phase(), Phase::Preparing);
    drop(blocker);
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("stale terminal index cannot permit mutation")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .collect(failed.candidate().expect("planned candidate"))
            .expect_err("original retention applies without index")
            .code,
        Code::Busy
    );
    assert!(
        store
            .begin_activation_preparation(
                Uuid::new_v4(),
                &source,
                &descriptor.content_fingerprint,
                selected.as_ref(),
                vec![]
            )
            .is_err()
    );
    assert_eq!(
        store
            .activation_transaction_ids("sample-plugin")
            .expect("exact originals")
            .len(),
        2
    );
    // Only explicit resumption of the failed original replaces the stale terminal pointer.
    // 仅明确恢复失败原请求才替换陈旧终态指针。
    let resumed = store
        .resume_activation_preparation("sample-plugin", failed_id)
        .expect("repair original index");
    assert!(store.prepare_activation_package(&resumed, &source).is_ok());
    drop(resumed);
    store
        .abort_activation_preparation_request("sample-plugin", failed_id)
        .expect("abandon repaired original");
}

/// A real partial-copy disk checkpoint stays owned by early intent and can only be explicitly abandoned then collected.
/// 真实部分复制磁盘检查点继续归属于早期意图，只能先明确放弃再回收。
#[test]
fn activation_preparation_partial_copy_checkpoint_retains_original_cleanup_identity() {
    // Construct physical interruption evidence without claiming a real process-kill or power-loss test.
    // 构造物理中断证据，不声称实际进程强杀或断电测试。
    let temp = tempfile::tempdir().expect("fixture");
    // Production preparation provides the authoritative paths and complete original journal.
    // 生产准备提供权威路径及完整原日志。
    let (store, source, preparation) = early_fixture(temp.path());
    // Keep identifiers after every live owner leaves.
    // 所有活动所有者退出后保留标识。
    let request_id = preparation.request_id();
    // The reserved generation is the only allowed cleanup target.
    // 预留代次是唯一允许的清理目标。
    let identity = preparation.intent().generation().clone();
    drop(
        store
            .prepare_activation_package(&preparation, &source)
            .expect("physical copy"),
    );
    // The production layout fixes this marker before copying any package bytes.
    // 生产布局在复制任何插件包字节之前固定此标记。
    let generation_root = store
        .root
        .join("packages/sample-plugin")
        .join(&identity.generation_id);
    // Replace the completion checkpoint with the earlier persisted phase and remove an unfinished copied file.
    // 将完成检查点替换为此前持久阶段，并移除尚未完成复制的文件。
    let mut marker: serde_json::Value = serde_json::from_slice(
        &fs::read(generation_root.join("generation.json")).expect("marker bytes"),
    )
    .expect("marker");
    marker["state"] = serde_json::json!("preparing");
    fs::write(
        generation_root.join("generation.json"),
        serde_json::to_vec(&marker).expect("earlier marker"),
    )
    .expect("interrupted marker");
    fs::remove_file(generation_root.join("sample-plugin/skills/review-helper/SKILL.md"))
        .expect("partial copied bytes");
    drop(preparation);
    drop(store);
    // A new store cannot promote or recopy this existing partial physical identity.
    // 新存储不能晋级或重新复制此既有部分物理身份。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen")
        .expect("store");
    assert!(reopened.pin(&identity).is_err());
    assert_eq!(
        reopened
            .collect(&identity)
            .expect_err("pending original retains partial copy")
            .code,
        Code::Busy
    );
    // Explicit resumption restores ownership only and never overwrites partial files.
    // 明确恢复仅恢复所有权，绝不覆盖部分文件。
    let resumed = reopened
        .resume_activation_preparation("sample-plugin", request_id)
        .expect("original owner");
    assert!(
        reopened
            .prepare_activation_package(&resumed, &source)
            .is_err()
    );
    assert!(
        reopened
            .finish_activation_preparation(
                resumed,
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .is_err()
    );
    assert_eq!(
        reopened
            .activation_request("sample-plugin", request_id)
            .expect("original query")
            .expect("record")
            .phase(),
        Phase::Preparing
    );
    reopened
        .abort_activation_preparation_request("sample-plugin", request_id)
        .expect("explicit abandon");
    assert!(
        reopened
            .collect(&identity)
            .expect("explicit partial copy collection")
    );
    assert_eq!(
        reopened
            .activation("sample-plugin")
            .expect("predecessor")
            .expect("selected")
            .revision,
        1
    );
}

/// A legacy aborted receipt remains idempotent while a newer early request owns the current index.
/// 较新早期请求持有当前索引时，旧版已中止回执仍保持幂等。
#[test]
fn activation_preparation_preserves_legacy_receipt_across_new_index() {
    // The shared fixture creates genuine legacy ready targets and their complete predecessor.
    // 共享夹具创建真实旧版就绪目标及其完整前驱。
    let temp = tempfile::tempdir().expect("fixture");
    // Legacy staging is completed before any early request reserves this namespace.
    // 旧版暂存在任何早期请求预留此命名空间前完成。
    let (store, _first, second, _first_credentials, second_credentials) = fixture(temp.path());
    // This historical request is terminated using the old public API.
    // 此历史请求通过旧公开接口终结。
    let legacy_id = Uuid::new_v4();
    // Retain the original owner through its explicit abort.
    // 通过明确中止过程保留原所有者。
    let mut legacy = begin(&store, &second, &second_credentials, legacy_id);
    store
        .abort_activation_transaction(&mut legacy)
        .expect("legacy abort");
    drop(legacy);
    // The same approved source is now allocated under a genuinely different early request.
    // 相同已批准来源现在分配于真正不同的早期请求之下。
    let source = temp.path().join("second/sample-plugin");
    // The complete authority must still match the selected predecessor.
    // 完整权威必须仍匹配已选前驱。
    let predecessor = store.activation("sample-plugin").expect("predecessor");
    // The new original owns the current index throughout the historical observation.
    // 历史观察期间，新原请求始终持有当前索引。
    let preparation = store
        .begin_activation_preparation(
            Uuid::new_v4(),
            &source,
            &second.identity().content_fingerprint,
            predecessor.as_ref(),
            vec![],
        )
        .expect("new early original");
    // Querying an old terminal receipt must not rewrite or release the newer original.
    // 查询旧终态回执不能改写或释放较新原请求。
    let current_id = preparation.request_id();
    assert_eq!(
        store
            .abort_activation_request("sample-plugin", legacy_id)
            .expect("legacy idempotent receipt")
            .phase(),
        Phase::Aborted
    );
    assert_eq!(
        store
            .activation_request("sample-plugin", current_id)
            .expect("current")
            .expect("record")
            .phase(),
        Phase::Preparing
    );
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("new original still fences writers")
            .code,
        Code::Busy
    );
    drop(preparation);
    store
        .abort_activation_preparation_request("sample-plugin", current_id)
        .expect("early abort");
    assert_eq!(
        store
            .abort_activation_request("sample-plugin", legacy_id)
            .expect("legacy receipt after terminal early index")
            .phase(),
        Phase::Aborted
    );
}

/// Build a genuine selected predecessor under `root` and return a new source and its early exclusive owner.
/// 在 `root` 下构建真实已选前驱，返回新来源及其早期排他所有者。
fn early_fixture(
    root: &Path,
) -> (
    VulcanPluginGenerationStore,
    PathBuf,
    VulcanPluginActivationPreparation,
) {
    // Fixture storage follows the production requirement that its user parent already exists.
    // 夹具存储遵循用户父目录已存在的生产要求。
    let store = VulcanPluginGenerationStore::open(root).expect("store");
    // Complete predecessor bytes and credentials are prepared through existing production APIs.
    // 完整前驱字节及凭据通过既有生产接口准备。
    let predecessor = prepare(
        &store,
        &write_package(&root.join("old-source"), "sample-plugin", "old"),
    );
    // The old secure revision stays independent from the future reserved revision.
    // 旧安全修订独立于未来预留修订。
    let revision = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &predecessor, &revision);
    store
        .activate_configured(
            &predecessor,
            0,
            vec![],
            revision,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("select predecessor");
    // Drop the real predecessor observer so retention assertions depend on durable request ownership.
    // 释放实际前驱观察者，使保留断言依赖持久请求所有权。
    drop(predecessor);
    // The new source has not been copied into managed storage yet.
    // 新来源尚未复制到受管存储。
    let source = write_package(&root.join("new-source"), "sample-plugin", "new");
    // Authoritative source inspection produces the exact approved fingerprint.
    // 权威来源检查产生精确已批准指纹。
    let fingerprint = crate::inspect_user_plugin_package(&source)
        .expect("source inspection")
        .content_fingerprint;
    // Capture the complete selected authority, not only its numeric revision.
    // 捕获完整已选权威，而非仅捕获数值修订。
    let selected = store.activation("sample-plugin").expect("selected");
    // Publication of the early original precedes any candidate directory or credential inventory.
    // 早期原请求发布先于任何候选目录或凭据清单。
    let preparation = store
        .begin_activation_preparation(
            Uuid::new_v4(),
            &source,
            &fingerprint,
            selected.as_ref(),
            vec![],
        )
        .expect("early original");
    (store, source, preparation)
}

/// Intent exists before physical allocation, and a changed source cannot replace its frozen identity.
/// 意图在物理分配之前存在，变化来源不能替换其固定身份。
#[test]
fn activation_preparation_intent_precedes_allocation_and_retains_original_after_failure() {
    // All effects remain inside the isolated real filesystem fixture.
    // 全部副作用留在隔离真实文件系统夹具内。
    let temp = tempfile::tempdir().expect("fixture");
    // The early owner contains planned identities but no copied candidate.
    // 早期所有者包含计划身份，但没有已复制候选。
    let (store, source, preparation) = early_fixture(temp.path());
    // Keep original identities after dropping observers, without accidentally retaining actual leases.
    // 释放观察者后保留原身份，不意外保留实际租约。
    let request_id = preparation.request_id();
    // The physical path is uniquely fixed by the actual planned identity.
    // 物理路径由实际计划身份唯一固定。
    let identity = preparation.intent().generation().clone();
    assert!(
        !store
            .root
            .join("packages/sample-plugin")
            .join(&identity.generation_id)
            .exists()
    );
    assert!(
        store
            .credential_record("sample-plugin", preparation.intent().credential_revision())
            .expect("credential query")
            .is_none()
    );
    // Query both readiness and identity without taking mutation ownership.
    // 查询就绪与身份，不取得变更所有权。
    let original = store
        .activation_request("sample-plugin", request_id)
        .expect("request query")
        .expect("original");
    assert_eq!(original.phase(), Phase::Preparing);
    assert!(original.target().is_none());
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("pending intent fences writes")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request_id)
            .expect_err("live original")
            .code,
        Code::Busy
    );
    drop(preparation);
    assert_eq!(
        store
            .collect(&identity)
            .expect_err("even unallocated identity belongs to pending intent")
            .code,
        Code::Busy
    );
    fs::write(
        source.join("skills/review-helper/SKILL.md"),
        "---\nname: review-helper\ndescription: Changed source.\n---\nchanged\n",
    )
    .expect("source changes after approval");
    // Explicit recovery owns the same original without copying automatically.
    // 明确恢复持有相同原请求，不自动复制。
    let resumed = store
        .resume_activation_preparation("sample-plugin", request_id)
        .expect("resume original");
    assert!(store.prepare_activation_package(&resumed, &source).is_err());
    assert_eq!(
        store
            .activation_request("sample-plugin", request_id)
            .expect("readback"),
        Some(original)
    );
    assert!(
        !store
            .root
            .join("packages/sample-plugin")
            .join(&identity.generation_id)
            .exists()
    );
    drop(resumed);
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request_id)
            .expect("explicit abandon")
            .phase(),
        Phase::Aborted
    );
    assert_eq!(
        store
            .activation("sample-plugin")
            .expect("predecessor")
            .expect("selected")
            .revision,
        1
    );
}

/// A configured target and actual resource leases emerge only after the immutable copy and parent-bound credentials.
/// 仅在不可变副本及父绑定凭据完成后，才形成已配置目标和实际资源租约。
#[test]
fn activation_preparation_promotes_same_request_and_supports_existing_commit_recovery() {
    // The original journal and copied generation use actual production paths.
    // 原日志及复制代次使用实际生产路径。
    let temp = tempfile::tempdir().expect("fixture");
    // Preparation begins before any candidate materialization.
    // 准备开始于任何候选实体化之前。
    let (store, source, preparation) = early_fixture(temp.path());
    // Preserve original request evidence for the phase transition invariant.
    // 为阶段转换不变量保留原请求证据。
    let request_id = preparation.request_id();
    // This initial record deliberately has no executable target.
    // 此初始记录有意没有可执行目标。
    let original = store
        .activation_request("sample-plugin", request_id)
        .expect("read original")
        .expect("original");
    // Copy into precisely the directory reserved by that original.
    // 复制到该原请求预留的精确目录。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy candidate");
    assert_eq!(Some(package.identity()), original.candidate());
    assert_eq!(
        store
            .begin_credential_staging(
                &package,
                preparation.intent().credential_revision().clone(),
                &VulcanPluginCredentialValues::new()
            )
            .expect_err("reserved revision rejects standalone ownership")
            .code,
        Code::Busy
    );
    // Actual staging carries durable and live parent ownership through backend work.
    // 实际暂存在后端工作期间携带持久及活动父所有权。
    let mut staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("owned staging");
    assert_eq!(staging.record().activation_request(), Some(request_id));
    staging
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("staged");
    drop(staging);
    // Promotion consumes the same real original lock and revalidates the complete physical target.
    // 晋级消费同一实际原锁，并重新校验完整物理目标。
    let transaction = store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("promote");
    // Typed readback explicitly distinguishes the newly configured target from early intent.
    // 类型化回读明确区分新已配置目标与早期意图。
    let ready = store
        .activation_request("sample-plugin", request_id)
        .expect("ready query")
        .expect("ready original");
    assert!(original.preserves_original(&ready));
    assert!(matches!(ready, Request::Activation(_)));
    assert_eq!(ready.phase(), Phase::Prepared);
    assert!(ready.target().is_some());
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request_id)
            .expect_err("configured stage cannot use early abort")
            .code,
        Code::Invalid
    );
    drop(transaction);
    // Restart recovery follows the existing configured activation protocol using the same UUID.
    // 重启恢复按相同 UUID 遵循既有已配置激活协议。
    let mut resumed = store
        .resume_activation_transaction("sample-plugin", request_id)
        .expect("resume configured original");
    store
        .commit_activation_transaction(&mut resumed)
        .expect("publish exact target");
    store
        .complete_activation_transaction(&mut resumed)
        .expect("complete original");
    assert_eq!(
        store
            .activation_request("sample-plugin", request_id)
            .expect("completed")
            .expect("record")
            .phase(),
        Phase::Completed
    );
}

/// A credential writer keeps original ownership after the early foreground owner disappears, including after restart.
/// 早期前台所有者消失后，凭据写入者继续保留原所有权，包括重启后。
#[test]
fn activation_preparation_credential_writer_retains_and_recovers_original_parent() {
    // Empty credentials still exercise real inventory publication, OS file locks and phase transitions.
    // 空凭据仍验证真实清单发布、操作系统文件锁及阶段转换。
    let temp = tempfile::tempdir().expect("fixture");
    // The owner precedes copied bytes and every credential write.
    // 所有者先于复制字节及每次凭据写入。
    let (store, source, preparation) = early_fixture(temp.path());
    // Retain stable identities independently of live original ownership.
    // 独立于活动原所有权保留稳定身份。
    let request_id = preparation.request_id();
    // The durable parent binds this exact revision, not the newest credential inventory.
    // 持久父级绑定此精确修订，而非最新凭据清单。
    let revision = preparation.intent().credential_revision().clone();
    // The real package lease exists before credential inventory derivation.
    // 实际插件包租约在凭据清单派生之前存在。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy");
    // This original writer independently retains parent ownership.
    // 此原写入者独立保留父所有权。
    let staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("staging");
    drop(preparation);
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request_id)
            .expect_err("writer still owns original")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .resume_activation_preparation("sample-plugin", request_id)
            .expect_err("writer excludes parent resumption")
            .code,
        Code::Busy
    );
    drop(staging);
    // A new store object must obtain the parent through the persisted original UUID.
    // 新存储对象必须通过持久原 UUID 取得父级。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen")
        .expect("existing");
    // Explicit revision recovery automatically reacquires its exact declared parent's actual lock.
    // 明确修订恢复自动重新取得精确声明父级的实际锁。
    let mut resumed = reopened
        .resume_credential_staging("sample-plugin", &revision)
        .expect("resume owned credentials");
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request_id)
            .expect_err("recovered writer owns original")
            .code,
        Code::Busy
    );
    resumed
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("finish original staging");
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request_id)
            .expect_err("writer retirement still required")
            .code,
        Code::Busy
    );
    drop(resumed);
    // Actual writer retirement permits recovering and promoting the original preparation.
    // 写入者实际退役后允许恢复及晋级原准备。
    let original = store
        .resume_activation_preparation("sample-plugin", request_id)
        .expect("resume original");
    store
        .finish_activation_preparation(original, VulcanPluginConfigurationSelection::NotDeclared {})
        .expect("promote original");
}

/// Explicit abandonment blocks a persisted credential writer from resuming even after newer requests replace the index.
/// 明确放弃阻止持久凭据写入者恢复，即使较新请求已替换索引。
#[test]
fn activation_preparation_aborted_parent_cannot_resume_credentials() {
    // Real inventory survives both original owner drops and later index replacement.
    // 真实清单跨原所有者释放及后续索引替换存续。
    let temp = tempfile::tempdir().expect("fixture");
    // Create an actual early original and its planned physical resources.
    // 创建实际早期原请求及其计划物理资源。
    let (store, source, preparation) = early_fixture(temp.path());
    // Historical identities remain explicit throughout this test.
    // 此测试全程保持历史身份明确。
    let request_id = preparation.request_id();
    // The exact old revision remains queryable after abandonment.
    // 精确旧修订在放弃之后仍可查询。
    let revision = preparation.intent().credential_revision().clone();
    // Copying precedes the actual credential inventory.
    // 复制先于实际凭据清单。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy");
    drop(
        store
            .begin_activation_credential_staging(
                &preparation,
                &package,
                &VulcanPluginCredentialValues::new(),
            )
            .expect("unfinished writer"),
    );
    drop(preparation);
    store
        .abort_activation_preparation_request("sample-plugin", request_id)
        .expect("explicit abandon");
    assert!(
        store
            .resume_credential_staging("sample-plugin", &revision)
            .is_err()
    );
    // Start a genuinely different request without reusing the old reserved revision.
    // 开始真正不同的请求，不复用旧预留修订。
    let selected = store.activation("sample-plugin").expect("selected");
    // The newer original owns a new immutable allocation and credential revision.
    // 较新原请求拥有新的不可变分配及凭据修订。
    let newer = store
        .begin_activation_preparation(
            Uuid::new_v4(),
            &source,
            &package.identity().content_fingerprint,
            selected.as_ref(),
            vec![],
        )
        .expect("new original");
    assert_ne!(newer.intent().credential_revision(), &revision);
    assert!(
        store
            .resume_credential_staging("sample-plugin", &revision)
            .is_err()
    );
}

/// The startup gate recognizes a preparation-to-activation transition without losing its original obligation.
/// 启动门控识别准备向激活的转换，不丢失其原始义务。
#[test]
fn activation_preparation_startup_gate_tracks_same_original_through_promotion() {
    // The new manager sees an early original already persisted by another process lifetime.
    // 新管理器看到另一进程寿命已持久化的早期原请求。
    let temp = tempfile::tempdir().expect("fixture");
    // All roots remain isolated from installed user plugins.
    // 全部根目录与已安装用户插件隔离。
    let (store, source, preparation) = early_fixture(temp.path());
    // A new manager holds independent process-local startup state.
    // 新管理器持有独立进程内启动状态。
    let runtime = VulcanPluginRuntimeManager::without_runtime(
        temp.path().join("bundled"),
        temp.path().to_path_buf(),
        temp.path().join("bridge"),
        temp.path().join("distributions"),
        temp.path().join("state"),
        temp.path().join("runtime"),
    );
    runtime
        .initialize_activation_recovery()
        .expect("initialize recovery");
    assert_eq!(
        runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("startup catalog")
            .plugins["sample-plugin"]
            .runtime_state,
        VulcanPluginRuntimeState::Unavailable
    );
    // A second startup observer will witness a later invalid target replacement independently.
    // 第二个启动观察者将独立见证后续非法目标替换。
    let changed_observer = VulcanPluginRuntimeManager::without_runtime(
        temp.path().join("bundled"),
        temp.path().to_path_buf(),
        temp.path().join("second-bridge"),
        temp.path().join("distributions"),
        temp.path().join("second-state"),
        temp.path().join("second-runtime"),
    );
    changed_observer
        .initialize_activation_recovery()
        .expect("capture early original independently");
    // Actual package and credential preparation promotes the original without replacing its request identity.
    // 实际插件包及凭据准备晋级原请求，不替换请求身份。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy");
    // The exact parent owns the writer until physical retirement.
    // 精确父级持有写入者，直至物理退役。
    let mut staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("staging");
    staging
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("stage");
    drop(staging);
    // Keep the configured original pending until explicit route acknowledgement.
    // 在明确路由确认之前保持已配置原请求为待完成。
    let mut transaction = store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("promote");
    assert_eq!(
        runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("pending catalog")
            .plugins["sample-plugin"]
            .runtime_state,
        VulcanPluginRuntimeState::Unavailable
    );
    changed_observer
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("observe complete target once");
    // Preserve exact original bytes before injecting a different, structurally valid configuration into that target.
    // 向目标注入不同且结构合法的配置前保留精确原字节。
    let path = record_path(temp.path(), transaction.record().request_id());
    // Restoration later must not clear an observer's latched identity mismatch.
    // 稍后还原不能清除观察者锁存的身份不匹配。
    let original_bytes = fs::read(&path).expect("original bytes");
    // This changes only the configured target, leaving the early frozen resource intent identical.
    // 此处仅改变已配置目标，保持早期固定资源意图完全相同。
    let mut changed: serde_json::Value = serde_json::from_slice(&original_bytes).expect("wire");
    changed["target"]["configuration"] =
        serde_json::to_value(VulcanPluginConfigurationSelection::Document {
            value: serde_json::json!({"unexpected":"replacement"}),
        })
        .expect("different complete configuration");
    fs::write(&path, serde_json::to_vec(&changed).expect("changed bytes"))
        .expect("inject target replacement");
    assert!(
        store
            .activation_request("sample-plugin", transaction.record().request_id())
            .is_ok()
    );
    changed_observer
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("reject changed observed target");
    fs::write(&path, &original_bytes).expect("restore original evidence");
    store
        .commit_activation_transaction(&mut transaction)
        .expect("select");
    store
        .complete_activation_transaction(&mut transaction)
        .expect("acknowledge");
    assert_eq!(
        runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("completed catalog")
            .plugins["sample-plugin"]
            .runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert_eq!(
        changed_observer
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("identity error remains latched")
            .plugins["sample-plugin"]
            .runtime_state,
        VulcanPluginRuntimeState::Unavailable
    );
}

/// Declared schemas reject impossible stage fields, while missing-index recovery requires the exact original.
/// 声明模式拒绝不可能的阶段字段，缺失索引恢复要求精确原请求。
#[test]
fn activation_preparation_validates_wire_shape_and_repairs_only_original_index() {
    // Only this private fixture journal is modified for explicit storage corruption cases.
    // 仅为明确存储损坏场景修改此私有夹具日志。
    let temp = tempfile::tempdir().expect("fixture");
    // The authentic original supplies every frozen identity and predecessor.
    // 真实原请求提供全部固定身份及前驱。
    let (store, _source, preparation) = early_fixture(temp.path());
    // Resolve the exact original path without selecting a record from directory order.
    // 解析精确原路径，不按目录顺序选择记录。
    let request_id = preparation.request_id();
    // The canonical filename is already part of the production record contract.
    // 规范文件名已属于生产记录契约。
    let path = record_path(temp.path(), request_id);
    // Retain real bytes for strict decoder cases and exact restoration.
    // 为严格解码器场景及精确还原保留真实字节。
    let bytes = fs::read(&path).expect("record bytes");
    for (field, value) in [
        ("schema_version", serde_json::json!(1)),
        ("phase", serde_json::json!("prepared")),
        ("target", serde_json::Value::Null),
        ("preparation", serde_json::Value::Null),
    ] {
        // Modify only the explicitly tested wire field, preserving unrelated original evidence.
        // 仅修改明确测试的线格式字段，保留无关原证据。
        let mut malformed: serde_json::Value =
            serde_json::from_slice(&bytes).expect("fixture wire");
        malformed[field] = value;
        fs::write(
            &path,
            serde_json::to_vec(&malformed).expect("malformed fixture"),
        )
        .expect("inject corruption");
        assert!(
            store
                .activation_request("sample-plugin", request_id)
                .is_err(),
            "field: {field}"
        );
    }
    fs::write(&path, &bytes).expect("restore exact original bytes");
    drop(preparation);
    fs::remove_file(path.with_file_name("current")).expect("interrupt index publication");
    assert_eq!(
        store
            .activation_recovery_records("sample-plugin")
            .expect("read original without repair")
            .len(),
        1
    );
    // Resuming this UUID alone explicitly repairs its missing index.
    // 仅恢复此 UUID 明确修复其缺失索引。
    let resumed = store
        .resume_activation_preparation("sample-plugin", request_id)
        .expect("repair original");
    assert!(path.with_file_name("current").is_file());
    drop(resumed);
    store
        .abort_activation_preparation_request("sample-plugin", request_id)
        .expect("abandon");
    assert!(
        store
            .activation_recovery_records("sample-plugin")
            .expect("terminal history")
            .is_empty()
    );
    // Each declared effect sidecar is impossible even after the early original becomes terminal.
    // 即使早期原请求已为终态，每种声明副作用附属回执仍不可能存在。
    for extension in ["install", "effects"] {
        // Isolate each corrupted sidecar so neither decoder masks the other one's coverage.
        // 隔离每个损坏附属文件，使两个解码器不会相互掩盖覆盖。
        let sidecar = path.with_extension(extension);
        fs::write(&sidecar, b"{}").expect("impossible receipt");
        assert!(store.activation_recovery_records("sample-plugin").is_err());
        if extension == "install" {
            assert!(
                store
                    .installation_receipt("sample-plugin", request_id)
                    .is_err()
            );
        } else {
            assert!(
                store
                    .activation_effects("sample-plugin", request_id)
                    .is_err()
            );
        }
        fs::remove_file(sidecar).expect("remove only fixture corruption");
    }
}
