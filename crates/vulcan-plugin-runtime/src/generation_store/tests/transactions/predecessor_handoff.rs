//! Actual stores, immutable pins and exclusive files verify original predecessor handoff retention.
//! 实际存储、不可变固定句柄及排他文件验证原前驱交接保留。

use super::*;
use crate::VulcanPluginPredecessorHandoffPhase as HandoffPhase;

/// Confirm a cross-store writer rejects Pending even after the same original activation is truly Aborted.
/// 确认跨存储写入者在相同原激活真正 Aborted 后仍拒绝 Pending。
#[test]
fn predecessor_handoff_pending_fences_two_store_handles_after_original_abort() {
    // Independent handles share actual disk authority rather than a service-local mutex.
    // 独立句柄共享实际磁盘权威，而非服务局部互斥锁。
    let temp = tempfile::tempdir().expect("create original handoff fixture");
    // Real configured predecessor and target packages have separate immutable files and credential revisions.
    // 真实已配置前驱及目标包具有独立不可变文件和凭据修订。
    let (store, first, second, first_credentials, second_credentials) = fixture(temp.path());
    // The competing manager observes the same canonical selected predecessor.
    // 竞争管理器观测相同规范所选前驱。
    let competing = VulcanPluginGenerationStore::open(temp.path()).expect("open competing store");
    // One original update reserves actual target ownership before private predecessor handoff.
    // 单个原更新在私有前驱交接前预留实际目标所有权。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // Freeze every original field before any abort or sidecar mutation.
    // 在任何中止或附属记录变更前固定每个原字段。
    let original = transaction.record().clone();
    // Independent file ownership preserves the existing unique original request-readiness rule.
    // 独立文件所有权保留既有唯一原请求就绪规则。
    let mut reservation = store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("retain real old resources");
    store
        .persist_predecessor_handoff_reservation(&mut reservation)
        .expect("persist original Pending");
    assert_eq!(
        competing
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect("read pending evidence")
            .expect("pending exists")
            .phase(),
        HandoffPhase::Pending
    );
    assert_eq!(
        competing
            .deactivate("sample-plugin", 1)
            .expect_err("pending fences selection")
            .code,
        Code::Busy
    );
    store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("abort with unique original readiness despite reservation");
    assert_eq!(transaction.record().phase(), Phase::Aborted);
    store
        .validate_aborted_predecessor_handoff_owner(&reservation)
        .expect("same live guard confirms exact original Aborted");
    assert_eq!(
        competing
            .activate_configured(
                &first,
                1,
                vec![],
                first_credentials.clone(),
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("Aborted must not release private handoff fence")
            .code,
        Code::Busy
    );
    assert_eq!(
        competing
            .begin_activation_transaction(
                Uuid::new_v4(),
                &second,
                1,
                vec![],
                second_credentials.clone(),
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("new request must not steal old closed namespace")
            .code,
        Code::Busy
    );
    assert_eq!(
        competing
            .activation_transaction("sample-plugin", original.request_id())
            .expect("original receipt stays readable")
            .expect("original remains")
            .phase(),
        Phase::Aborted
    );
    drop(transaction);
    assert_eq!(
        competing
            .resume_activation_transaction("sample-plugin", original.request_id())
            .expect_err("same UUID cannot recreate private instance ownership")
            .code,
        Code::Busy
    );
    assert_eq!(
        competing
            .abort_activation_request("sample-plugin", original.request_id())
            .expect_err("public original abort cannot clear private handoff")
            .code,
        Code::Busy
    );
    drop(reservation);
    assert_eq!(
        competing
            .deactivate("sample-plugin", 1)
            .expect_err("owner loss leaves durable Pending")
            .code,
        Code::Busy
    );
    assert_eq!(
        competing
            .resume_activation_transaction("sample-plugin", original.request_id())
            .expect_err("crash cannot synthesize original instance")
            .code,
        Code::Busy
    );
    // Central retention checks preserve the exact old authority even without a remaining live reservation guard.
    // 即使不再有存活保留权守卫，集中保留检查仍保护精确旧权威。
    let _namespace = competing
        .namespace_lock("sample-plugin", false)
        .expect("hold central retention fence");
    assert_eq!(
        competing
            .check_predecessor_handoff_package_retention_locked(first.identity())
            .expect_err("Pending journal retains old package")
            .code,
        Code::Busy
    );
    assert_eq!(
        competing
            .check_predecessor_handoff_credential_retention_locked(
                "sample-plugin",
                &first_credentials
            )
            .expect_err("Pending journal retains old credential revision")
            .code,
        Code::Busy
    );
}

/// Reject a full-record mismatch even when UUID, plugin and package identities remain identical.
/// 即使 UUID、插件及包身份相同，也拒绝完整记录不匹配。
#[test]
fn predecessor_handoff_reservation_rejects_wrong_full_original_and_foreign_root() {
    // Use actual managed packages rather than constructing a fabricated original activation record.
    // 使用实际托管包，而非构造伪造原激活记录。
    let temp = tempfile::tempdir().expect("create owner mismatch fixture");
    // Both target and predecessor retain original physical leases through this test.
    // 目标及前驱在此测试期间都保留原物理租约。
    let (store, _first, second, _first_credentials, second_credentials) = fixture(temp.path());
    // The actual owner remains Prepared; a forged same-UUID Committing observation grants no authority.
    // 实际所有者保持 Prepared；伪造相同 UUID 的 Committing 观测不授予权威。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // Complete serialization permits changing exactly one authority field while preserving valid typed shape.
    // 完整序列化允许仅改变单个权威字段，同时保持有效类型形状。
    let mut altered =
        serde_json::to_value(transaction.record()).expect("serialize complete original");
    altered["phase"] = serde_json::json!("committing");
    // Different original phase cannot be normalized to manufacture the initial frozen request authority.
    // 不得通过规范化不同原阶段来制造初始固定请求权威。
    let altered: VulcanPluginActivationTransactionRecord =
        serde_json::from_value(altered).expect("decode typed forged owner");
    assert_eq!(
        store
            .prepare_predecessor_handoff_reservation(&mut transaction, &altered)
            .expect_err("reject wrong full original")
            .code,
        Code::Conflict
    );
    // A second root cannot apply the real owner to an unrelated namespace.
    // 第二个根目录不能将实际所有者用于无关命名空间。
    let foreign_temp = tempfile::tempdir().expect("create foreign root");
    // Opening foreign storage itself grants no original reservation access.
    // 打开外部存储本身不授予原保留权访问。
    let foreign =
        VulcanPluginGenerationStore::open(foreign_temp.path()).expect("open foreign store");
    // Freeze the exact authoritative record after deliberately rejecting the altered one.
    // 明确拒绝变更记录后，固定精确权威记录。
    let original = transaction.record().clone();
    assert_eq!(
        foreign
            .prepare_predecessor_handoff_reservation(&mut transaction, &original)
            .expect_err("reject cross-root owner")
            .code,
        Code::Conflict
    );
    assert!(
        store
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect("read untouched evidence")
            .is_none()
    );
}

/// Lose the real successful Pending write result and immediate readback while retaining the same exclusive file.
/// 丢失真实成功 Pending 写入结果及即时回读，同时保留相同排他文件。
#[test]
fn predecessor_handoff_unknown_write_retains_original_file_and_exact_retry() {
    // A fresh store gives this scoped loss its own original request and actual immutable package inventory.
    // 全新存储为此作用域丢失提供自有原请求及实际不可变包清单。
    let temp = tempfile::tempdir().expect("create unknown sidecar fixture");
    // Real target ownership stays separate from independent predecessor handoff ownership.
    // 真实目标所有权与独立前驱交接所有权保持分离。
    let (store, _first, second, _first_credentials, second_credentials) = fixture(temp.path());
    // Original request files remain owned through both observations.
    // 两次观测期间原请求文件保持自有。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // Retain the complete original before acquiring the independent actual file.
    // 取得独立实际文件前保留完整原记录。
    let original = transaction.record().clone();
    // Caller receives its same actual owner before persistence can return an unknown result.
    // 持久化可返回未知结果之前，调用方取得相同实际所有者。
    let mut reservation = store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("prepare actual original owner");
    // Exact successful real write is the sole controlled loss; canonical public reads remain genuine.
    // 精确成功真实写入是唯一受控丢失；规范公开读取保持真实。
    let _loss = store.lose_predecessor_handoff_observation_once(original.request_id());
    assert_eq!(
        store
            .persist_predecessor_handoff_reservation(&mut reservation)
            .expect_err("unknown immediate observations")
            .code,
        Code::Storage
    );
    assert_eq!(
        store
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect("canonical real disk evidence")
            .expect("real Pending was written")
            .phase(),
        HandoffPhase::Pending
    );
    // Probe the actual canonical independent file through the same production lock primitive.
    // 通过相同生产锁原语探测实际规范独立文件。
    let lease_path = store.root.join("locks").join(format!(
        "sample-plugin.predecessor-handoff.{}.lock",
        original.request_id()
    ));
    assert_eq!(
        files::lock(&lease_path, false)
            .expect_err("unknown retains actual FileBusy")
            .code,
        Code::Busy
    );
    store
        .persist_predecessor_handoff_reservation(&mut reservation)
        .expect("same owner confirms existing exact Pending without overwrite");
    store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("confirm original abort without replay");
    store
        .validate_aborted_predecessor_handoff_owner(&reservation)
        .expect("same owner may prepare actual private resources");
    drop(reservation);
    assert!(
        files::lock(&lease_path, false).is_ok(),
        "only actual owner drop releases actual file"
    );
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("durable Pending survives physical guard loss")
            .code,
        Code::Busy
    );
}

/// A sealed fixture authority transfers only its same real owner and leaves original Aborted bytes unchanged.
/// 密封夹具权威仅移交相同真实所有者，并保持原 Aborted 字节不变。
#[test]
fn predecessor_handoff_sealed_transfer_preserves_aborted_and_exact_physical_retention() {
    // The fixture models storage transfer only; no fake Hook or MCP readiness is declared.
    // 夹具仅模拟存储移交；不声明伪造 Hook 或 MCP 就绪。
    let temp = tempfile::tempdir().expect("create sealed transfer fixture");
    // Old resources remain physically held after Transferred until the same native guard is actually dropped.
    // Transferred 后，旧资源仍被物理持有，直至相同原生守卫实际释放。
    let (store, first, second, first_credentials, second_credentials) = fixture(temp.path());
    // One original request owns the actual target, while reservation owns the actual old package/revision.
    // 单个原请求拥有实际目标，保留权拥有实际旧包及修订。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // Full authority remains immutable even when activation phase advances.
    // 即使激活阶段推进，完整权威也保持不可变。
    let original = transaction.record().clone();
    // This guard is the only possible reservation supplied to the sealed fixture capability.
    // 此守卫是向密封夹具能力提供的唯一可能保留权。
    let mut reservation = store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("prepare transfer owner");
    store
        .persist_predecessor_handoff_reservation(&mut reservation)
        .expect("persist Pending transfer");
    store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("persist real original Aborted");
    // Compare exact actual journal bytes, rather than merely checking its phase.
    // 比较精确实际日志字节，而非仅检查阶段。
    let aborted_bytes = fs::read(record_path(temp.path(), original.request_id()))
        .expect("read original Aborted bytes");
    // Only cfg(test) constructs this capability; production currently has no callable constructor.
    // 仅 cfg(test) 构造此能力；生产当前没有可调用构造器。
    // Bound borrowed transfer authority lexically; only the returned real resource pins survive this scope.
    // 以词法作用域约束借用转交权威；仅返回的真实资源固定句柄跨此作用域存续。
    let (resource_package, resource_credentials) = {
        let authority = store.predecessor_handoff_transfer_test_authority(&reservation);
        store
            .complete_predecessor_handoff_owned(&authority)
            .expect("confirm sealed transfer");
        store
            .complete_predecessor_handoff_owned(&authority)
            .expect("same authority confirms exact prior transfer");
        assert_eq!(
            fs::read(record_path(temp.path(), original.request_id()))
                .expect("read unchanged Aborted bytes"),
            aborted_bytes
        );
        assert_eq!(
            store
                .predecessor_handoff_record("sample-plugin", original.request_id())
                .expect("read final original handoff")
                .expect("historical handoff remains")
                .phase(),
            HandoffPhase::Transferred
        );
        assert_eq!(
            store
                .deactivate("sample-plugin", 1)
                .expect_err("actual original guard must release before successor writes")
                .code,
            Code::Busy
        );
        // Actual receiving resource ownership is represented by real shared readers rather than string identities.
        // 实际接收资源所有权由真实共享读者表示，而非字符串身份。
        store.predecessor_handoff_test_resource_pins(&reservation)
    };
    drop(reservation);
    // Release the terminal original request file while both receiving resource readers remain actually alive.
    // 实际接收资源两个读者仍存活时，释放终态原请求文件。
    drop(transaction);
    // Selecting another real generation is now allowed, but does not release either old physical reader.
    // 现在允许选择另一个真实代次，但不会释放任一旧物理读者。
    store
        .activate_configured(
            &second,
            1,
            vec![],
            second_credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("later exact selection after actual transfer");
    assert_eq!(
        store
            .collect(first.identity())
            .expect_err("old physical package is still pinned")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .collect_credentials("sample-plugin", &first_credentials, &EmptyCredentialBackend)
            .expect_err("old real credential reader remains")
            .code,
        Code::Busy
    );
    drop(first);
    drop(resource_package);
    drop(resource_credentials);
    assert!(
        store
            .collect(
                original
                    .predecessor()
                    .expect("original old selection")
                    .generation()
                    .expect("managed old generation")
            )
            .expect("old package becomes collectible after actual last pin release")
    );
    assert!(
        store
            .collect_credentials("sample-plugin", &first_credentials, &EmptyCredentialBackend)
            .expect("old credentials collectible after actual last pin release")
    );
    assert_eq!(
        store
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect("read historical transfer after later selection")
            .expect("historical exact original remains")
            .phase(),
        HandoffPhase::Transferred
    );
}

/// Unknown Transferred observation keeps its original live file and only exact same-owner confirmation succeeds.
/// 未知 Transferred 观测保留原存活文件，仅精确相同所有者确认成功。
#[test]
fn predecessor_handoff_unknown_transfer_requires_same_guard_and_strict_full_record() {
    // A genuine original Aborted record stays independent from the actual successful sidecar transfer.
    // 真实原 Aborted 记录独立于实际成功附属移交。
    let temp = tempfile::tempdir().expect("create unknown transfer fixture");
    // Two immutable packages isolate old handoff ownership from target readiness ownership.
    // 两个不可变包隔离旧交接所有权与目标就绪所有权。
    let (store, _first, second, _first_credentials, second_credentials) = fixture(temp.path());
    // Original UUID cannot be replaced by a new request or a reconstructed instance.
    // 新请求或重建实例不能替代原 UUID。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // The unchanged complete original is retained by both actual owner types.
    // 两个实际所有者类型都保留未变完整原记录。
    let original = transaction.record().clone();
    // Physical reservation remains alive through the real successful write and deliberately missing observations.
    // 物理保留权在真实成功写入及故意缺失观测期间保持存活。
    let mut reservation = store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("prepare exact original reservation");
    store
        .persist_predecessor_handoff_reservation(&mut reservation)
        .expect("persist original Pending");
    store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("confirm actual abort");
    // Test-only capability is never a public release-by-UUID API.
    // 仅测试能力绝不是按 UUID 释放的公开 API。
    let authority = store.predecessor_handoff_transfer_test_authority(&reservation);
    // Scoped failure affects only immediate transfer acknowledgement, never canonical real disk bytes.
    // 作用域失败仅影响即时移交确认，绝不影响规范真实磁盘字节。
    let _loss = store.lose_predecessor_handoff_observation_once(original.request_id());
    assert_eq!(
        store
            .complete_predecessor_handoff_owned(&authority)
            .expect_err("transfer observation remains unknown")
            .code,
        Code::Storage
    );
    assert_eq!(
        store
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect("inspect actual transfer")
            .expect("actual transfer exists")
            .phase(),
        HandoffPhase::Transferred
    );
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("unknown transfer keeps actual independent writer fence")
            .code,
        Code::Busy
    );
    store
        .complete_predecessor_handoff_owned(&authority)
        .expect("same guard confirms exact Transferred");
    // Deliberately changing one full-original authority field must reject even when its UUID is unchanged.
    // 即使 UUID 未变，故意改变一个完整原权威字段也必须拒绝。
    let path = store
        .root
        .join("transactions/sample-plugin/predecessor-handoffs")
        .join(format!("{}.json", original.request_id()));
    // The fixture changes a non-secret phase field, leaving every resource identifier unchanged.
    // 夹具改变非秘密阶段字段，保持每个资源标识未变。
    let mut altered: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read strict sidecar"))
            .expect("decode strict sidecar fixture");
    altered["original"]["phase"] = serde_json::json!("aborted");
    fs::write(
        path,
        serde_json::to_vec(&altered).expect("encode altered sidecar"),
    )
    .expect("write altered full owner");
    assert_eq!(
        store
            .complete_predecessor_handoff_owned(&authority)
            .expect_err("changed full original cannot confirm")
            .code,
        Code::Invalid
    );
}

/// Unknown schema or fields fail closed rather than allowing a future writer to overlook a Pending original.
/// 未知模式或字段保持关闭，避免未来写入者忽略 Pending 原请求。
#[test]
fn predecessor_handoff_strict_schema_and_inventory_reject_corruption() {
    // Strict sidecar parsing uses real original record bytes and the existing namespace fence.
    // 严格附属记录解析使用真实原记录字节及既有命名空间屏障。
    let temp = tempfile::tempdir().expect("create strict sidecar fixture");
    // Actual two-package fixture provides full immutable managed predecessor authority.
    // 实际双包夹具提供完整不可变托管前驱权威。
    let (store, _first, second, _first_credentials, second_credentials) = fixture(temp.path());
    // Original live target owner stays unchanged during all read-only validation failures.
    // 所有只读校验失败期间，原存活目标所有者保持不变。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // Full original includes every nested selection and preparation field.
    // 完整原记录包含每个嵌套选择及准备字段。
    let original = transaction.record().clone();
    // Same real reservation remains owned despite deliberately malformed persisted evidence.
    // 即使持久证据被故意损坏，相同真实保留权仍保持自有。
    let mut reservation = store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("prepare strict owner");
    store
        .persist_predecessor_handoff_reservation(&mut reservation)
        .expect("write genuine strict original");
    store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("terminal original must not mask malformed private handoff evidence");
    // Literal path belongs to the one documented strict original sidecar inventory.
    // 字面路径属于唯一声明严格原附属清单。
    let path = store
        .root
        .join("transactions/sample-plugin/predecessor-handoffs")
        .join(format!("{}.json", original.request_id()));
    // Preserve canonical bytes so each corruption case is independent.
    // 保留规范字节，使每种损坏情况相互独立。
    let canonical = fs::read(&path).expect("read canonical original reservation");
    // Only explicitly supported independent schemas may authorize a transfer observation.
    // 仅明确支持的独立模式能授权移交观测。
    let mut unknown_schema: serde_json::Value =
        serde_json::from_slice(&canonical).expect("decode schema fixture");
    unknown_schema["schema_version"] = serde_json::json!(2);
    fs::write(
        &path,
        serde_json::to_vec(&unknown_schema).expect("encode unknown schema"),
    )
    .expect("persist unknown schema fixture");
    assert_eq!(
        store
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect_err("unknown schema fails closed")
            .code,
        Code::Invalid
    );
    // Denied fields cannot be silently ignored by read-only status or centralized writer fencing.
    // 只读状态或集中写入屏障不能静默忽略被拒绝字段。
    let mut extra_field: serde_json::Value =
        serde_json::from_slice(&canonical).expect("decode unknown field fixture");
    extra_field["completed_by_uuid"] = serde_json::json!(original.request_id());
    fs::write(
        &path,
        serde_json::to_vec(&extra_field).expect("encode unknown field"),
    )
    .expect("persist unknown field fixture");
    assert_eq!(
        store
            .predecessor_handoff_record("sample-plugin", original.request_id())
            .expect_err("unknown completion field fails closed")
            .code,
        Code::Invalid
    );
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("writer cannot bypass corrupted original reservation")
            .code,
        Code::Invalid
    );
    fs::write(&path, &canonical).expect("restore exact real record for independent filename check");
    // Arbitrary files in this dedicated strict inventory cannot be mistaken for unrelated transactions.
    // 此专用严格清单中的任意文件不能被误判为无关事务。
    let unexpected = path.parent().expect("sidecar parent").join("foreign.json");
    fs::write(unexpected, b"{}").expect("persist noncanonical inventory fixture");
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("noncanonical inventory fails closed")
            .code,
        Code::Invalid
    );
}

/// One read-only admission rule follows actual Pending and unknown transfer ownership across two store handles.
/// 单个只读入场规则跨两个存储句柄追踪实际 Pending 及未知移交所有权。
#[test]
fn predecessor_handoff_read_only_admission_tracks_unknown_transfer_and_real_retirement() {
    // Actual native stores share immutable original authority and actual OS file locks.
    // 实际原生存储共享不可变原权威及实际操作系统文件锁。
    let temp = tempfile::tempdir().expect("create read-only admission fixture");
    // Keep two physical packages through the full admission transition rather than rebuilding an old descriptor.
    // 在完整入场转换期间保留两个物理包，而非重建旧描述符。
    let (store, _first, second, _first_credentials, second_credentials) = fixture(temp.path());
    // Another handle exercises the same centralized namespace decision independently of service locking.
    // 另一句柄独立于服务锁执行相同集中命名空间决策。
    let competing =
        VulcanPluginGenerationStore::open(temp.path()).expect("open second admission observer");
    assert!(
        !competing
            .predecessor_handoff_blocks_admission("sample-plugin")
            .expect("no handoff evidence")
    );
    assert!(
        !competing
            .predecessor_handoff_blocks_admission("absent-plugin")
            .expect("missing namespace is read-only false")
    );
    assert!(
        !store.root.join("locks/absent-plugin.lock").exists(),
        "read-only absence must not create a namespace File"
    );
    assert!(
        !store
            .root
            .join("transactions/sample-plugin/predecessor-handoffs")
            .exists(),
        "read-only absence must not create sidecar directory"
    );
    // Actual original update holds the sole request readiness lease throughout abort staging.
    // 实际原更新在中止暂存期间持有唯一请求就绪租约。
    let mut transaction = begin(&store, &second, &second_credentials, Uuid::new_v4());
    // Original full request remains fixed independently of local transfer observations.
    // 原完整请求独立于局部移交观测保持固定。
    let original = transaction.record().clone();
    // Allocate physical old authority before writing its persistent Pending evidence.
    // 写入持久 Pending 证据前分配物理旧权威。
    let mut reservation = store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("prepare actual admission reservation");
    store
        .persist_predecessor_handoff_reservation(&mut reservation)
        .expect("persist actual Pending admission fence");
    assert!(
        competing
            .predecessor_handoff_blocks_admission("sample-plugin")
            .expect("Pending blocks admission")
    );
    store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("same original abort before private transfer");
    // Only this test constructor substitutes for actual composite resource guard authority.
    // 仅此测试构造器代替实际组合资源守卫权威。
    // Bound borrowed authority lexically while retaining both real resource pins and the original observation scope until test exit.
    // 以词法作用域约束借用权威，同时将真实资源固定句柄及原观测作用域保留至测试退出。
    let (_loss, (_receiving_package, _receiving_credentials)) = {
        let authority = store.predecessor_handoff_transfer_test_authority(&reservation);
        // The original owner remains physically retained when both local completion observations disappear.
        // 两个局部完成观测消失时，原所有者仍被物理保留。
        let _loss = store.lose_predecessor_handoff_observation_once(original.request_id());
        assert_eq!(
            store
                .complete_predecessor_handoff_owned(&authority)
                .expect_err("real Transferred but unknown local acknowledgement")
                .code,
            Code::Storage
        );
        assert!(
            competing
                .predecessor_handoff_blocks_admission("sample-plugin")
                .expect("Transferred real lease still Busy")
        );
        store
            .complete_predecessor_handoff_owned(&authority)
            .expect("same owner confirms exact real Transferred");
        assert!(
            competing
                .predecessor_handoff_blocks_admission("sample-plugin")
                .expect("confirmation alone does not fabricate owner retirement")
        );
        // Actual receiving readers retain old resources after the independent reservation lease is retired.
        // 独立保留权租约退役后，实际接收读者保留旧资源。
        (
            _loss,
            store.predecessor_handoff_test_resource_pins(&reservation),
        )
    };
    drop(reservation);
    assert!(
        !competing
            .predecessor_handoff_blocks_admission("sample-plugin")
            .expect("exact confirmed transfer and real lease retirement allow admission")
    );
    // Missing known lease is explicit corruption, never an opportunity for a read-only query to recreate it.
    // 已知租约缺失是显式损坏，绝不是只读查询重建它的机会。
    let lease_path = store.root.join("locks").join(format!(
        "sample-plugin.predecessor-handoff.{}.lock",
        original.request_id()
    ));
    fs::remove_file(&lease_path).expect("remove known retired lease fixture");
    assert_eq!(
        competing
            .predecessor_handoff_blocks_admission("sample-plugin")
            .expect_err("known record without actual original File is invalid")
            .code,
        Code::Invalid
    );
    assert!(
        !lease_path.exists(),
        "read-only corruption handling must not create a replacement File"
    );
}
