//! Real service abort restores one managed old native process and transfers its original reservation.
//! 真实服务中止恢复单个受管旧原生进程，并转交其原保留权。

use super::*;
use crate::skills_services::local_plugin_update;
use crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH;
use vulcan_agent_runtime::agent_runtime::ExtensionGeneration;
use vulcan_agent_runtime::mcp::McpRuntimeToolAllowlist;
use vulcan_plugin_runtime::{
    VulcanPluginActivationEffect, VulcanPluginActivationTransactionPhase,
    VulcanPluginActivationTransactionRecord, VulcanPluginEffectObservation,
    VulcanPluginPredecessorHandoffPhase, VulcanPluginRuntimeState,
};

/// Retain the existing real fixture after the production enabled update physically drained its old child.
/// 在生产启用更新物理排空其旧子进程之后，保留既有真实夹具。
pub(in crate::skills_services) struct FailedUpdate {
    /// Actual service state retains the original update resources and the sole Full pointer.
    /// 实际服务状态保留原更新资源及唯一完整指针。
    pub(in crate::skills_services) state: SharedServiceBridgeState,
    /// The same manager remains the only authority for catalog publication and admission.
    /// 相同管理器保持为目录发布及准入的唯一权威。
    manager: VulcanPluginRuntimeManager,
    /// Real durable store exposes the complete original without resuming or manufacturing ownership.
    /// 真实持久存储公开完整原记录，不恢复或制造所有权。
    pub(in crate::skills_services) store: VulcanPluginGenerationStore,
    /// The production allocator's complete Prepared record identifies this exact failed request.
    /// 生产分配器的完整 Prepared 记录标识此精确失败请求。
    pub(in crate::skills_services) prepared: VulcanPluginActivationTransactionRecord,
    /// Only actual child processes append records to this private marker.
    /// 仅实际子进程向此私有标记追加记录。
    pub(in crate::skills_services) marker: std::path::PathBuf,
    /// The actual original child PID precedes drain; restored old work must use a distinct child.
    /// 实际原子进程 PID 先于排空；恢复旧工作必须使用不同子进程。
    original_pid: u64,
    /// The private policy destination is installed around the direct production preparation coordinator.
    /// 私有策略目标在直接生产准备协调器外安装作用域。
    policy: std::path::PathBuf,
    /// Keep immutable files valid until preceding real service and native owners have been disposed.
    /// 保持不可变文件有效，直至前置真实服务及原生所有者已销毁。
    _temp: tempfile::TempDir,
}

/// Capture the actual service Full pointer under its sole mutex; state supplies ownership, return preserves identity.
/// 在唯一互斥锁内捕获实际服务完整指针；state 提供所有权，返回值保留身份。
fn full(state: &SharedServiceBridgeState) -> Arc<ExtensionGeneration> {
    Arc::clone(
        &state
            .lock()
            .expect("actual Full observation")
            .agent_runtime
            .as_ref()
            .expect("actual service bridge")
            .extension_generation,
    )
}

/// Read real child-produced JSON records from marker; returns complete process observations, never readiness authority.
/// 从 marker 读取真实子进程生成的 JSON 记录；返回完整进程观测，绝不作为就绪权威。
pub(in crate::skills_services) fn starts(marker: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(marker)
        .expect("actual child startup marker")
        .lines()
        .map(|line| serde_json::from_str(line).expect("actual complete startup JSON"))
        .collect()
}

/// Invoke the discovered original_process tool through generation; message is real input, returned PID is child evidence.
/// 通过 generation 调用已发现 original_process 工具；message 为真实输入，返回 PID 为子进程证据。
async fn invoke_pid(generation: &Arc<ExtensionGeneration>, message: &str) -> u64 {
    // Stable declared tool identity avoids positional assumptions about the discovered inventory.
    // 稳定声明工具身份避免对发现清单的位置作假设。
    let binding = generation
        .mcp_runtime()
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "published-service:actual/original_process")
        .expect("the expected real original_process declaration may have changed");
    // The actual transport result proves normal business reached the captured native process.
    // 实际传输结果证明普通业务到达捕获的原生进程。
    let result = binding
        .invoke(serde_json::json!({"message":message}))
        .await
        .expect("real restored service business");
    assert_eq!(result["message"], message);
    result["pid"].as_u64().expect("actual child PID")
}

/// Prepare actual old A, then fail one production update before selection; returns its real retained service owner.
/// 准备实际旧 A，随后让单次生产更新在选择前失败；返回其真实保留服务所有者。
/// No inputs; the private direct coordinator scope avoids the public RPC's deliberately non-inherited task locals.
/// 无输入；私有直接协调器作用域避免公开 RPC 按设计不继承的任务局部变量。
async fn failed_update() -> FailedUpdate {
    failed_update_with_fixture(fixture(), false).await
}

/// Launch fixture's real old A then fail the same production update; remove_old_secret is true only for its declared variant.
/// 启动 fixture 的真实旧 A 后让同一生产更新失败；remove_old_secret 仅在其声明变体中为真。
/// Return the actual retained request and files; removing the old env from mutable candidate prevents OS-secret access.
/// 返回实际保留请求及文件；从可变候选删除旧环境输入，防止访问操作系统秘密。
pub(in crate::skills_services) async fn failed_update_with_fixture(
    fixture: Fixture,
    remove_old_secret: bool,
) -> FailedUpdate {
    // Reuse the existing genuine managed old selection and the service's first native issuer.
    // 复用既有真实受管旧选择及服务首次原生签发者。
    let Fixture {
        state,
        configs,
        marker,
        environment: _,
        _temp,
    } = fixture;
    // Both authorities originate from one real service observation, without creating another native registration.
    // 两个权威来自单次真实服务观察，不创建另一原生注册。
    let (manager, authority) =
        crate::service_state_access::service_vulcan_plugin_native_mcp_host(&state)
            .expect("actual original service issuer");
    // This current access is the actual original lineage used by subsequent update and restoration.
    // 此当前访问为后续更新及恢复使用的实际原谱系。
    let current = full(&state);
    // Consume the fixture's prepared pins by launching the one real original child.
    // 通过启动唯一真实原子进程消费夹具的已准备固定句柄。
    let native =
        PublishedPluginMcpRuntime::start(&current.mcp_runtime(), &manager, configs, &authority)
            .expect("actual original native preparation");
    // The real observer must independently confirm initialization before any service publication.
    // 任何服务发布之前，真实观察器必须独立确认初始化。
    let observation = native.observer();
    observation
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual original native Ready");
    // Production publication owns the original native lease under the actual Full pointer.
    // 生产发布在实际完整指针下拥有原原生租约。
    let published =
        publish_native_published_plugin_extension_generation(&state, &native, manager.snapshot())
            .expect("actual original service Full");
    // Obtain A from actual business rather than inferring readiness from marker bytes.
    // 从实际业务取得 A，不从标记字节推断就绪。
    let original_pid = invoke_pid(&published, "old A before enabled update").await;
    assert!(original_pid > 0);
    assert_eq!(
        starts(&marker).len(),
        1,
        "actual original start count may have changed"
    );
    drop(native);
    // The real store exposes the complete managed predecessor before any candidate effect.
    // 在任何候选副作用之前，真实存储公开完整受管前驱。
    let store = VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("actual managed store");
    // Preserve the writer-selected full baseline to verify the failed update never selected its target.
    // 保留写入器所选完整基线，验证失败更新从未选择其目标。
    let predecessor = store
        .activation("published-service")
        .expect("actual old selection")
        .expect("managed old selection exists");
    assert!(
        store
            .activation_transaction_ids("published-service")
            .expect("actual initial request inventory")
            .is_empty()
    );
    // This exact mutable source belongs to fixture(); immutable predecessor bytes are never rewritten.
    // 此精确可变来源属于 fixture()；不可变前驱字节绝不重写。
    let source = _temp.path().join("source/published-service");
    // Changing the known manifest version creates a distinct inspected candidate.
    // 改变已知清单版本创建不同的已检查候选。
    let manifest = std::fs::read_to_string(source.join("plugin.yaml"))
        .expect("actual mutable candidate manifest");
    assert!(manifest.contains("version: 1.0.0"));
    // The explicitly secret-bearing variant deletes only its known old input from mutable candidate bytes.
    // 明确携带秘密的变体仅从可变候选字节删除其已知旧输入。
    let manifest = if remove_old_secret {
        assert!(manifest.contains("          CANDIDATE_TOKEN: {source: secret, key: old_token}\n"));
        manifest.replacen(
            "          CANDIDATE_TOKEN: {source: secret, key: old_token}\n",
            "",
            1,
        )
    } else {
        manifest
    };
    std::fs::write(
        source.join("plugin.yaml"),
        manifest.replacen("version: 1.0.0", "version: 1.0.1", 1),
    )
    .expect("actual candidate manifest version");
    std::fs::write(source.join("bin/server.bin"), b"invalid executable fixture")
        .expect("actual candidate execution failure");
    // Production inspection supplies the exact content fingerprint and approved permissions.
    // 生产检查提供精确内容指纹及批准权限。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("actual invalid-executable candidate inspection");
    // Only this private path can receive the real coordinator's policy persistence.
    // 仅此私有路径可接收真实协调器的策略持久化。
    let policy = _temp.path().join("predecessor-update-policy.json");
    // Hold the same management fence as the public RPC while preserving the private persistence scope.
    // 保持与公开 RPC 相同的管理屏障，同时保留私有持久化作用域。
    let management = crate::skills_services::lock_luaskills_management_mutation().await;
    // The actual update error must follow real old drain and leave the original transaction retained.
    // 实际更新错误必须发生于真实旧排空之后，并保留原事务。
    let result = PROVIDER_CONFIGURATION_PATH
        .scope(policy.clone(), async {
            local_plugin_update::update(
                &ServiceEventEmitter::new(Vec::new()),
                &state,
                source.to_string_lossy().into_owned(),
                preview.content_fingerprint.clone(),
                preview.permissions,
                false,
                Vec::new(),
            )
            .await
        })
        .await;
    drop(management);
    assert!(
        result.is_err(),
        "invalid actual candidate executable unexpectedly succeeded"
    );
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("actual old A physically retired");
    // The production allocator's sole durable UUID is discovered rather than generated in this test.
    // 发现生产分配器的唯一持久 UUID，而非在本测试中生成。
    let ids = store
        .activation_transaction_ids("published-service")
        .expect("actual failed request inventory");
    assert_eq!(
        ids.len(),
        1,
        "expected original request count may have changed"
    );
    // Validate the complete actual body before it selects lock or sidecar paths.
    // 在选择锁或附属路径之前，校验完整实际正文。
    let prepared = store
        .activation_transaction(
            "published-service",
            *ids.iter().next().expect("actual sole request"),
        )
        .expect("actual configured original request")
        .expect("actual configured record exists");
    assert_eq!(
        prepared.phase(),
        VulcanPluginActivationTransactionPhase::Prepared
    );
    assert_eq!(prepared.predecessor(), Some(&predecessor));
    assert_eq!(
        prepared
            .target()
            .generation()
            .expect("actual target")
            .content_fingerprint,
        preview.content_fingerprint
    );
    assert_eq!(
        store
            .activation("published-service")
            .expect("unchanged selected old"),
        Some(predecessor)
    );
    // This durable successful actual drain is independent of both marker observations and update error text.
    // 此持久成功实际排空独立于标记观测及更新错误文本。
    let effects = store
        .activation_effects("published-service", prepared.request_id())
        .expect("actual original drain effects")
        .expect("actual old drain receipt");
    assert!(effects.observations().contains(&(
        VulcanPluginActivationEffect::DrainPredecessorLua,
        VulcanPluginEffectObservation::Succeeded,
    )));
    assert!(effects.predecessor_drain_identity().is_some());
    assert_eq!(
        starts(&marker).len(),
        1,
        "invalid candidate must not start a child"
    );
    assert!(
        !policy.exists(),
        "unselected Prepared update must not persist plugin policy"
    );
    assert_owner(&state, prepared.request_id(), true).await;
    FailedUpdate {
        state,
        manager,
        store,
        prepared,
        marker,
        original_pid,
        policy,
        _temp,
    }
}

/// Query the actual live service registry projection for request; expected is ownership, never a readiness assertion.
/// 查询 request 的实际存活服务注册表投影；expected 为所有权，绝非就绪断言。
async fn assert_owner(state: &SharedServiceBridgeState, request: uuid::Uuid, expected: bool) {
    // Read-only service status remains independent of the slot's asynchronous work and private fields.
    // 只读服务状态独立于槽的异步工作及私有字段。
    let rows = local_plugin_update_records(Arc::clone(state))
        .await
        .expect("actual service update status");
    // Serialize the declared camelCase DTO instead of accessing inaccessible ownership implementation fields.
    // 序列化声明的驼峰命名 DTO，不访问不可见所有权实现字段。
    let rows = serde_json::to_value(rows).expect("actual status serialization");
    // The exact original UUID selects the row without relying on request inventory ordering.
    // 精确原 UUID 选择状态行，不依赖请求清单顺序。
    let row = rows
        .as_array()
        .expect("declared status list")
        .iter()
        .find(|row| row["requestId"] == request.to_string())
        .expect("expected original status identity may have changed");
    assert_eq!(row["hasServiceOwner"], expected);
}

/// Open the already-created original sidecar lease; fixture supplies actual manager root and full production UUID.
/// 打开已创建原附属租约；fixture 提供实际管理器根及完整生产 UUID。
/// Returns an independent OS file handle without creating a lock or acquiring an owner.
/// 返回独立操作系统文件句柄，不创建锁或取得所有者。
pub(in crate::skills_services) fn handoff_file(fixture: &FailedUpdate) -> std::fs::File {
    // generation_store::STORE_DIRECTORY and predecessor_handoff_lease_path are the single production mapping.
    // generation_store::STORE_DIRECTORY 与 predecessor_handoff_lease_path 为唯一生产映射。
    let path = fixture
        .manager
        .user_plugin_root()
        .join(".vulcan-generations")
        .join("locks")
        .join(format!(
            "{}.predecessor-handoff.{}.lock",
            fixture.prepared.target().plugin_id,
            fixture.prepared.request_id()
        ));
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("actual original sidecar File must already exist")
}

/// Read the original journal bytes using records::transaction_directory's sole mapping; fixture identifies the original.
/// 使用 records::transaction_directory 的唯一映射读取原日志字节；fixture 标识原请求。
/// Returns exact existing Aborted bytes, without changing or synthesizing any record.
/// 返回精确既有 Aborted 字节，不改变或合成任何记录。
fn journal_bytes(fixture: &FailedUpdate) -> Vec<u8> {
    std::fs::read(
        fixture
            .manager
            .user_plugin_root()
            .join(".vulcan-generations")
            .join("transactions")
            .join(&fixture.prepared.target().plugin_id)
            .join(format!("{}.json", fixture.prepared.request_id())),
    )
    .expect("actual original Aborted journal")
}

/// Physically drain the restored real node through the current published access; fixture retains files until completion.
/// 通过当前已发布访问物理排空恢复真实节点；fixture 保留文件直至完成。
/// Returns no value; successful production drain prevents live child ownership escaping this test.
/// 无返回值；成功生产排空防止存活子进程所有权逸出本测试。
async fn cleanup(fixture: &FailedUpdate) {
    // Capture the actual current lineage, including service-owned restored native leases.
    // 捕获实际当前谱系，包括服务持有恢复原生租约。
    let access = full(&fixture.state).mcp_runtime();
    access
        .begin_plugin_mcp_drain("published-service")
        .expect("actual restored cleanup drain");
    access
        .wait_plugin_mcp_drain("published-service", OBSERVE)
        .await
        .expect("actual restored physical cleanup");
}

/// Public acknowledged abort restores actual B and transfers its real old reservation without another initialization.
/// 公开确认中止恢复实际 B，并转交其真实旧保留权，不执行另一初始化。
/// No inputs or return; real Full/catalog identity, native business PID, original Aborted bytes and OS File prove the path.
/// 无输入或返回值；真实完整槽及目录身份、原生业务 PID、原 Aborted 字节及操作系统文件证明此路径。
#[tokio::test]
async fn service_predecessor_abort_transfers_actual_restored_node_without_replay() {
    // The actual Prepared failure already retired A and kept its unchanged old selection and live service slot.
    // 实际 Prepared 失败已经退役 A，并保持未变旧选择及存活服务槽。
    let fixture = failed_update().await;
    // This real UUID came from production allocation and must select every later status and durable observation.
    // 此真实 UUID 来自生产分配，必须选择每个后续状态及持久观测。
    let request = fixture.prepared.request_id();
    crate::skills_services::service_abort_local_plugin_update(
        Arc::clone(&fixture.state),
        String::from("published-service"),
        request.to_string(),
        true,
    )
    .await
    .expect("public actual enabled predecessor abort and joint transfer");
    // The canonical original stays Aborted; handoff is a separate actual resource-transfer record.
    // 规范原记录保持 Aborted；交接为独立实际资源转交记录。
    let aborted = fixture
        .store
        .activation_transaction("published-service", request)
        .expect("actual original Aborted readback")
        .expect("actual original remains present");
    assert_eq!(
        aborted.phase(),
        VulcanPluginActivationTransactionPhase::Aborted
    );
    assert_eq!(aborted.predecessor(), fixture.prepared.predecessor());
    assert_eq!(aborted.target(), fixture.prepared.target());
    // Capture the entire actual body and its bytes before exercising disk-only repeated abort.
    // 在执行仅磁盘重复中止之前，捕获整个实际正文及其字节。
    let aborted_bytes = journal_bytes(&fixture);
    // The handoff preserves its entire Prepared original independently of the activation's later Aborted phase.
    // 交接保留其整个 Prepared 原正文，独立于激活之后的 Aborted 阶段。
    let transferred = fixture
        .store
        .predecessor_handoff_record("published-service", request)
        .expect("actual transfer observation")
        .expect("actual original handoff exists");
    assert_eq!(
        transferred.phase(),
        VulcanPluginPredecessorHandoffPhase::Transferred
    );
    assert_eq!(transferred.original(), &fixture.prepared);
    // A separate existing OS file must become available only after original combined completion.
    // 仅原组合完成之后，独立既有操作系统文件才必须变为可用。
    let competing = handoff_file(&fixture);
    competing
        .try_lock()
        .expect("actual original unique reservation released");
    competing
        .unlock()
        .expect("release independent probe before business admission");
    assert!(
        !fixture
            .store
            .predecessor_handoff_blocks_admission("published-service")
            .expect("actual transferred admission readback")
    );
    assert_owner(&fixture.state, request, false).await;
    // The real service Full must capture the exact current core catalog allocation.
    // 真实服务完整槽必须捕获精确当前核心目录分配。
    let committed = full(&fixture.state);
    assert!(Arc::ptr_eq(
        &committed.plugin_catalog().expect("actual Full catalog"),
        &fixture.manager.snapshot()
    ));
    assert_eq!(
        fixture.manager.snapshot().plugins["published-service"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert_eq!(
        fixture
            .store
            .activation("published-service")
            .expect("actual restored old selection")
            .as_ref(),
        aborted.predecessor()
    );
    // Actual discovery and invocation prove restored B is open through this same service Full.
    // 实际发现及调用证明恢复 B 通过此相同服务完整槽开放。
    let restored_pid = invoke_pid(&committed, "restored B after public abort").await;
    assert_ne!(
        restored_pid, fixture.original_pid,
        "drained A must not be presented as restored B"
    );
    // Both records were produced by real processes; no failed candidate process was dispatched successfully.
    // 两条记录均由真实进程生成；未成功派发失败候选进程。
    let actual_starts = starts(&fixture.marker);
    assert_eq!(
        actual_starts.len(),
        2,
        "expected one A and one restored B may have changed"
    );
    assert_eq!(
        actual_starts
            .iter()
            .filter(|record| record["pid"] == restored_pid)
            .count(),
        1
    );
    assert_eq!(
        actual_starts
            .iter()
            .filter(|record| record["pid"] == fixture.original_pid)
            .count(),
        1
    );
    // Disk-only repeat is a historical abort readback after the strong service owner was genuinely retired.
    // 强服务所有者真实退休之后，仅磁盘重复为历史中止回读。
    crate::skills_services::service_abort_local_plugin_update(
        Arc::clone(&fixture.state),
        String::from("published-service"),
        request.to_string(),
        true,
    )
    .await
    .expect("public repeated original disk abort");
    assert!(Arc::ptr_eq(&committed, &full(&fixture.state)));
    assert_eq!(
        invoke_pid(&committed, "same restored B after repeat").await,
        restored_pid
    );
    assert_eq!(starts(&fixture.marker), actual_starts);
    assert_eq!(journal_bytes(&fixture), aborted_bytes);
    assert_eq!(
        fixture
            .store
            .activation_transaction("published-service", request)
            .expect("same complete Aborted body")
            .as_ref(),
        Some(&aborted)
    );
    assert_eq!(
        fixture
            .store
            .predecessor_handoff_record("published-service", request)
            .expect("same complete Transferred body")
            .as_ref(),
        Some(&transferred)
    );
    assert!(
        !fixture.policy.exists(),
        "predecessor abort must preserve original policy without persistence"
    );
    cleanup(&fixture).await;
}

/// Actual Full revision exhaustion preserves Ready B and Pending ownership until public same-request resume succeeds.
/// 实际完整槽修订耗尽保留就绪 B 及 Pending 所有权，直至公开同请求恢复成功。
/// No inputs or return; this real failure occurs after native readiness and before publication, without a simulated adapter.
/// 无输入或返回值；此真实失败发生于原生就绪之后及发布之前，不使用模拟适配器。
#[tokio::test]
async fn service_predecessor_full_failure_retains_actual_node_for_same_request_resume() {
    // Reuse exactly the real failed enabled update; no restoration source is constructed by this test.
    // 精确复用真实失败启用更新；本测试不构造任何恢复来源。
    let fixture = failed_update().await;
    // Keep the actual previous Full Arc so the injected failure can be undone before any assertions.
    // 保留实际先前完整 Arc，使注入故障可在任何断言之前撤销。
    let previous = full(&fixture.state);
    // A valid standalone generation uses the same real catalog, native lineage and prompt resources at MAX.
    // 有效独立代次在 MAX 使用相同真实目录、原生谱系及提示资源。
    let exhausted = Arc::new(ExtensionGeneration::new(
        u64::MAX,
        previous.plugin_catalog(),
        previous.mcp_runtime(),
        previous.prompt_skill_catalog(),
    ));
    fixture
        .state
        .lock()
        .expect("actual Full failure injection")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .extension_generation = Arc::clone(&exhausted);
    // The actual coordinator waits for real predecessor native readiness before preparing the Full successor.
    // 实际协调器在准备完整后继之前等待真实前驱原生就绪。
    let failed = crate::skills_services::service_abort_local_plugin_update(
        Arc::clone(&fixture.state),
        String::from("published-service"),
        fixture.prepared.request_id().to_string(),
        true,
    )
    .await;
    // Observe the actual failure slot and immediately restore the shared prior pointer before checking failure evidence.
    // 观察实际故障槽，并在检查故障证据之前立即恢复共享先前指针。
    let observed = full(&fixture.state);
    fixture
        .state
        .lock()
        .expect("restore actual prior Full after failure")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .extension_generation = Arc::clone(&previous);
    assert!(Arc::ptr_eq(&exhausted, &observed));
    assert!(
        failed
            .expect_err("actual exhausted Full must reject preparation")
            .contains("Extension generation revision exhausted.")
    );
    // This request remains the original production UUID even after its durable activation became Aborted.
    // 持久激活变为 Aborted 之后，此请求仍保持原生产 UUID。
    let request = fixture.prepared.request_id();
    // Read the full real Aborted record before probing its original Pending reservation.
    // 探测其原 Pending 保留权之前，读取完整真实 Aborted 记录。
    let aborted = fixture
        .store
        .activation_transaction("published-service", request)
        .expect("actual failed publication original")
        .expect("actual Aborted remains present");
    assert_eq!(
        aborted.phase(),
        VulcanPluginActivationTransactionPhase::Aborted
    );
    assert_eq!(aborted.predecessor(), fixture.prepared.predecessor());
    assert_eq!(aborted.target(), fixture.prepared.target());
    // These exact bytes must remain unchanged across same-request restoration retry.
    // 跨同请求恢复重试，这些精确字节必须保持不变。
    let aborted_bytes = journal_bytes(&fixture);
    // The Pending handoff keeps its frozen Prepared body; independent file contention proves physical retention.
    // Pending 交接保持其冻结 Prepared 正文；独立文件争用证明物理保留。
    let pending = fixture
        .store
        .predecessor_handoff_record("published-service", request)
        .expect("actual Pending observation")
        .expect("actual Pending reservation exists");
    assert_eq!(
        pending.phase(),
        VulcanPluginPredecessorHandoffPhase::Pending
    );
    assert_eq!(pending.original(), &fixture.prepared);
    // The independent handle opens only the actual existing file and never fabricates its lock state.
    // 独立句柄仅打开实际既有文件，绝不伪造其锁状态。
    let competing = handoff_file(&fixture);
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        fixture
            .store
            .predecessor_handoff_blocks_admission("published-service")
            .expect("actual retained Pending admission fence")
    );
    assert_owner(&fixture.state, request, true).await;
    // The core restored catalog stays closed after the failed actual service publication.
    // 实际服务发布失败之后，核心恢复目录保持关闭。
    let closed = fixture.manager.snapshot();
    assert_eq!(
        closed.plugins["published-service"].runtime_state,
        VulcanPluginRuntimeState::Enabling
    );
    assert!(Arc::ptr_eq(&previous, &full(&fixture.state)));
    // Production's wait-before-publish ordering establishes readiness; marker itself is only actual PID evidence.
    // 生产先等待后发布的顺序确立就绪；标记自身仅为实际 PID 证据。
    let actual_starts = starts(&fixture.marker);
    assert_eq!(
        actual_starts.len(),
        2,
        "expected one old A and one retained B may have changed"
    );
    // Select the distinct actual restored child by its recorded identity rather than a line number.
    // 按记录身份选择不同实际恢复子进程，不按行号选择。
    let restored_pid = actual_starts
        .iter()
        .map(|record| record["pid"].as_u64().expect("real child PID"))
        .find(|pid| *pid != fixture.original_pid)
        .expect("actual restored B differs from retired A");
    assert!(restored_pid > 0);
    crate::skills_services::service_resume_local_plugin_update(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.state),
        String::from("published-service"),
        request.to_string(),
    )
    .await
    .expect("public same-request predecessor restoration retry");
    // Genuine service Full captures this exact core final allocation, which is distinct from the closed snapshot.
    // 真实服务完整槽捕获此精确核心最终分配，区别于关闭快照。
    let committed = full(&fixture.state);
    assert!(!Arc::ptr_eq(&previous, &committed));
    assert!(!Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
    assert!(Arc::ptr_eq(
        &committed
            .plugin_catalog()
            .expect("actual restored Full catalog"),
        &fixture.manager.snapshot()
    ));
    assert_eq!(
        fixture.manager.snapshot().plugins["published-service"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert_eq!(
        invoke_pid(&committed, "same retained B after public resume").await,
        restored_pid
    );
    assert_eq!(
        starts(&fixture.marker),
        actual_starts,
        "same-request retry must not initialize another process"
    );
    assert_eq!(journal_bytes(&fixture), aborted_bytes);
    assert_eq!(
        fixture
            .store
            .activation_transaction("published-service", request)
            .expect("same complete original after retry")
            .as_ref(),
        Some(&aborted)
    );
    // Actual transfer changes only the sidecar phase and releases the original independent physical File.
    // 实际转交仅改变附属阶段，并释放原独立物理文件。
    let transferred = fixture
        .store
        .predecessor_handoff_record("published-service", request)
        .expect("actual same-request transfer")
        .expect("actual sidecar remains present");
    assert_eq!(
        transferred.phase(),
        VulcanPluginPredecessorHandoffPhase::Transferred
    );
    assert_eq!(transferred.original(), pending.original());
    competing
        .try_lock()
        .expect("actual same original reservation released after retry");
    competing
        .unlock()
        .expect("release independent post-retry probe");
    assert!(
        !fixture
            .store
            .predecessor_handoff_blocks_admission("published-service")
            .expect("actual retry cleared admission fence")
    );
    assert_owner(&fixture.state, request, false).await;
    assert!(
        !fixture.policy.exists(),
        "old restoration and retry must not persist replacement policy"
    );
    cleanup(&fixture).await;
}
