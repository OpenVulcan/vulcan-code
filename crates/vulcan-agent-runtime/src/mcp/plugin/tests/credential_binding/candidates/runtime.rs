//! Real candidate admission exercises immutable requests and actual subprocess ownership.
//! 真实候选入场验证不可变请求及实际子进程所有权。

use super::*;
use crate::mcp::{CandidatePluginMcpRuntime, McpRuntimeServerStatus, PreparedPluginMcpConfigs};

pub(in crate::mcp) mod published_fixture;
mod retained_validation;
use std::path::PathBuf;
use std::time::Duration;
use vulcan_plugin_runtime::{VulcanPluginActivationTransaction, VulcanPluginMcpScopeManifest};

/// One bounded observation deadline shared by this real-process fixture.
/// 此真实进程夹具共用的一项有限观察期限。
pub(in crate::mcp) const OBSERVE: Duration = Duration::from_secs(10);

/// Owns an actual published predecessor and one separately prepared immutable request.
/// 拥有实际已发布前驱及一个单独准备的不可变请求。
struct Fixture {
    /// Sealed definitions retain the original readiness and environment leases.
    /// 密封定义保留原就绪及环境租约。
    prepared: PreparedPluginMcpConfigs,
    /// The durable store supplies real cross-owner exclusion.
    /// 持久存储提供真实跨所有者互斥。
    store: VulcanPluginGenerationStore,
    /// Foreground transaction ownership can disappear independently of actual readiness work.
    /// 前台事务所有权可独立于实际就绪工作消失。
    transaction: VulcanPluginActivationTransaction,
    /// Public discovery remains on the predecessor throughout readiness.
    /// 公开发现在就绪期间始终保持前驱。
    manager: VulcanPluginRuntimeManager,
    /// The child records actual resolved configuration and credentials here.
    /// 子进程在此记录实际解析配置及凭据。
    marker: PathBuf,
    /// Last field retains all fixture paths until the resource-owning fields above are disposed.
    /// 最后字段保留全部夹具路径，直至上方拥有资源的字段销毁。
    _temp: tempfile::TempDir,
}

/// Builds a native Node package, real staged credentials and an unpublished request.
/// 构建原生 Node 插件包、真实暂存凭据及未发布请求。
/// Mode selects actual protocol behavior; returns production-prepared immutable inputs.
/// mode 选择实际协议行为；返回经生产路径准备的不可变输入。
fn fixture(mode: &str) -> Fixture {
    fixture_with_scope(mode, VulcanPluginMcpScopeManifest::Global)
}

/// Build the same actual native fixture with its explicit manifest scope fixed before package selection.
/// 在插件包选定之前固定显式清单作用域，构建同一实际原生夹具。
/// mode selects protocol behavior and scope selects the genuine declaration lifecycle; returns actual immutable inputs.
/// mode 选择协议行为，scope 选择真实声明生命周期；返回实际不可变输入。
fn fixture_with_scope(mode: &str, scope: VulcanPluginMcpScopeManifest) -> Fixture {
    // Only this temporary store and its exact copied executable participate in the test.
    // 仅此临时存储及其精确复制的可执行文件参与测试。
    let temp = tempfile::tempdir().expect("candidate runtime fixture");
    // The isolated user root owns only this fixture's immutable selections.
    // 隔离用户根仅拥有此夹具的不可变选择。
    let user = temp.path().join("user");
    // An empty bundled root prevents unrelated namespace precedence.
    // 空内置根避免无关命名空间优先级。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&user).expect("user root");
    fs::create_dir(&bundled).expect("bundled root");
    // Use production discovery and preparation without constructing a Lua engine.
    // 使用生产发现及准备，不构造 Lua 引擎。
    let manager = VulcanPluginRuntimeManager::without_runtime(
        bundled,
        user.clone(),
        temp.path().join("bridge"),
        temp.path().join("managed"),
        temp.path().join("state"),
        temp.path().join("runtime"),
    );
    // The authoritative fixture supplies supported native target declarations.
    // 权威夹具提供受支持原生目标声明。
    let source = package(&temp.path().join("source"));
    // Resolve the actual test dependency once, then package it rather than relying on child PATH.
    // 解析实际测试依赖一次，随后将其打包，不依赖子进程 PATH。
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("Node.js is required by the workspace test environment");
    assert!(node.status.success(), "Node.js executable discovery failed");
    // The successful dependency query supplies the exact executable path.
    // 成功的依赖查询提供精确可执行路径。
    let executable = String::from_utf8(node.stdout).expect("UTF-8 Node.js path");
    fs::copy(executable.trim(), source.join("bin/server.bin"))
        .expect("bundle exact Node executable");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/candidate-mcp.mjs"),
        source.join("server.mjs"),
    )
    .expect("bundle protocol fixture");
    // Modify known manifest fields from the existing authoritative native-binary fixture.
    // 修改既有权威原生二进制夹具中的已知清单字段。
    let manifest_path = source.join("plugin.yaml");
    // Parse declared fields once before replacing this test's transport definitions.
    // 在替换本测试传输定义前一次性解析声明字段。
    let mut manifest: Value =
        serde_yaml::from_str(&fs::read_to_string(&manifest_path).expect("base manifest"))
            .expect("parse base manifest");
    manifest["permissions"] = serde_json::json!([
        "config",
        "secrets_read",
        "process_binary",
        "process_session"
    ]);
    manifest["config"] =
        serde_json::json!({"schema_file":"schema.json","defaults_file":"defaults.json"});
    manifest["execution"]["dependencies"] =
        serde_json::json!({"manifest":"dependencies.yaml","install_mode":"bundled"});
    // The actual conversation contract requires one Core-owned endpoint, while global declarations prohibit it.
    // 实际会话契约要求单个 Core 自有端点，全局声明则禁止它。
    let arguments = match scope {
        VulcanPluginMcpScopeManifest::Global => {
            serde_json::json!([{"source":"literal","value":"server.mjs"}])
        }
        VulcanPluginMcpScopeManifest::Conversation => serde_json::json!([
            {"source":"literal","value":"server.mjs"}, {"source":"core_browser_endpoint"}
        ]),
    };
    manifest["mcp"]["servers"] = serde_json::json!([{
        "id":"local", "name":"Exact candidate",
        "scope": match scope {
            VulcanPluginMcpScopeManifest::Global => "global",
            VulcanPluginMcpScopeManifest::Conversation => "conversation",
        },
        "transport":{
            "type":"stdio", "launcher":{"type":"native_binary","binary_id":"server"},
            "args":arguments,
            "env":{
                "CANDIDATE_LABEL":{"source":"config","key":"label"},
                "CANDIDATE_MARKER":{"source":"config","key":"marker"},
                "CANDIDATE_MODE":{"source":"config","key":"mode"},
                "CANDIDATE_TOKEN":{"source":"secret","key":"token"}
            }
        }
    }]);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("write configured native manifest");
    fs::write(source.join("dependencies.yaml"), "{}\n").expect("bundled dependencies");
    fs::write(
        source.join("schema.json"),
        serde_json::to_vec(&serde_json::json!({
            "type":"object", "properties":{
                "label":{"type":"string"}, "marker":{"type":"string"}, "mode":{"type":"string"}
            }, "required":["label","marker","mode"], "additionalProperties":false
        }))
        .expect("schema JSON"),
    )
    .expect("write schema");
    // The real child writes its start evidence outside immutable package bytes.
    // 真实子进程在不可变包字节之外写入启动证据。
    let marker = temp.path().join("actual-starts.jsonl");
    // Both selections use a validated document, with distinct public and private labels.
    // 两次选择使用已校验文档，具有不同公开及私有标签。
    let mut configuration = serde_json::json!({"label":"public-value","marker":marker,"mode":mode});
    fs::write(
        source.join("defaults.json"),
        serde_json::to_vec(&configuration).expect("defaults JSON"),
    )
    .expect("write defaults");
    // Distinct real revisions make accidentally reading public credentials observable inside the child.
    // 不同真实修订使子进程内部可观察到意外读取公开凭据。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("inspect native package");
    // The actual generation store owns durable activation and request locks.
    // 实际代次存储拥有持久激活及请求锁。
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    // The production fingerprint pins the exact package copied for execution.
    // 生产指纹固定复制供执行的精确插件包。
    let generation = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("immutable package");
    // The published predecessor receives its own staged credential revision.
    // 已发布前驱接收独立暂存凭据修订。
    let first = VulcanPluginCredentialRevision::allocate();
    // The unpublished candidate receives a separate staged credential revision.
    // 未发布候选接收单独暂存凭据修订。
    let second = VulcanPluginCredentialRevision::allocate();
    // The same revision-aware backend verifies staging and materializes real launch values.
    // 同一识别修订的后端验证暂存并物化实际启动值。
    let secrets = RevisionSecrets {
        labels: BTreeMap::from([(first.clone(), "first"), (second.clone(), "second")]),
        reads: Mutex::new(Vec::new()),
    };
    stage_fixture_credentials(&store, &generation, &preview, &first, &secrets);
    stage_fixture_credentials(&store, &generation, &preview, &second, &secrets);
    store
        .activate_configured(
            &generation,
            0,
            preview.permissions.clone(),
            first,
            VulcanPluginConfigurationSelection::Document {
                value: configuration.clone(),
            },
        )
        .expect("publish predecessor");
    manager
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("public predecessor");
    configuration["label"] = serde_json::json!("private-candidate-value");
    // This foreground owner retains the original durable activation request.
    // 此前台所有者保留原持久激活请求。
    let transaction = store
        .begin_activation_transaction(
            uuid::Uuid::new_v4(),
            &generation,
            1,
            preview.permissions,
            second,
            VulcanPluginConfigurationSelection::Document {
                value: configuration,
            },
        )
        .expect("original candidate request");
    // Only the original inspected request supplies retained launch authority.
    // 仅原已检查请求提供保留启动权威。
    let prepared = {
        // This borrowed inspection cannot survive its foreground transaction.
        // 此借用检查不能比其前台事务存续更久。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect candidate");
        // Only the original inspected request supplies retained launch authority.
        // 仅原已检查请求提供保留启动权威。
        let prepared = manager
            .prepare_mcp_activation_candidate(&candidate)
            .expect("prepare exact launcher");
        match scope {
            VulcanPluginMcpScopeManifest::Global => {
                crate::mcp::candidate_local_mcp_server_configs(prepared, &secrets)
                    .expect("materialize exact local installation inputs")
            }
            // This unused conversation request is observed only through its genuinely empty global declarations.
            // 此未使用会话请求仅通过其真实空全局声明观察。
            VulcanPluginMcpScopeManifest::Conversation => {
                crate::mcp::candidate_global_mcp_server_configs(prepared, &secrets)
                    .expect("materialize empty global candidate without issuing a browser grant")
            }
        }
    };
    Fixture {
        prepared,
        store,
        transaction,
        manager,
        marker,
        _temp: temp,
    }
}

/// Read complete original child records within the shared finite observation budget.
/// 在共享有限观察预算内读取完整原子进程记录。
/// marker is the actual producer output path; return only genuine newline-completed startup values.
/// marker 为实际生产者输出路径；仅返回真实换行完成的启动值。
pub(in crate::mcp) async fn recorded_starts(marker: &std::path::Path) -> Vec<Value> {
    // The newline is the child writer's completion delimiter, so partial file writes remain pending.
    // 换行是子进程写入者的完成分隔符，因此部分文件写入保持等待。
    let bytes = tokio::time::timeout(OBSERVE, async {
        loop {
            if marker.is_file() {
                // Only a complete delimited record may be parsed as the child's selected inputs.
                // 仅完整分隔记录可解析为子进程选定输入。
                let bytes = fs::read_to_string(marker).expect("actual startup inputs");
                if bytes.ends_with('\n') {
                    break bytes;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual child start");
    // Multiple complete records would reveal an unauthorized automatic restart.
    // 多条完整记录会揭示未经授权的自动重启。
    bytes
        .lines()
        .map(|line| serde_json::from_str(line).expect("startup JSON"))
        .collect()
}

/// Verify one actual candidate start has the request's exact private configuration and credentials.
/// 验证单个实际候选启动具有请求精确私有配置及凭据。
/// marker is the same real child output path; no fabricated process receipt is accepted.
/// marker 为同一真实子进程输出路径；不接受伪造进程回执。
async fn assert_started_once(marker: &std::path::Path) {
    // Reuse the unique bounded producer readback before checking the candidate-specific values.
    // 检查候选专属值之前，复用唯一有界生产者读回。
    let starts = recorded_starts(marker).await;
    assert_eq!(
        starts.len(),
        1,
        "one candidate request must never restart automatically"
    );
    // Locate by the unique record invariant rather than a mutable catalog position.
    // 根据唯一记录约束定位，而非可变目录位置。
    let actual = starts
        .first()
        .expect("expected candidate startup record may have changed");
    assert_eq!(actual["label"], "private-candidate-value");
    assert_eq!(actual["token"], "second-token");
}

/// A ready candidate keeps its exact transaction fenced until real shutdown releases all authority.
/// 已就绪候选保持精确事务隔离，直至真实关闭释放全部权威。
/// No inputs or return; verifies production start, selected child inputs, public predecessor and release.
/// 无输入与返回值；验证生产启动、子进程选定输入、公开前驱及释放。
#[tokio::test]
async fn ready_candidate_preserves_predecessor_and_fences_transaction() {
    // Keep the original record to detect premature activation publication.
    // 保留原记录，以检测提前发布激活。
    let mut fixture = fixture("ready");
    // The predecessor snapshot detects any premature public activation change.
    // 前驱快照检测任何提前公开激活变更。
    let original = fixture
        .store
        .activation("bound-plugin")
        .expect("predecessor");
    // Copy the exact stable request identity without acquiring another resource lease.
    // 复制精确稳定请求身份，不获取其他资源租约。
    let request_id = fixture.transaction.record().request_id();
    // Unique control owns shutdown admission for this original candidate attempt.
    // 唯一控制拥有此原候选尝试的关闭入场。
    let control = CandidatePluginMcpRuntime::start(fixture.prepared).expect("candidate admission");
    // The cloned observer preserves diagnostics and original release evidence only.
    // 克隆观察对象仅保留诊断及原释放证据。
    let observer = control.observer();
    assert_eq!(observer.request_id(), request_id);
    assert_eq!(observer.plugin_id(), "bound-plugin");
    observer
        .wait_until_ready(OBSERVE)
        .await
        .expect("real protocol ready");
    assert_started_once(&fixture.marker).await;
    assert_eq!(
        fixture
            .store
            .activation("bound-plugin")
            .expect("current selection"),
        original
    );
    assert_eq!(
        fixture
            .manager
            .plugin_config("bound-plugin")
            .expect("public config")["label"],
        "public-value"
    );
    assert_eq!(
        fixture
            .store
            .commit_activation_transaction(&mut fixture.transaction)
            .expect_err("live candidate blocks commit")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        fixture
            .store
            .abort_activation_transaction(&mut fixture.transaction)
            .expect_err("live candidate blocks abort")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    observer
        .wait_until_released(Duration::from_millis(25))
        .await
        .expect_err("still active");
    drop(control);
    observer
        .wait_until_ready(OBSERVE)
        .await
        .expect_err("shutdown rejects stale readiness");
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("actual release");
    assert!(
        observer
            .server_states()
            .iter()
            .all(|state| state.status == McpRuntimeServerStatus::Stopped)
    );
    fixture
        .store
        .abort_activation_transaction(&mut fixture.transaction)
        .expect("release permits abort");
    assert_eq!(
        fixture
            .store
            .activation("bound-plugin")
            .expect("retained predecessor"),
        original
    );
}

/// Cancelling a readiness observer cannot release the request while the actual child still initializes.
/// 取消就绪观察者不能在实际子进程仍初始化时释放请求。
/// No inputs or return; recovery succeeds only after original control requests actual cleanup.
/// 无输入与返回值；仅原控制请求实际清理后，恢复才成功。
#[tokio::test]
async fn cancelled_candidate_observation_keeps_original_request_until_release() {
    // Use an isolated actual store and declared protocol behavior.
    // 使用隔离实际存储及声明协议行为。
    let fixture = fixture("stall");
    // Copy the exact stable request identity without acquiring another resource lease.
    // 复制精确稳定请求身份，不获取其他资源租约。
    let request_id = fixture.transaction.record().request_id();
    // Unique control owns shutdown admission for this original candidate attempt.
    // 唯一控制拥有此原候选尝试的关闭入场。
    let control = CandidatePluginMcpRuntime::start(fixture.prepared).expect("candidate admission");
    // The cloned observer preserves diagnostics and original release evidence only.
    // 克隆观察对象仅保留诊断及原释放证据。
    let observer = control.observer();
    drop(fixture.transaction);
    assert_started_once(&fixture.marker).await;
    // An independent cancelled wait does not own the transaction or lifecycle.
    // 独立的已取消等待不拥有事务或生命周期。
    let observing = observer.clone();
    // A separate task makes observer cancellation independent of lifecycle ownership.
    // 独立任务使观察取消独立于生命周期所有权。
    let waiter = tokio::spawn(async move { observing.wait_until_ready(OBSERVE).await });
    tokio::task::yield_now().await;
    waiter.abort();
    assert!(waiter.await.expect_err("wait cancelled").is_cancelled());
    assert_eq!(
        fixture
            .store
            .resume_activation_transaction("bound-plugin", request_id)
            .expect_err("actual candidate work retains original OS lock")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(control);
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("original cleanup release");
    // Resume only the same durable request after its actual resource barrier opens.
    // 仅在实际资源屏障打开后恢复同一持久请求。
    let mut resumed = fixture
        .store
        .resume_activation_transaction("bound-plugin", request_id)
        .expect("recover exact original request");
    fixture
        .store
        .abort_activation_transaction(&mut resumed)
        .expect("abort recovered request");
}

/// The service validation helper distinguishes protocol readiness from actual shutdown and resource release.
/// 服务校验辅助入口区分协议就绪、实际关闭及资源释放。
/// No inputs or return; real successful and failed children retain one-attempt semantics and release original ownership.
/// 无输入与返回值；真实成功及失败子进程保持单次尝试语义，并释放原所有权。
#[tokio::test]
async fn candidate_service_validation_requires_ready_shutdown_and_release() {
    for mode in ["ready", "fail"] {
        // Each original request gets one real immutable process and selected credential revision.
        // 每个原请求获得一个真实不可变进程及选定凭据修订。
        let mut fixture = fixture(mode);
        // Only the original control admits work and requests shutdown.
        // 仅原控制接纳工作并请求关闭。
        let control =
            CandidatePluginMcpRuntime::start(fixture.prepared).expect("candidate control");
        // Diagnostic observation retains no actual resource authority.
        // 诊断观测不保留实际资源权威。
        let observer = control.observer();
        // This is the complete helper consumed by the service update coordinator.
        // 此处为服务更新协调器消费的完整辅助入口。
        let result = control.validate_and_release().await;
        assert_eq!(
            result.is_ok(),
            mode == "ready",
            "exact protocol outcome: {result:?}"
        );
        if let Err(error) = result {
            assert!(
                error.contains("readiness:"),
                "original failure must remain observable: {error}"
            );
        }
        observer
            .wait_until_released(OBSERVE)
            .await
            .expect("original resources released");
        assert_started_once(&fixture.marker).await;
        fixture
            .store
            .abort_activation_transaction(&mut fixture.transaction)
            .expect("actual release permits original abort");
    }
}

/// Failed initialization is attempted once and releases authority without hiding its failure state.
/// 初始化失败仅尝试一次，释放权威且不隐藏其失败状态。
/// No inputs or return; a still-retained controller and observer do not obstruct actual release.
/// 无输入与返回值；仍保留的控制与观察对象不阻碍实际释放。
#[tokio::test]
async fn failed_candidate_releases_authority_without_automatic_restart() {
    // Use an isolated actual store and declared protocol behavior.
    // 使用隔离实际存储及声明协议行为。
    let mut fixture = fixture("fail");
    // Unique control owns shutdown admission for this original candidate attempt.
    // 唯一控制拥有此原候选尝试的关闭入场。
    let control = CandidatePluginMcpRuntime::start(fixture.prepared).expect("candidate admission");
    // The cloned observer preserves diagnostics and original release evidence only.
    // 克隆观察对象仅保留诊断及原释放证据。
    let observer = control.observer();
    observer
        .wait_until_ready(OBSERVE)
        .await
        .expect_err("declared protocol failure");
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("failed attempt released");
    // Retain diagnostic values independently of actual resource ownership.
    // 独立于实际资源所有权保留诊断值。
    let states = observer.server_states();
    assert!(!states.is_empty());
    assert!(
        states
            .iter()
            .all(|state| state.status == McpRuntimeServerStatus::Failed && state.retry_count == 1)
    );
    observer
        .wait_until_ready(OBSERVE)
        .await
        .expect_err("failure is retained without restart");
    assert_started_once(&fixture.marker).await;
    fixture
        .store
        .abort_activation_transaction(&mut fixture.transaction)
        .expect("diagnostic handles retain no candidate authority after failure");
}

/// Executor loss revokes registered transport ownership even while its original observer remains alive.
/// 执行器丢失时，即使原观察对象仍存活，也会撤销已注册传输所有权。
/// No inputs or return; real prepared and connected tasks are observed by a second executor.
/// 无输入与返回值；真实已准备及已连接任务由第二执行器观察。
#[test]
fn candidate_executor_loss_releases_unpolled_and_connected_work() {
    for connected in [false, true] {
        // The host executor may disappear before first poll or after real protocol readiness.
        // 宿主执行器可能在首次轮询之前或实际协议就绪之后消失。
        let fixture = fixture("ready");
        // Copy the exact stable request identity without acquiring another resource lease.
        // 复制精确稳定请求身份，不获取其他资源租约。
        let request_id = fixture.transaction.record().request_id();
        // The original runtime owns execution while observers retain only its control state.
        // 原运行时拥有执行，观察者仅保留其控制状态。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("original executor");
        // Unique control owns shutdown admission for this original candidate attempt.
        // 唯一控制拥有此原候选尝试的关闭入场。
        let control = {
            // Enter without polling tasks to exercise cancellation before first execution.
            // 进入但不轮询任务，以验证首次执行前取消。
            let _entered = runtime.enter();
            CandidatePluginMcpRuntime::start(fixture.prepared).expect("candidate admission")
        };
        // The cloned observer preserves diagnostics and original release evidence only.
        // 克隆观察对象仅保留诊断及原释放证据。
        let observer = control.observer();
        drop(fixture.transaction);
        if connected {
            runtime.block_on(async {
                observer
                    .wait_until_ready(OBSERVE)
                    .await
                    .expect("connected candidate");
                assert_started_once(&fixture.marker).await;
            });
        }
        assert_eq!(
            fixture
                .store
                .resume_activation_transaction("bound-plugin", request_id)
                .expect_err("original task owns exact request")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        drop(runtime);
        // Keep both original API handles during observation, proving they do not retain native resources.
        // 观察期间保留两个原 API 句柄，证明它们不保留原生资源。
        let recovery_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("recovery executor");
        // Observe the original aggregate receipt on the replacement executor.
        // 在替换执行器上观察原聚合回执。
        let released = recovery_runtime.block_on(observer.wait_until_released(OBSERVE));
        // Retain diagnostic values independently of actual resource ownership.
        // 独立于实际资源所有权保留诊断值。
        let states = observer.server_states();
        // Always release foreground handles before an assertion failure can unwind fixture paths.
        // 始终先释放前台句柄，再允许断言失败展开夹具路径。
        drop(control);
        drop(observer);
        released.expect("original actual resources released after executor loss");
        assert!(!states.is_empty());
        assert!(
            states
                .iter()
                .all(|state| state.status == McpRuntimeServerStatus::Failed)
        );
        // Resume only the same durable request after its actual resource barrier opens.
        // 仅在实际资源屏障打开后恢复同一持久请求。
        let mut resumed = fixture
            .store
            .resume_activation_transaction("bound-plugin", request_id)
            .expect("original request recoverable after actual release");
        fixture
            .store
            .abort_activation_transaction(&mut resumed)
            .expect("abort original recovered request");
    }
}
