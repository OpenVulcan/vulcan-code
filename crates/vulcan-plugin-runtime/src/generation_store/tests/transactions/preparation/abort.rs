//! Actual original Files and atomic Aborted writers verify retained early abandonment without resource cleanup or replay.
//! 实际原 File 及原子 Aborted 写入器验证保留式早期放弃，不清理资源或重放操作。

use super::*;

/// Try a real competing lock through the sole original request path resolver; assert the retained actual File remains exclusive.
/// 通过唯一原请求路径解析器尝试真实竞争锁；断言保留实际 File 仍排他。
/// The competing handle opens an existing file only and is immediately retired; it never creates or replaces ownership.
/// 竞争句柄仅打开既有文件并立即退役；绝不创建或替换所有权。
fn assert_request_busy(store: &VulcanPluginGenerationStore, request: Uuid) {
    // This production path resolver is visible to generation-store tests and does not acquire a File.
    // 此生产路径解析器对代际存储测试可见，且不取得 File。
    let path = store.transaction_lease_path("sample-plugin", request);
    // No create/truncate option can manufacture a lease to make this assertion pass.
    // 不提供创建、截断选项，不能伪造租约使断言通过。
    let competitor = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("existing actual request file");
    assert!(
        matches!(
            competitor.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ),
        "the same actual original request File must remain held"
    );
}

/// Snapshot this original's actual journal/index bytes and modification times, excluding its still-locked request file.
/// 捕获此原请求实际日志、索引字节及修改时间，排除其仍锁定请求文件。
/// Return exact durable evidence for read-only retry and rejection comparisons.
/// 返回精确持久证据，用于只读重试及拒绝比较。
fn checkpoint(
    root: &Path,
    request: Uuid,
) -> (
    Vec<u8>,
    std::time::SystemTime,
    Vec<u8>,
    std::time::SystemTime,
) {
    // The existing unique fixture resolver determines the original's actual journal path.
    // 既有唯一夹具解析器确定原请求实际日志路径。
    let path = record_path(root, request);
    // The declared index lives beside this original rather than at an inferred newest journal.
    // 声明索引位于此原请求旁边，而非推导的最新日志处。
    let index = path.with_file_name("current");
    (
        fs::read(&path).expect("journal bytes"),
        fs::metadata(&path)
            .expect("journal metadata")
            .modified()
            .expect("journal time"),
        fs::read(&index).expect("index bytes"),
        fs::metadata(&index)
            .expect("index metadata")
            .modified()
            .expect("index time"),
    )
}

/// Keep the original journal open without Windows deletion sharing; return its actual read handle.
/// 不授予 Windows 删除共享地保持原日志打开；返回实际读取句柄。
/// Atomic replacement must fail while a correct read-only confirmation succeeds.
/// 原子替换须失败，而正确只读确认须成功。
fn block_replacement(path: &Path) -> std::fs::File {
    // This bounded blocker never opens or releases the actual original request File.
    // 此有界阻挡器绝不打开或释放实际原请求 File。
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1 | 2);
    }
    options.open(path).expect("pin actual original journal")
}

/// Prepare the original's actual package while retaining its early request owner and incomplete credentials.
/// 准备原请求实际插件包，同时保留其早期请求所有者及未完成凭据。
/// Return production store/preparation/package owners, without creating a configured transaction.
/// 返回生产存储、准备、插件包所有者，不创建已配置事务。
fn package_fixture(
    root: &Path,
) -> (
    VulcanPluginGenerationStore,
    VulcanPluginActivationPreparation,
    PinnedVulcanPluginGeneration,
) {
    // The real fixture provides a selected predecessor and actual early journal.
    // 真实夹具提供已选前驱及实际早期日志。
    let (store, source, preparation) = early_fixture(root);
    // The physical package uses this original's exact planned generation identity.
    // 物理插件包使用此原请求精确计划代际身份。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("actual original package");
    (store, preparation, package)
}

/// Reserved publication transfers its same original owner before borrowed abandonment, without an unlock gap.
/// 预留发布在借用放弃之前移交同一原所有者，不产生解锁间隙。
#[test]
fn preparation_abort_retained_preserves_reserved_handoff_and_terminal_fence() {
    // Every lease belongs to the actual isolated fixture namespace.
    // 每个租约均属于实际隔离夹具命名空间。
    let temp = tempfile::tempdir().expect("fixture");
    // Real empty selection supplies the original unmanaged predecessor.
    // 真实空选择提供原无受管前驱。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("store");
    // Exact inspected source fixes the original fingerprint.
    // 精确检查来源固定原指纹。
    let source = write_package(&temp.path().join("source"), "sample-plugin", "new");
    // The source descriptor is actual production inspection output.
    // 来源描述符来自实际生产检查输出。
    let descriptor = crate::inspect_user_plugin_package(&source).expect("source");
    // Reservation creates the original request File before actual journal publication.
    // 预留在实际日志发布之前创建原请求 File。
    let mut reservation = store
        .reserve_activation_update_preparation(
            Uuid::new_v4(),
            &source,
            &descriptor.content_fingerprint,
            None,
            vec![],
            crate::VulcanPluginUpdatePolicy {
                explicitly_enabled: false,
                explicitly_disabled: true,
                predecessor_default_enabled: true,
            },
        )
        .expect("reserved original");
    // Publication shares the actual File while the reservation still owns its namespace.
    // 预留仍持有命名空间时，发布共享实际 File。
    let mut preparation = store
        .publish_activation_preparation_retained(&mut reservation)
        .expect("actual publication");
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("reservation must retire")
            .code,
        Code::Busy
    );
    drop(reservation);
    // This canonical identity remains stable before and after retained terminal publication.
    // 此规范身份在保留式终态发布前后保持稳定。
    let request = preparation.request_id();
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect("borrowed original abort")
            .phase(),
        Phase::Aborted
    );
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request)
            .expect_err("same request File still held")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .begin_activation_preparation(
                Uuid::new_v4(),
                &source,
                &descriptor.content_fingerprint,
                None,
                vec![]
            )
            .expect_err("terminal actual owner still fences successor")
            .code,
        Code::Busy
    );
    drop(preparation);
    assert_eq!(
        store
            .abort_activation_preparation_request("sample-plugin", request)
            .expect("real retirement permits competition")
            .phase(),
        Phase::Aborted
    );
    assert!(
        store
            .begin_activation_preparation(
                Uuid::new_v4(),
                &source,
                &descriptor.content_fingerprint,
                None,
                vec![]
            )
            .is_ok()
    );
}

/// Actual credential reservation and its completed inner writer both prevent abandonment until the original shared File retires.
/// 实际凭据预留及其已完成内部写入者均阻止放弃，直到原共享 File 退役。
#[test]
fn preparation_abort_retained_waits_for_actual_credential_reservation_and_writer() {
    // Real physical package and credential revision are separate from the early request owner.
    // 真实物理插件包及凭据修订独立于早期请求所有者。
    let temp = tempfile::tempdir().expect("fixture");
    // The same original remains borrowed while all secure work is observed.
    // 观察全部安全工作期间始终借用同一原请求。
    let (store, mut preparation, package) = package_fixture(temp.path());
    // Save exact revision identity before terminal selection to verify no credential cleanup occurs.
    // 在终态选择之前保存精确修订身份，验证未发生凭据清理。
    let revision = preparation.intent().credential_revision().clone();
    // Empty real values exercise owner retention without introducing user secrets.
    // 空真实值验证所有者保留，不引入用户秘密。
    let values = VulcanPluginCredentialValues::new();
    // The actual reservation retains the same parent File before any inventory write.
    // 实际预留在任何清单写入之前保留同一父 File。
    let mut reservation = store
        .reserve_activation_credential_staging(&preparation, &package, &values)
        .expect("credential reservation");
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("live credential reservation")
            .code,
        Code::Busy
    );
    store
        .publish_activation_credential_staging_retained(&mut reservation, &preparation)
        .expect("actual inner writer")
        .write_and_finish(&EmptyCredentialBackend, &values)
        .expect("actual credentials completed");
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("finished original writer still lives")
            .code,
        Code::Busy
    );
    drop(reservation);
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect("actual writer retired")
            .phase(),
        Phase::Aborted
    );
    assert!(
        package.package_root().exists(),
        "abandonment retains actual package bytes"
    );
    assert!(
        store
            .credential_record("sample-plugin", &revision)
            .expect("actual credential inventory")
            .is_some(),
        "abandonment retains secure inventory"
    );
}

/// Real Aborted writer acknowledgement loss and panic confirm through the same original owner without any second write.
/// 真实 Aborted 写入器确认丢失及 panic 通过同一原所有者确认，不发生第二次写入。
#[test]
fn preparation_abort_retained_confirms_unknown_real_writer_readonly() {
    for panic in [false, true] {
        // Both failure modes use independent actual original journals and request files.
        // 两种失败模式使用独立实际原日志及请求文件。
        let temp = tempfile::tempdir().expect("fixture");
        // Local last-confirmed Preparing remains owned across worker unwinding.
        // 本地最后确认 Preparing 跨工作器展开保持被持有。
        let (store, _source, mut preparation) = early_fixture(temp.path());
        // The original UUID binds the fault and all later read-only evidence.
        // 原 UUID 绑定故障及全部后续只读证据。
        let request = preparation.request_id();
        // This hook loses acknowledgement only after the real atomic Aborted writer succeeds.
        // 此钩子仅在真实原子 Aborted 写入器成功后丢失确认。
        let _fault = store.observe_preparation_abort_writes_for_test(request, Some(panic));
        // The mutable original lives outside the panicking worker closure.
        // 可变原请求存活于 panic 工作器闭包之外。
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.abort_activation_preparation_retained(&mut preparation)
        }));
        if panic {
            assert!(outcome.is_err());
        } else {
            assert_eq!(
                outcome
                    .expect("returned Err")
                    .expect_err("lost writer acknowledgement")
                    .code,
                Code::Storage
            );
        }
        assert_eq!(
            store
                .activation_request("sample-plugin", request)
                .expect("actual original read")
                .expect("true durable original")
                .phase(),
            Phase::Aborted
        );
        assert_eq!(store.preparation_abort_write_count_for_test(request), 1);
        assert_request_busy(&store, request);
        // Pinning the actual durable journal makes any attempted Windows replacement fail.
        // 固定实际持久日志，使任何 Windows 替换尝试失败。
        let _blocker = block_replacement(&record_path(temp.path(), request));
        // Exact successful bytes and timestamps must survive confirmation unchanged.
        // 精确成功字节及时间戳须跨确认保持不变。
        let before = checkpoint(temp.path(), request);
        assert_eq!(
            store
                .abort_activation_preparation_retained(&mut preparation)
                .expect("read-only original confirmation")
                .phase(),
            Phase::Aborted
        );
        assert_eq!(
            store
                .abort_activation_preparation_retained(&mut preparation)
                .expect("exact local Aborted retry")
                .phase(),
            Phase::Aborted
        );
        assert_eq!(store.preparation_abort_write_count_for_test(request), 1);
        assert_eq!(checkpoint(temp.path(), request), before);
    }
}

/// Changed complete authority cannot be repaired or replaced while the actual early owner remains borrowed.
/// 实际早期所有者仍被借用时，变化的完整权威不能被修复或替换。
#[test]
fn preparation_abort_retained_rejects_changed_original_authority() {
    for corruption in [
        "store",
        "missing",
        "index",
        "selection",
        "intent",
        "predecessor",
        "schema",
    ] {
        // Each corruption belongs to a separate real namespace and immutable original.
        // 每种损坏均属于独立真实命名空间及不可变原请求。
        let temp = tempfile::tempdir().expect("fixture");
        // Real Preparing bytes are generated before controlled fixture corruption.
        // 真实 Preparing 字节在受控夹具损坏之前生成。
        let (store, _source, mut preparation) = early_fixture(temp.path());
        // Original identity alone selects its journal path.
        // 仅原身份选择其日志路径。
        let request = preparation.request_id();
        // Actual original bytes define all schema-valid replacement fixtures.
        // 实际原字节定义全部模式有效替代夹具。
        let path = record_path(temp.path(), request);
        if corruption == "store" {
            // The actual foreign store cannot claim the original simply because its plugin namespace name matches.
            // 实际外来存储不能仅因插件命名空间名称匹配便声称持有原请求。
            let foreign = tempfile::tempdir().expect("foreign fixture");
            // Opening this root allocates no ownership in the original store.
            // 打开此根不在原存储分配所有权。
            let other = VulcanPluginGenerationStore::open(foreign.path()).expect("foreign store");
            assert_eq!(
                other
                    .abort_activation_preparation_retained(&mut preparation)
                    .expect_err("foreign binding")
                    .code,
                Code::Conflict
            );
        } else {
            match corruption {
                "missing" => {
                    fs::remove_file(&path).expect("remove actual journal fixture");
                }
                "index" => {
                    fs::remove_file(path.with_file_name("current"))
                        .expect("remove actual index fixture");
                }
                "selection" => {
                    fs::remove_file(store.root.join("active/sample-plugin.json"))
                        .expect("remove actual selected fixture");
                }
                "intent" | "predecessor" | "schema" => {
                    // Mutate exact original bytes, rather than asking the implementation to infer alternate fields.
                    // 修改精确原字节，而不是要求实现推导替代字段。
                    let mut wire: serde_json::Value =
                        serde_json::from_slice(&fs::read(&path).expect("original bytes"))
                            .expect("wire");
                    if corruption == "intent" {
                        wire["preparation"]["credential_revision"] =
                            serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                                .expect("valid replacement revision");
                    } else if corruption == "predecessor" {
                        wire["predecessor"]["settings"]["configuration"] =
                            serde_json::to_value(VulcanPluginConfigurationSelection::Document {
                                value: serde_json::Value::Null,
                            })
                            .expect("valid changed predecessor configuration");
                    } else {
                        wire["schema_version"] = serde_json::json!(u32::MAX);
                    }
                    fs::write(&path, serde_json::to_vec(&wire).expect("replacement bytes"))
                        .expect("actual fixture corruption");
                }
                _ => unreachable!("declared corruptions"),
            }
            assert!(
                store
                    .abort_activation_preparation_retained(&mut preparation)
                    .is_err(),
                "{corruption} cannot authorize abandonment"
            );
        }
        assert_request_busy(&store, request);
        if corruption == "missing" {
            assert!(!path.exists(), "no original journal repair");
        }
        if corruption == "index" {
            assert!(!path.with_file_name("current").exists(), "no index repair");
        }
    }
}

/// An actual different terminal index cannot replace this original, even though both requests belong to one plugin.
/// 即使两个请求属于同一插件，实际不同终态索引也不能替换此原请求。
#[test]
fn preparation_abort_retained_rejects_foreign_terminal_index() {
    // Real terminal history gives the foreign pointer a valid original body.
    // 真实终态历史赋予外来指针有效原正文。
    let temp = tempfile::tempdir().expect("fixture");
    // First retain only the old actual identity before retiring its exclusive owner.
    // 首先仅保留旧实际身份，再退役其排他所有者。
    let (store, source, early) = early_fixture(temp.path());
    // The old terminal UUID is never guessed from enumeration.
    // 旧终态 UUID 绝不从枚举猜测。
    let old = early.request_id();
    drop(early);
    store
        .abort_activation_preparation_request("sample-plugin", old)
        .expect("actual terminal history");
    // These exact bytes are the real earlier index, not a synthesized pointer.
    // 此精确字节属于真实较早索引，而非合成指针。
    let old_index = fs::read(record_path(temp.path(), old).with_file_name("current"))
        .expect("actual old index");
    // Capture the unchanged current selection for the next real early original.
    // 捕获下一个真实早期原请求的未变当前选择。
    let selected = store.activation("sample-plugin").expect("selected");
    // Real source inspection provides the frozen fingerprint.
    // 真实来源检查提供固定指纹。
    let fingerprint = crate::inspect_user_plugin_package(&source)
        .expect("source")
        .content_fingerprint;
    // Actual new original creates its own request File and journal.
    // 实际新原请求创建自身请求 File 及日志。
    let mut preparation = store
        .begin_activation_preparation(
            Uuid::new_v4(),
            &source,
            &fingerprint,
            selected.as_ref(),
            vec![],
        )
        .expect("new original");
    fs::write(
        record_path(temp.path(), preparation.request_id()).with_file_name("current"),
        old_index,
    )
    .expect("restore only fixture foreign index");
    // No rejecting call can repair the foreign pointer or change the new original body.
    // 任何拒绝调用均不能修复外来指针或改变新原正文。
    let before = checkpoint(temp.path(), preparation.request_id());
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("actual foreign terminal pointer")
            .code,
        Code::Conflict
    );
    assert_eq!(checkpoint(temp.path(), preparation.request_id()), before);
}

/// Exact local Aborted cannot accept a disk reversal, and a promoted actual configured owner cannot return to early abandonment.
/// 精确本地 Aborted 不能接受磁盘逆退，已晋级实际配置所有者不能返回早期放弃。
#[test]
fn preparation_abort_retained_rejects_phase_reversal_and_promoted_owner() {
    // An actual retained writer establishes the local and durable Aborted checkpoint.
    // 实际保留式写入器建立本地及持久 Aborted 检查点。
    let temp = tempfile::tempdir().expect("fixture");
    // Keep the same immutable early intent while testing a deliberate reverse-phase corruption.
    // 测试明确逆向阶段损坏时保留同一不可变早期意图。
    let (store, _source, mut preparation) = early_fixture(temp.path());
    store
        .abort_activation_preparation_retained(&mut preparation)
        .expect("actual retained abort");
    // Actual terminal bytes are the only source of the reverse-phase corruption fixture.
    // 实际终态字节是逆向阶段损坏夹具的唯一来源。
    let path = record_path(temp.path(), preparation.request_id());
    // A manual reverse phase is corruption only, never the test's evidence of a real Aborted write.
    // 手动逆向阶段仅属于损坏，绝不充当真实 Aborted 写入测试证据。
    let mut body: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("Aborted bytes")).expect("wire");
    body["phase"] = serde_json::json!("preparing");
    fs::write(&path, serde_json::to_vec(&body).expect("reversed wire"))
        .expect("fixture phase reversal");
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("local Aborted cannot regress")
            .code,
        Code::Conflict
    );
    // A separate complete real candidate tests the actual same-File promotion transfer window.
    // 独立完整真实候选验证实际同 File 晋级移交窗口。
    let promoted = tempfile::tempdir().expect("promoted fixture");
    // Physical package and credential writer use the true original parent.
    // 物理插件包及凭据写入者使用真实原父级。
    let (store, mut preparation, package) = package_fixture(promoted.path());
    // Retire the actual completed credential writer before promotion can share its original File.
    // 在晋级可以共享原 File 之前退役实际已完成凭据写入者。
    let mut staging = store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("writer");
    staging
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("actual complete credentials");
    drop(staging);
    // The actual returned configured transaction still shares the original request File with preparation.
    // 实际返回配置事务仍与准备对象共享原请求 File。
    let transaction = store
        .finish_activation_preparation_retained(
            &mut preparation,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("actual promotion");
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("configured owner still shares File")
            .code,
        Code::Busy
    );
    drop(transaction);
    assert_eq!(
        store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("actual Prepared cannot become early Aborted")
            .code,
        Code::Conflict
    );
}

/// Early execution sidecars and ASCII aliases use sealed inspection's unique canonical inventory and block every retained write.
/// 早期执行附属文件及 ASCII 别名使用密封检查唯一规范清单，阻止全部保留式写入。
#[test]
fn preparation_abort_retained_rejects_actual_sidecars_and_ascii_aliases() {
    for extension in [
        "effects",
        "install",
        "target-hook",
        "target-mcp",
        "EFFECTS",
        "INSTALL",
        "TARGET-HOOK",
        "TARGET-MCP",
    ] {
        // Each declared sidecar case owns an independent actual namespace and request File.
        // 每个声明附属用例持有独立实际命名空间及请求 File。
        let temp = tempfile::tempdir().expect("fixture");
        // Genuine early journal publication precedes corrupt impossible sidecar injection.
        // 真实早期日志发布先于不可能附属损坏注入。
        let (store, _source, mut preparation) = early_fixture(temp.path());
        // The canonical actual original UUID fixes the sole sidecar location.
        // 规范实际原 UUID 固定唯一附属位置。
        let path = record_path(temp.path(), preparation.request_id()).with_extension(extension);
        fs::write(&path, b"{}").expect("actual impossible early sidecar");
        // Durable bytes and timestamps remain unchanged after strict rejection.
        // 严格拒绝之后，持久字节及时间戳保持不变。
        let before = checkpoint(temp.path(), preparation.request_id());
        // This exact error proves neither a hidden None nor normalization followed by decoder failure.
        // 此精确错误证明既没有隐藏为 None，也没有规范化后解码失败。
        let error = store
            .abort_activation_preparation_retained(&mut preparation)
            .expect_err("actual early sidecar");
        assert_eq!(
            error.message,
            if extension.bytes().any(|byte| byte.is_ascii_uppercase()) {
                "Noncanonical reserved request sidecar extension."
            } else {
                "Early preparation cannot own request sidecars."
            }
        );
        assert_eq!(checkpoint(temp.path(), preparation.request_id()), before);
        assert_eq!(fs::read(&path).expect("evidence retained"), b"{}");
    }
}
