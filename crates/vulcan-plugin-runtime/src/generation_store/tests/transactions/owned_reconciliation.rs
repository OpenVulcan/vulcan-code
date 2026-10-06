//! Real checkpoint writers verify same-owner reconciliation without repairing durable authority.
//! 真实检查点写入器验证相同所有者核对，不修复持久权威。

use super::*;

/// Read transaction's actual retained request-file identity without retaining another readiness clone.
/// 读取 transaction 实际保留请求文件身份，不保留另一就绪克隆。
fn request_owner_address(transaction: &VulcanPluginActivationTransaction) -> *const std::fs::File {
    // This crate-internal clone observes the existing Arc; it never opens another file or grants execution.
    // 此 crate 内克隆观察既有 Arc；绝不打开另一文件或授予执行。
    let owner = transaction.retain_published_target_owner();
    Arc::as_ptr(&owner)
}

/// Verify store's actual original request lock remains Busy even after its persisted record became terminal.
/// 验证 store 实际原请求锁保持 Busy，即使持久记录已成为终态。
fn assert_original_request_busy(store: &VulcanPluginGenerationStore, request: Uuid) {
    assert_eq!(
        files::lock(
            &store
                .root
                .join("locks")
                .join(format!("sample-plugin.transaction.{request}.lock")),
            false,
        )
        .expect_err("same original physical request must remain exclusively owned")
        .code,
        Code::Busy
    );
}

/// Write the test's precise checkpoint corruption, preserving the original record path and requested phase.
/// 写入测试精确检查点损坏，保留原记录路径及请求阶段。
fn overwrite_record(root: &Path, request: Uuid, record: &serde_json::Value) {
    fs::write(
        record_path(root, request),
        serde_json::to_vec(record).expect("encode exact test journal image"),
    )
    .expect("write explicit external journal corruption");
}

/// Real successful checkpoints can lose both immediate observations, then confirm with the same UUID and physical lease.
/// 真实成功检查点可丢失两个即时观测，之后以相同 UUID 及物理租约确认。
#[test]
fn owned_reconciliation_preserves_original_after_actual_checkpoint_observation_loss() {
    for phase in [
        Phase::Committing,
        Phase::Published,
        Phase::Completed,
        Phase::Aborted,
    ] {
        // Each actual writer operates on independent immutable package and credential generations.
        // 每个实际写入器操作独立不可变插件包及凭据代次。
        let temp = tempfile::tempdir().expect("create owned reconciliation writer fixture");
        // Actual production preparation supplies every frozen authority and live physical pin.
        // 实际生产准备提供每个固定权威及存活物理固定句柄。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // One original UUID is retained through all failures and observations.
        // 在全部失败及观测期间保留单个原 UUID。
        let request = Uuid::new_v4();
        // This live owner is never dropped or reconstructed during reconciliation.
        // 在核对期间绝不释放或重建此存活所有者。
        let mut transaction = begin(&store, &second, &credentials, request);
        if phase == Phase::Completed {
            store
                .commit_activation_transaction(&mut transaction)
                .expect("publish before real completion writer");
        }
        // Preserve the actual Arc allocation, complete old observation and original pinned identities.
        // 保留实际 Arc 分配、完整旧观测及原固定身份。
        let owner = request_owner_address(&transaction);
        // Immutable metadata must not be reconstructed when only the phase observation advances.
        // 仅阶段观测推进时，不得重建不可变元数据。
        let original = transaction.record().clone();
        // The same physical target and credential inventory remain owned throughout this loop case.
        // 相同物理目标及凭据清单在此循环场景全程保持所有权。
        let package = transaction.generation().identity().clone();
        // Complete staged inventory comparison includes its original parent request when present.
        // 完整已暂存清单比较包含其存在时的原父请求。
        let inventory = transaction.credentials().record().clone();
        // Capture the actual index bytes so no readback may repair or reassign this original request.
        // 捕获实际索引字节，使回读不得修复或重新分配此原请求。
        let index_path = store.root.join("transactions/sample-plugin/current");
        // The real writer changes the journal only, or the selected activation during publication.
        // 真实写入器仅变更日志，或在发布期间变更已选激活。
        let index_before = fs::read(&index_path).expect("retain original indexed UUID bytes");
        // Fault begins only after the requested actual checkpoint writer succeeds.
        // 故障仅在请求的实际检查点写入器成功之后开始。
        let _fault = store.lose_activation_checkpoint_observation_once(request, phase);
        // Invoke the ordinary real mutation rather than writing a fabricated successful checkpoint.
        // 调用普通真实变更，而非写入伪造成功检查点。
        let write_error = match phase {
            Phase::Committing | Phase::Published => store
                .commit_activation_transaction(&mut transaction)
                .expect_err("lose the successful real commit checkpoint observation"),
            Phase::Completed => store
                .complete_activation_transaction(&mut transaction)
                .expect_err("lose the successful real completion checkpoint observation"),
            Phase::Aborted => store
                .abort_activation_transaction(&mut transaction)
                .expect_err("lose the successful real abort checkpoint observation"),
            _ => unreachable!(),
        };
        assert_eq!(write_error.code, Code::Storage);
        // Publishing first confirms Committing; all other failures retain their previous full record.
        // 发布首先确认 Committing；其他失败保留先前完整记录。
        let mut expected_memory = original.clone();
        if phase == Phase::Published {
            expected_memory = serde_json::from_value({
                // Derive only the known preceding checkpoint, preserving every immutable authority field.
                // 仅派生已知前序检查点，保留每个不可变权威字段。
                let mut encoded =
                    serde_json::to_value(&original).expect("encode original authority");
                encoded["phase"] = serde_json::to_value(Phase::Committing).expect("phase");
                encoded
            })
            .expect("decode exact preceding committing observation");
        }
        assert_eq!(transaction.record(), &expected_memory);
        assert_original_request_busy(&store, request);
        assert_eq!(request_owner_address(&transaction), owner);
        // Canonical original queries still read the real newly persisted checkpoint.
        // 规范原查询仍读取真实新持久检查点。
        let durable = store
            .activation_transaction("sample-plugin", request)
            .expect("read canonical actual persisted checkpoint")
            .expect("original request exists");
        assert_eq!(durable.phase(), phase);
        // Preserve exact durable bytes across both the failed and confirmed owned readbacks.
        // 在失败及确认的已持有所有者回读期间保留精确持久字节。
        let journal = fs::read(record_path(temp.path(), request)).expect("actual durable bytes");
        // The immediate original-owner readback loses observation without changing memory or disk.
        // 即时原所有者回读丢失观测，不变更内存或磁盘。
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect_err("immediate indexed observation remains unknown")
                .code,
            Code::Storage
        );
        assert_eq!(transaction.record(), &expected_memory);
        assert_original_request_busy(&store, request);
        // A later confirmation reads the same indexed original with the same actual owner and no new request.
        // 后续确认使用相同实际所有者读取相同已索引原请求，不创建新请求。
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect("later same-owner exact readback confirms"),
            phase
        );
        assert_eq!(transaction.record(), &durable);
        assert_eq!(transaction.record().request_id(), request);
        assert_eq!(request_owner_address(&transaction), owner);
        assert_original_request_busy(&store, request);
        assert_eq!(transaction.generation().identity(), &package);
        assert_eq!(transaction.credentials().record(), &inventory);
        assert_eq!(
            fs::read(index_path).expect("unchanged original index"),
            index_before
        );
        assert_eq!(
            fs::read(record_path(temp.path(), request)).expect("unchanged original journal"),
            journal
        );
        assert!(
            store
                .installation_receipt("sample-plugin", request)
                .expect("no installation replay")
                .is_none()
        );
        assert!(
            store
                .activation_effects("sample-plugin", request)
                .expect("no callback replay")
                .is_none()
        );
    }
}

/// Equal observations and explicit one-step progression remain read-only for every configured phase.
/// 相同观测及明确单步推进对每个已配置阶段均保持只读。
#[test]
fn owned_reconciliation_accepts_equal_and_only_explicit_next_checkpoints() {
    for (observed, durable, target_selected) in [
        (Phase::Prepared, Phase::Prepared, false),
        (Phase::Prepared, Phase::Committing, false),
        (Phase::Prepared, Phase::Committing, true),
        (Phase::Prepared, Phase::Aborted, false),
        (Phase::Committing, Phase::Committing, false),
        (Phase::Committing, Phase::Committing, true),
        (Phase::Committing, Phase::Published, true),
        (Phase::Committing, Phase::Aborted, false),
        (Phase::Published, Phase::Published, true),
        (Phase::Published, Phase::Completed, true),
        (Phase::Completed, Phase::Completed, true),
        (Phase::Aborted, Phase::Aborted, false),
    ] {
        // Explicit boundary images exercise only the declared checkpoint and selection matrix.
        // 明确边界映像仅验证声明的检查点及选择矩阵。
        let temp = tempfile::tempdir().expect("checkpoint matrix fixture");
        // Every image preserves real immutable package, configuration and credential authorities.
        // 每个映像保留真实不可变插件包、配置及凭据权威。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // Original live owner remains exclusive while disk observations differ.
        // 磁盘观测不同时，原存活所有者保持排他。
        let request = Uuid::new_v4();
        // Prepare a genuine full original record before constructing explicit interruption boundaries.
        // 构造明确中断边界之前准备真实完整原记录。
        let mut transaction = begin(&store, &second, &credentials, request);
        // Advance its actual in-memory owner to the requested baseline with real store mutations.
        // 使用真实存储变更将其实际内存所有者推进至请求基线。
        match observed {
            Phase::Committing => {
                // A successful real Committing write loses its local result, preserving that exact intermediate state.
                // 成功真实 Committing 写入丢失局部结果，保留该精确中间状态。
                let fault =
                    store.lose_activation_checkpoint_observation_once(request, Phase::Committing);
                store
                    .commit_activation_transaction(&mut transaction)
                    .expect_err("stop at actual committing");
                store
                    .reconcile_activation_transaction_owned(&mut transaction)
                    .expect_err("lose immediate observation");
                store
                    .reconcile_activation_transaction_owned(&mut transaction)
                    .expect("confirm committing owner");
                drop(fault);
            }
            Phase::Published | Phase::Completed => {
                store
                    .commit_activation_transaction(&mut transaction)
                    .expect("publish baseline");
                if observed == Phase::Completed {
                    store
                        .complete_activation_transaction(&mut transaction)
                        .expect("complete baseline");
                }
            }
            Phase::Aborted => store
                .abort_activation_transaction(&mut transaction)
                .expect("abort baseline"),
            Phase::Prepared => {}
            Phase::Preparing => unreachable!(),
        }
        // Alter the actual selected pointer only to the explicitly documented side of Committing.
        // 仅将实际已选指针变更至明确记录的 Committing 一侧。
        let selected = if target_selected {
            Some(transaction.record().target().clone())
        } else {
            transaction.record().predecessor().cloned()
        };
        fs::write(
            store.root.join("active/sample-plugin.json"),
            serde_json::to_vec(&selected.expect("fixture predecessor exists"))
                .expect("selection image"),
        )
        .expect("set explicit selection boundary");
        // Only phase differs between this frozen full original and the durable boundary image.
        // 此固定完整原记录与持久边界映像之间仅阶段不同。
        let mut image = serde_json::to_value(transaction.record()).expect("full original image");
        image["phase"] = serde_json::to_value(durable).expect("durable phase");
        overwrite_record(temp.path(), request, &image);
        // Confirming never writes a replacement journal or activation.
        // 确认绝不写入替代日志或激活。
        let journal = fs::read(record_path(temp.path(), request)).expect("boundary journal bytes");
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect("confirm exact allowed boundary"),
            durable
        );
        assert_eq!(
            fs::read(record_path(temp.path(), request)).expect("read-only journal"),
            journal
        );
    }
}

/// Full schema, predecessor, target and immutable preparation authority cannot be replaced by matching revisions.
/// 完整模式、前驱、目标及不可变准备权威不能由匹配修订代替。
#[test]
fn owned_reconciliation_rejects_changed_full_original_record_without_repair() {
    for changed in ["schema_version", "predecessor", "target", "preparation"] {
        // Independent bytes identify the exact failed immutable-authority check.
        // 独立字节标识精确失败的不可变权威检查。
        let temp = tempfile::tempdir().expect("full authority fixture");
        // The source and target remain real physically pinned packages even when disk is corrupted.
        // 即使磁盘损坏，来源及目标仍为真实物理固定插件包。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // Preserve a live actual owner to prove corruption cannot force lease reacquisition.
        // 保留存活实际所有者，证明损坏不能迫使重新取得租约。
        let request = Uuid::new_v4();
        // Its initial complete record is the sole immutable authority.
        // 其初始完整记录为唯一不可变权威。
        let mut transaction = begin(&store, &second, &credentials, request);
        // Preserve memory and corrupt only externally persisted bytes.
        // 保留内存，仅损坏外部持久字节。
        let original = transaction.record().clone();
        // All immutable fields are inspected from actual current serializer output.
        // 所有不可变字段均从实际当前序列化输出检查。
        let mut image = serde_json::to_value(&original).expect("complete record image");
        match changed {
            "schema_version" => image[changed] = serde_json::json!(99),
            "predecessor" => image[changed]["approved_permissions"] = serde_json::json!(null),
            "target" => {
                image[changed]["settings"]["credential_revision"] =
                    serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                        .expect("different exact revision")
            }
            "preparation" => image[changed] = serde_json::json!({}),
            _ => unreachable!(),
        }
        overwrite_record(temp.path(), request, &image);
        // Rejected bytes remain evidence rather than being repaired from the live record.
        // 被拒绝字节继续作为证据，不从存活记录修复。
        let rejected =
            fs::read(record_path(temp.path(), request)).expect("rejected full record bytes");
        assert!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .is_err()
        );
        assert_eq!(transaction.record(), &original);
        assert_eq!(
            fs::read(record_path(temp.path(), request)).expect("retain rejected bytes"),
            rejected
        );
        assert_original_request_busy(&store, request);
    }
}

/// The declared index must still point to the original request; absence and another valid UUID are never repaired.
/// 声明索引必须仍指向原请求；缺失及另一有效 UUID 绝不被修复。
#[test]
fn owned_reconciliation_rejects_missing_or_switched_original_index() {
    for missing in [false, true] {
        // The live owner remains the same while only the namespace index is externally altered.
        // 仅命名空间索引被外部变更，存活所有者保持相同。
        let temp = tempfile::tempdir().expect("index authority fixture");
        // Real staged package authorities prevent unrelated validation failures.
        // 真实已暂存插件包权威避免无关校验失败。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // This UUID must be retained even when another complete journal exists.
        // 即使另一完整日志存在，仍必须保留此 UUID。
        let request = Uuid::new_v4();
        // Never release this owner during switched-index readback.
        // 在索引切换回读期间绝不释放此所有者。
        let mut transaction = begin(&store, &second, &credentials, request);
        // The full record and index are independently preserved for read-only failure assertions.
        // 独立保留完整记录及索引，用于只读失败断言。
        let original = transaction.record().clone();
        // The actual index location comes from the production record layout.
        // 实际索引位置来自生产记录布局。
        let index = store.root.join("transactions/sample-plugin/current");
        if missing {
            fs::remove_file(&index).expect("remove original declared index");
        } else {
            // Another valid canonical request prevents an absence-only implementation from passing.
            // 另一有效规范请求防止仅检查缺失的实现通过。
            let other = Uuid::new_v4();
            // Preserve every full authority except the exact request identity.
            // 保留每个完整权威，仅改变精确请求身份。
            let mut other_image = serde_json::to_value(&original).expect("other valid full record");
            other_image["request_id"] = serde_json::to_value(other).expect("canonical other UUID");
            overwrite_record(temp.path(), other, &other_image);
            // Derive its index schema from the actual original index rather than inventing field names.
            // 从实际原索引派生其索引模式，不虚构字段名称。
            let mut pointer: serde_json::Value =
                serde_json::from_slice(&fs::read(&index).expect("actual index bytes"))
                    .expect("actual index structure");
            pointer["request_id"] = serde_json::to_value(other).expect("new index UUID");
            fs::write(
                &index,
                serde_json::to_vec(&pointer).expect("other valid index"),
            )
            .expect("switch actual index");
        }
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect_err("exact original index must remain authoritative")
                .code,
            Code::Conflict
        );
        assert_eq!(transaction.record(), &original);
        assert_eq!(index.exists(), !missing);
        assert_original_request_busy(&store, request);
    }
}

/// Skipped, reversed and terminal-to-other checkpoints cannot be inferred from a later-looking enum phase.
/// 跳步、倒退及终态至其他检查点不能从看似较后的枚举阶段推断。
#[test]
fn owned_reconciliation_rejects_checkpoint_jumps_and_terminal_changes() {
    for (baseline, rejected) in [
        (Phase::Prepared, Phase::Preparing),
        (Phase::Prepared, Phase::Published),
        (Phase::Prepared, Phase::Completed),
        (Phase::Committing, Phase::Prepared),
        (Phase::Committing, Phase::Completed),
        (Phase::Published, Phase::Prepared),
        (Phase::Published, Phase::Committing),
        (Phase::Published, Phase::Aborted),
        (Phase::Completed, Phase::Published),
        (Phase::Completed, Phase::Aborted),
        (Phase::Aborted, Phase::Prepared),
        (Phase::Aborted, Phase::Completed),
    ] {
        // Each rejected phase starts from a real production checkpoint and retains all original owners.
        // 每个被拒绝阶段从真实生产检查点开始，并保留全部原所有者。
        let temp = tempfile::tempdir().expect("invalid checkpoint matrix fixture");
        // Real full target authority stays unchanged, isolating checkpoint order rejection.
        // 真实完整目标权威保持不变，隔离检查点顺序拒绝。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // Canonical original request survives every rejected observation.
        // 规范原请求在每个被拒绝观测期间继续存续。
        let request = Uuid::new_v4();
        // Advance through ordinary production operations rather than changing the live owner's phase.
        // 通过普通生产操作推进，不改变存活所有者阶段。
        let mut transaction = begin(&store, &second, &credentials, request);
        if baseline == Phase::Committing {
            // A real interrupted checkpoint establishes Committing without publishing the target.
            // 真实中断检查点建立 Committing，而不发布目标。
            let fault =
                store.lose_activation_checkpoint_observation_once(request, Phase::Committing);
            store
                .commit_activation_transaction(&mut transaction)
                .expect_err("stop at actual committing checkpoint");
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect_err("immediate committing observation lost");
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect("confirm actual committing baseline");
            drop(fault);
        }
        if matches!(baseline, Phase::Published | Phase::Completed) {
            store
                .commit_activation_transaction(&mut transaction)
                .expect("publish actual baseline");
        }
        if baseline == Phase::Completed {
            store
                .complete_activation_transaction(&mut transaction)
                .expect("complete actual baseline");
        }
        if baseline == Phase::Aborted {
            store
                .abort_activation_transaction(&mut transaction)
                .expect("abort actual baseline");
        }
        // Preserve the exact live checkpoint across external durable corruption.
        // 在外部持久损坏期间保留精确存活检查点。
        let original = transaction.record().clone();
        // Change only the disk phase to expose forbidden ordering without replacing authority fields.
        // 仅改变磁盘阶段，暴露禁止顺序，不替换权威字段。
        let mut image = serde_json::to_value(&original).expect("exact original record");
        image["phase"] = serde_json::to_value(rejected).expect("rejected phase");
        overwrite_record(temp.path(), request, &image);
        // Preparing is rejected by the sole full-record validator; all well-formed invalid jumps are conflicts.
        // Preparing 由唯一完整记录校验器拒绝；全部格式合法的无效跳步均为冲突。
        let expected = if rejected == Phase::Preparing {
            Code::Invalid
        } else {
            Code::Conflict
        };
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect_err("checkpoint cannot jump or leave terminal")
                .code,
            expected
        );
        assert_eq!(transaction.record(), &original);
    }
}

/// Every checkpoint requires its complete allowed selection, rejecting equal-revision substitutions and wrong pointer sides.
/// 每个检查点要求其完整允许选择，拒绝相同修订替换及错误指针一侧。
#[test]
fn owned_reconciliation_rejects_selection_mismatches_without_changing_memory() {
    for (durable, target_selected, altered_equal_revision) in [
        (Phase::Prepared, true, false),
        (Phase::Committing, false, true),
        (Phase::Published, false, false),
        (Phase::Completed, false, false),
        (Phase::Aborted, true, false),
    ] {
        // Selection corruption is kept separate from a legitimate original checkpoint advance.
        // 选择损坏与合法原检查点推进保持分离。
        let temp = tempfile::tempdir().expect("selection mismatch fixture");
        // Complete configured activation authorities originate from production preparation.
        // 完整已配置激活权威来自生产准备。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // One UUID and live owner remain unchanged after selection mismatch rejection.
        // 选择错配拒绝后，单个 UUID 及存活所有者保持不变。
        let request = Uuid::new_v4();
        // The baseline is Prepared, except completion requires a legitimately observed Published predecessor.
        // 基线为 Prepared，但完成要求合法已观察的 Published 前序。
        let mut transaction = begin(&store, &second, &credentials, request);
        if matches!(durable, Phase::Published | Phase::Completed) {
            store
                .commit_activation_transaction(&mut transaction)
                .expect("publish required baseline");
        }
        // Failed observation cannot change the actual original record in memory.
        // 失败观测不能改变内存中的实际原记录。
        let original = transaction.record().clone();
        // Explicitly install the wrong selection while retaining its format and canonical plugin identity.
        // 明确安装错误选择，同时保留其格式及规范插件身份。
        let mut selected = serde_json::to_value(if target_selected {
            original.target()
        } else {
            original.predecessor().expect("actual predecessor")
        })
        .expect("selected complete record");
        if altered_equal_revision {
            // Same revision cannot hide changed credential authority.
            // 相同修订不能隐藏已变凭据权威。
            selected["settings"]["credential_revision"] =
                serde_json::to_value(VulcanPluginCredentialRevision::allocate())
                    .expect("different selected credential revision");
        }
        fs::write(
            store.root.join("active/sample-plugin.json"),
            serde_json::to_vec(&selected).expect("wrong selection bytes"),
        )
        .expect("write explicit pointer mismatch");
        // Derive the permitted checkpoint advance independently of the incorrect activation.
        // 独立于错误激活派生允许检查点推进。
        let mut image = serde_json::to_value(&original).expect("exact original image");
        image["phase"] = serde_json::to_value(durable).expect("desired durable checkpoint");
        overwrite_record(temp.path(), request, &image);
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect_err("full selected authority must match exact phase")
                .code,
            Code::Conflict
        );
        assert_eq!(transaction.record(), &original);
    }
}

/// Actual readiness ownership must retire before either terminal checkpoint can be confirmed.
/// 确认任一终态检查点之前，实际就绪所有权必须退役。
#[test]
fn owned_reconciliation_terminal_confirmation_requires_actual_readiness_retirement() {
    for aborted in [false, true] {
        // One retained physical readiness clone must prevent the terminal observation from becoming authoritative.
        // 单个保留物理就绪克隆必须阻止终态观测成为权威。
        let temp = tempfile::tempdir().expect("terminal readiness fixture");
        // Immutable target and credential leases remain valid throughout retirement.
        // 不可变目标及凭据租约在退役期间保持有效。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // Keep the sole original request rather than creating a substitute terminal owner.
        // 保留唯一原请求，不创建替代终态所有者。
        let request = Uuid::new_v4();
        // Real preparation yields the original request lock to be shared with readiness.
        // 真实准备产生待与就绪共享的原请求锁。
        let mut transaction = begin(&store, &second, &credentials, request);
        if !aborted {
            store
                .commit_activation_transaction(&mut transaction)
                .expect("publish completion baseline");
        }
        // Freeze the observed baseline before writing the explicit terminal boundary.
        // 写入明确终态边界前固定已观察基线。
        let original = transaction.record().clone();
        // This is an actual clone of the exclusively locked original file, not a synthetic busy flag.
        // 此为实际排他加锁原文件克隆，不是合成忙碌标记。
        let readiness = transaction.retain_published_target_owner();
        // The externally constructed terminal image remains blocked until that actual clone retires.
        // 外部构造终态映像保持阻挡，直到此实际克隆退役。
        let terminal = if aborted {
            Phase::Aborted
        } else {
            Phase::Completed
        };
        // Preserve every immutable original field and change only its known terminal checkpoint.
        // 保留每个不可变原字段，仅改变其已知终态检查点。
        let mut image = serde_json::to_value(&original).expect("exact readiness original");
        image["phase"] = serde_json::to_value(terminal).expect("terminal checkpoint");
        overwrite_record(temp.path(), request, &image);
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect_err("actual readiness clone blocks terminal confirmation")
                .code,
            Code::Busy
        );
        assert_eq!(transaction.record(), &original);
        drop(readiness);
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect("same owner confirms only after actual retirement"),
            terminal
        );
        assert_original_request_busy(&store, request);
    }
}

/// Strict private Hook or MCP evidence rejects ordinary owned readback even when complete disk bytes match.
/// 即使完整磁盘字节匹配，严格私有 Hook 或 MCP 证据仍拒绝普通已持有所有者回读。
#[test]
fn owned_reconciliation_rejects_private_target_receipts_and_completion() {
    for (mcp, completed) in [(false, false), (false, true), (true, false), (true, true)] {
        // Durable private evidence cannot substitute for actual combined runtime owners.
        // 持久私有证据不能代替实际组合运行时所有者。
        let temp = tempfile::tempdir().expect("private receipt boundary fixture");
        // The published target remains a genuine full configured activation.
        // 已发布目标保持为真实完整已配置激活。
        let (store, _first, second, _, credentials) = fixture(temp.path());
        // One original request persists across the explicitly simulated private receipt boundary.
        // 单个原请求在明确模拟的私有回执边界期间继续存续。
        let request = Uuid::new_v4();
        // Ordinary publication is legitimate before any private resource is recorded.
        // 记录任何私有资源之前，普通发布合法。
        let mut transaction = begin(&store, &second, &credentials, request);
        store
            .commit_activation_transaction(&mut transaction)
            .expect("publish actual original target");
        // Neither current nor terminal readback may alter this full observed original.
        // 当前及终态回读均不得改变此完整已观察原记录。
        let original = transaction.record().clone();
        // Build an independently validated strict Started receipt; no runtime-start success is fabricated.
        // 构造独立校验的严格 Started 回执；不伪造运行时启动成功。
        let receipt = if mcp {
            serde_json::json!({"schema_version": 1, "request_id": request, "target": original.target(), "routing_generation": 7, "phase": "started", "instances": null})
        } else {
            serde_json::json!({"schema_version": 1, "request_id": request, "target": original.target(), "routing_generation": 7, "phase": "started", "core": null})
        };
        // Separate authoritative suffixes come directly from the respective receipt readers.
        // 独立权威后缀直接来自对应回执读取器。
        let suffix = if mcp { "target-mcp" } else { "target-hook" };
        fs::write(
            store
                .root
                .join("transactions/sample-plugin")
                .join(format!("{request}.{suffix}")),
            serde_json::to_vec(&receipt).expect("strict Started evidence bytes"),
        )
        .expect("write explicit private receipt boundary");
        if completed {
            // A completed journal alone cannot prove transfer of the absent physical owners.
            // 单独完成日志不能证明缺失物理所有者已转交。
            let mut image = serde_json::to_value(&original).expect("original full journal");
            image["phase"] = serde_json::to_value(Phase::Completed).expect("completed checkpoint");
            overwrite_record(temp.path(), request, &image);
        }
        assert_eq!(
            store
                .reconcile_activation_transaction_owned(&mut transaction)
                .expect_err("private resource handoff cannot be confirmed by ordinary readback")
                .code,
            Code::Conflict
        );
        assert_eq!(transaction.record(), &original);
        assert_original_request_busy(&store, request);
    }
}

/// Foreign stores cannot observe another original owner or change either namespace's persistent state.
/// 外部存储不能观察另一原所有者，或改变任一命名空间持久状态。
#[test]
fn owned_reconciliation_rejects_cross_store_owner_before_namespace_access() {
    // The original live request is pinned inside its own actual store.
    // 原存活请求固定在其自身实际存储内。
    let temp = tempfile::tempdir().expect("original store fixture");
    // Real generations provide the original complete record.
    // 真实代次提供原完整记录。
    let (store, _first, second, _, credentials) = fixture(temp.path());
    // Own one exact request without dropping its physical leases.
    // 持有单个精确请求，不释放其物理租约。
    let mut transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    // Preserve the original complete observation across foreign-root rejection.
    // 在外部根目录拒绝期间保留原完整观测。
    let original = transaction.record().clone();
    // The other root starts with no plugin namespace to make unintended access observable.
    // 另一根目录初始没有插件命名空间，使意外访问可被观察。
    let other = tempfile::tempdir().expect("foreign store fixture");
    // Opening a distinct actual store cannot confer authority over the original owner.
    // 打开不同实际存储不能授予原所有者权威。
    let foreign = VulcanPluginGenerationStore::open(other.path()).expect("foreign store");
    assert_eq!(
        foreign
            .reconcile_activation_transaction_owned(&mut transaction)
            .expect_err("cross-store owner must fail before namespace lock")
            .code,
        Code::Conflict
    );
    assert_eq!(transaction.record(), &original);
    assert!(!foreign.root.join("locks/sample-plugin.lock").exists());
    assert_original_request_busy(&store, original.request_id());
}

/// An original never-published namespace preserves explicit predecessor absence rather than inventing a revision-zero record.
/// 原从未发布命名空间保留显式前驱缺失，不虚构修订零记录。
#[test]
fn owned_reconciliation_preserves_explicit_unpublished_predecessor_absence() {
    // This isolated store has no earlier activation to provide an accidental predecessor.
    // 此隔离存储没有先前激活，不会意外提供前驱。
    let temp = tempfile::tempdir().expect("unpublished predecessor fixture");
    // Real storage construction creates the same authority root used by the production host.
    // 真实存储构造创建与生产宿主相同的权威根。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open never-published store");
    // A genuinely prepared target is the only immutable package in this namespace.
    // 真正已准备目标为此命名空间唯一不可变插件包。
    let package = prepare(
        &store,
        &write_package(&temp.path().join("source"), "sample-plugin", "first target"),
    );
    // Staging still retains actual credential ownership despite having no configured secret slots.
    // 即使没有已配置秘密槽位，暂存仍保留实际凭据所有权。
    let credentials = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &package, &credentials);
    // One original UUID binds the explicit absent predecessor to this first target.
    // 单个原 UUID 将显式缺失前驱绑定至此首个目标。
    let request = Uuid::new_v4();
    // Revision zero is only the caller's expected absence, not a persisted activation image.
    // 修订零仅为调用方预期缺失，不是持久激活映像。
    let mut transaction = store
        .begin_activation_transaction(
            request,
            &package,
            0,
            vec![],
            credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("begin first exact activation");
    assert!(transaction.record().predecessor().is_none());
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut transaction)
            .expect("absence is the exact prepared predecessor"),
        Phase::Prepared
    );
    // A real aborted writer can lose acknowledgement without ever creating an active selection file.
    // 真实中止写入器可丢失确认，而不创建任何有效选择文件。
    let _fault = store.lose_activation_checkpoint_observation_once(request, Phase::Aborted);
    store
        .abort_activation_transaction(&mut transaction)
        .expect_err("lose real first-activation abort observation");
    store
        .reconcile_activation_transaction_owned(&mut transaction)
        .expect_err("lose immediate exact indexed abort observation");
    assert_eq!(
        store
            .reconcile_activation_transaction_owned(&mut transaction)
            .expect("confirm aborted original absent predecessor"),
        Phase::Aborted
    );
    assert!(transaction.record().predecessor().is_none());
    assert!(
        store
            .activation("sample-plugin")
            .expect("read actual absent selection")
            .is_none()
    );
    assert!(!store.root.join("active/sample-plugin.json").exists());
    assert_original_request_busy(&store, request);
}
