//! The real formal startup producer transfers one child and closes original work on late service failure.
//! 真实正式启动生产者转交单个子进程，并在服务后期失败时关闭原工作。

use super::*;
use crate::service_bootstrap::mcp_startup::PluginMcpBootstrapPlan;
use vulcan_agent_runtime::mcp::{
    McpRuntimeToolAllowlist, McpServerSource, McpServerTransportConfig,
    replace_user_mcp_runtime_partition,
};

/// Poll only genuine user tool discovery; the existing observation budget still bounds the entire wait.
/// 仅轮询真正用户工具发现；既有观察预算仍限制整个等待。
const USER_DISCOVERY_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

/// Disable the actually retired source and verify no retained observation blocks real environment repair.
/// 停用实际已退役来源，验证没有保留观察阻挡真实环境修复。
/// manager owns this isolated root, environment identifies its real native directory and temp bounds deletion.
/// manager 拥有此隔离根，environment 标识其真实原生目录，temp 限定删除范围。
/// No result; assertions exercise the production filesystem owner after actual physical release.
/// 无结果；断言在实际物理释放之后执行生产文件系统所有者。
fn verify_retired_environment(
    manager: &VulcanPluginRuntimeManager,
    environment: &std::path::Path,
    temp: &tempfile::TempDir,
) {
    manager
        .begin_draining("published-service")
        .expect("actual disabling source");
    manager
        .finish_lifecycle_transition(
            "published-service",
            &BTreeSet::from([String::from("published-service")]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("actual disabled source");
    assert!(environment.starts_with(temp.path().canonicalize().expect("isolated fixture root")));
    std::fs::remove_file(environment.join("bin/server.bin"))
        .expect("remove isolated retired binary");
    assert_eq!(
        manager
            .prepare_plugin_environment("published-service")
            .expect("startup observation does not pin environment"),
        environment
    );
}

/// Formal bootstrap uses the first issuer and one native factory, and final readiness leaves the same child callable.
/// 正式引导使用首次签发者及单个原生工厂，最终就绪使同一子进程保持可调用。
/// No inputs or return; exact source, actual full pointer projection and PID equality are independently verified.
/// 无输入或返回；独立验证精确来源、实际完整指针投影及 PID 相等。
#[tokio::test]
async fn service_formal_bootstrap_commits_same_original_and_preserves_user_declaration() {
    // The shared fixture prepares real files and full credentials but has dispatched no native task.
    // 共享夹具准备真实文件及完整凭据，但尚未派发原生任务。
    let Fixture {
        state,
        configs,
        marker,
        environment,
        _temp,
    } = fixture();
    // Acquire exclusive unpublished bridge ownership, matching formal bootstrap rather than a copied service slot.
    // 取得排他未发布桥所有权，与正式引导一致，不使用复制服务槽。
    let mut bridge = state
        .lock()
        .expect("actual service fixture lock")
        .agent_runtime
        .take()
        .expect("original unpublished bridge");
    // Both authorities belong to this one original bridge and manager.
    // 两项权威均属于此单个原桥及管理器。
    let manager = bridge.vulcan_plugin_runtime.clone();
    // Retain the original first issuer without registering a replacement.
    // 保留原首次签发者，不注册替代者。
    let authority = bridge.vulcan_plugin_native_mcp_host.clone();
    // The current immutable catalog is the original selected full source.
    // 当前不可变目录为原已选完整来源。
    let catalog = manager.snapshot();
    // A disabled user declaration is an explicit user-owned input, independent of the plugin native producer.
    // 已停用用户声明为明确用户拥有输入，与插件原生生产者独立。
    let mut user = configs
        .servers()
        .iter()
        .find(|server| server.id == "published-service:actual")
        .expect("actual fixture declaration may have changed")
        .clone();
    user.id = String::from("user-disabled-bootstrap");
    user.source = McpServerSource::UserConfigured;
    user.enabled = false;
    drop(configs);
    // An equal-value copy cannot choose a new source or trigger a legacy attempt after rejection.
    // 等值副本不能选择新来源，拒绝后也不能触发旧入口尝试。
    let copied = Arc::new((*catalog).clone());
    assert!(
        PluginMcpBootstrapPlan::prepare(&manager, &copied, &authority, &EmptyCredentials).is_err()
    );
    assert!(
        !marker.exists(),
        "no actual child may start during invalid source preparation"
    );
    // The production selector consumes the actual complete selection and original first issuer.
    // 生产选择器消费实际完整选择及原首次签发者。
    let plan = PluginMcpBootstrapPlan::prepare(&manager, &catalog, &authority, &EmptyCredentials)
        .expect("formal original bootstrap plan");
    // Every original starts once before this real readiness wait returns.
    // 在此真实就绪等待返回之前，每个原实例仅启动一次。
    let pending = plan
        .start(&manager, &authority, vec![user])
        .await
        .expect("original bootstrap readiness");
    // Select independent original observation by stable identity rather than evolving collection positions.
    // 按稳定身份选择独立原观察，不使用会演进的集合位置。
    let observation = pending
        .observations()
        .into_iter()
        .find(|observer| observer.identity().0 == "published-service")
        .expect("expected original bootstrap observer may have changed");
    observation
        .verify_source("published-service:actual", &observation.profile())
        .expect("same actual dispatched full source");
    assert!(
        pending
            .access()
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    bridge.extension_generation = bridge
        .prepare_plugin_extension_generation(Arc::clone(&catalog), pending.access())
        .expect("actual initial unpublished bootstrap slot");
    // The production commit consumes original closed controls and writes the real bridge pointer.
    // 生产提交消费原关闭控制，并写入真实桥指针。
    let published = pending
        .publish(&mut bridge, &catalog)
        .expect("formal bootstrap full commit");
    // Projection comes from that actual final pointer and includes the retained user declaration.
    // 投影来自该实际最终指针，并包含保留用户声明。
    let access = bridge.extension_generation.mcp_runtime();
    // This same runtime access is the source used by the formal startup workbench projection.
    // 此同一运行时访问为正式启动工作台投影使用的来源。
    let projection = vulcan_service_runtime::mcp_bridge::mcp_runtime_catalog_from_access(&access);
    assert_eq!(
        projection.configured_server_count, 2,
        "expected full and user declaration counts may have changed"
    );
    assert_eq!(
        projection.connected_server_count, 1,
        "expected actual native discovery count may have changed"
    );
    // Final readiness disarms only original startup cleanup, without transferring or creating another native lease.
    // 最终就绪仅解除原启动清理，不转交或创建另一个原生租约。
    published.commit();
    state
        .lock()
        .expect("actual final service ownership")
        .agent_runtime = Some(bridge);
    // The original child alone wrote this one completion record.
    // 仅原子进程写入此单个完成记录。
    let starts = std::fs::read_to_string(&marker).expect("actual startup record");
    assert_eq!(
        starts.lines().count(),
        1,
        "expected single actual start may have changed"
    );
    // Compare machine-readable actual child identity against its independent business response.
    // 将机器可读实际子进程身份与独立业务响应比较。
    let actual: serde_json::Value =
        serde_json::from_str(starts.lines().next().expect("actual first record"))
            .expect("actual process JSON");
    // Tool selection derives from actual discovery in the committed full pointer.
    // 工具选择来自已提交完整指针中的实际发现。
    let binding = access
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "published-service:actual/original_process")
        .expect("expected original business tool may have changed");
    // A completed bootstrap must leave that same real process callable after all foreground controls have dropped.
    // 完成引导必须在全部前台控制销毁后使该同一真实进程保持可调用。
    let result = binding
        .invoke(serde_json::json!({"message":"formal bootstrap original"}))
        .await
        .expect("same original after final bootstrap commit");
    assert_eq!(result["pid"], actual["pid"]);
    assert!(actual["pid"].as_u64().expect("actual native PID") > 0);
    assert_eq!(result["message"], "formal bootstrap original");
    assert_eq!(
        std::fs::read_to_string(&marker).expect("same original record"),
        starts
    );
    access
        .begin_plugin_mcp_drain("published-service")
        .expect("actual original drain");
    access
        .wait_plugin_mcp_drain("published-service", OBSERVE)
        .await
        .expect("actual original drained");
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("actual original physical release");
    verify_retired_environment(&manager, &environment, &_temp);
}

/// A late startup error closes plugin and user replacement originals while the real successor and bindings own their leases.
/// 启动后期错误关闭插件及用户替换原实例，同时真实后继及绑定仍拥有其租约。
/// No inputs or return; actual release and real environment repair prove cleanup without relying on dropped execution owners.
/// 无输入或返回；实际释放及真实环境修复证明清理，不依赖执行所有者销毁。
#[tokio::test]
async fn service_formal_bootstrap_late_failure_closes_retained_actual_service_original() {
    // Actual immutable package construction starts no process before this production plan.
    // 实际不可变插件包构造在此生产计划之前不启动进程。
    let Fixture {
        state,
        configs,
        marker,
        environment,
        _temp,
    } = fixture();
    // One original bridge remains exclusively owned by bootstrap until its full native commit.
    // 单个原桥在其完整原生提交之前保持由引导排他拥有。
    let mut bridge = state
        .lock()
        .expect("actual original bridge lock")
        .agent_runtime
        .take()
        .expect("actual unpublished bridge");
    // Read only this original manager rather than discovering another source.
    // 仅读取此原管理器，不发现另一个来源。
    let manager = bridge.vulcan_plugin_runtime.clone();
    // Keep the exact same first service registration throughout preparation and publication.
    // 跨准备及发布保留精确同一首次服务登记。
    let authority = bridge.vulcan_plugin_native_mcp_host.clone();
    // The actual current full catalog remains unchanged during this real bootstrap.
    // 实际当前完整目录在此真实引导期间保持不变。
    let catalog = manager.snapshot();
    // User configuration explicitly selects the real inspected native executable without plugin source certification.
    // 用户配置明确选择真实已检查原生可执行文件，不具有插件来源认证。
    let mut user = configs
        .servers()
        .iter()
        .find(|server| server.id == "published-service:actual")
        .expect("actual native fixture declaration may have changed")
        .clone();
    user.id = String::from("user-late-bootstrap");
    user.source = McpServerSource::UserConfigured;
    // A separate child-owned record distinguishes actual user startup from the sealed plugin original.
    // 独立子进程拥有记录区分实际用户启动及密封插件原实例。
    let user_marker = _temp.path().join("actual-user-starts.jsonl");
    match &mut user.transport {
        McpServerTransportConfig::Stdio { env, .. } => {
            env.insert(
                String::from("CANDIDATE_MARKER"),
                user_marker.to_string_lossy().into_owned(),
            );
            env.insert(
                String::from("CANDIDATE_LABEL"),
                String::from("actual-user-replacement"),
            );
        }
        _ => panic!("expected actual native stdio declaration may have changed"),
    }
    drop(configs);
    // Consume real full sources through the same factory called by formal startup.
    // 经正式启动调用的同一工厂消费真实完整来源。
    let plan = PluginMcpBootstrapPlan::prepare(&manager, &catalog, &authority, &EmptyCredentials)
        .expect("actual full startup plan");
    // This actual return follows complete original native discovery, with business still closed.
    // 此实际返回发生在完整原原生发现之后，业务仍关闭。
    let pending = plan
        .start(&manager, &authority, Vec::new())
        .await
        .expect("actual original discovery");
    // Independent cleanup observation owns neither the real execution lease nor the environment file.
    // 独立清理观察既不拥有真实执行租约，也不拥有环境文件。
    let observation = pending
        .observations()
        .into_iter()
        .find(|observer| observer.identity().0 == "published-service")
        .expect("expected actual startup original may have changed");
    bridge.extension_generation = bridge
        .prepare_plugin_extension_generation(Arc::clone(&catalog), pending.access())
        .expect("actual original lineage slot");
    // Keep the cleanup owner armed after real publication, exactly as the fallible workbench stage does.
    // 真实发布之后保持清理所有者启用，与可失败工作台阶段完全一致。
    let published = pending
        .publish(&mut bridge, &catalog)
        .expect("actual full commit before late failure");
    // This real service-access generation intentionally remains strong across cleanup.
    // 此真实服务访问代次有意跨清理保持强引用。
    let access = bridge.extension_generation.mcp_runtime();
    // A real discovered binding also remains an execution owner; resource release must follow explicit original shutdown.
    // 真实发现绑定也保持执行所有者；资源释放必须遵循明确原关闭。
    let binding = access
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "published-service:actual/original_process")
        .expect("expected actual original binding may have changed");
    state
        .lock()
        .expect("retain actual service owner")
        .agent_runtime = Some(bridge);
    // The real configuration transaction creates a user successor before final bootstrap readiness commits.
    // 真实配置事务在最终引导就绪提交之前创建用户后继。
    let replacement = {
        // Production mutations serialize through this original configuration authority.
        // 生产变更经此原配置权威串行化。
        let _configuration_transaction =
            crate::service_state_access::begin_provider_configuration_transaction().await;
        // The replacement is actually started through the public user factory on the original bootstrap lineage.
        // 替换经公开用户工厂，在原引导谱系上实际启动。
        let replacement = replace_user_mcp_runtime_partition(&access, vec![user.clone()])
            .await
            .expect("actual user replacement during late bootstrap");
        // This mutation writes the sole real full service pointer and preserves the sealed plugin execution lease.
        // 此变更写入唯一真实完整服务指针，并保留密封插件执行租约。
        let configuration = crate::service_state_access::provider_configuration_from_state(&state)
            .expect("actual service configuration");
        crate::settings_runtime_projection::store_provider_and_mcp_configuration_after_change(
            &state,
            configuration,
            replacement.clone(),
        )
        .expect("actual user successor publication");
        replacement
    };
    // Keep the actual full successor alive through failure, excluding lease destruction as cleanup evidence.
    // 跨失败保留实际完整后继，排除租约析构作为清理证据。
    let full_successor = Arc::clone(
        &state
            .lock()
            .expect("actual user full successor lock")
            .agent_runtime
            .as_ref()
            .expect("actual user full successor bridge")
            .extension_generation,
    );
    assert!(
        full_successor
            .mcp_runtime()
            .shares_server_partition(&replacement, "user-late-bootstrap")
    );
    assert!(
        full_successor
            .mcp_runtime()
            .shares_server_partition(&access, "published-service:actual")
    );
    // A real discovered binding retains the new child independently of both access generations.
    // 真实发现绑定独立于两个访问代次保留新子进程。
    let user_binding = tokio::time::timeout(OBSERVE, async {
        loop {
            if let Some(binding) = full_successor
                .mcp_runtime()
                .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
                .into_iter()
                .find(|binding| binding.origin_tool_id() == "user-late-bootstrap/original_process")
            {
                return binding;
            }
            tokio::time::sleep(USER_DISCOVERY_POLL_INTERVAL).await;
        }
    })
    .await
    .expect("actual user initialization and discovery");
    // Both independent native processes answer before the original startup cleanup owner is triggered.
    // 原启动清理所有者触发之前，两个独立原生进程均应答。
    let user_before = user_binding
        .invoke(serde_json::json!({"message":"actual user before late failure"}))
        .await
        .expect("actual latest user callable before late failure");
    // The genuine marker provides independent native identity without synthesized host receipts.
    // 真正标记提供独立原生身份，不使用合成宿主回执。
    let user_starts = std::fs::read_to_string(&user_marker).expect("actual user startup record");
    assert_eq!(
        user_starts.lines().count(),
        1,
        "expected unique user start may have changed"
    );
    // Parse the actual child's sole original record after its successful business response.
    // 在成功业务应答之后解析实际子进程唯一原记录。
    let user_actual: serde_json::Value = serde_json::from_str(
        user_starts
            .lines()
            .next()
            .expect("actual user original record"),
    )
    .expect("actual user native PID record");
    assert_eq!(user_before["pid"], user_actual["pid"]);
    assert!(user_actual["pid"].as_u64().expect("actual user PID") > 0);
    drop(published);
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("same original released on late startup failure");
    access
        .wait_user_mcp_drain(OBSERVE)
        .await
        .expect("original startup owner retires the real published user successor");
    assert!(
        binding
            .invoke(serde_json::json!({"message":"after late startup failure"}))
            .await
            .is_err()
    );
    assert!(
        user_binding
            .invoke(serde_json::json!({"message":"user after late startup failure"}))
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(&user_marker).expect("no actual user replay"),
        user_starts,
    );
    assert!(
        replace_user_mcp_runtime_partition(&replacement, vec![user])
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(&user_marker).expect("closed actual user admission"),
        user_starts,
    );
    assert_eq!(
        std::fs::read_to_string(&marker)
            .expect("actual starts")
            .lines()
            .count(),
        1,
        "expected unique original startup count may have changed"
    );
    assert!(
        state
            .lock()
            .expect("actual retained service")
            .agent_runtime
            .is_some()
    );
    verify_retired_environment(&manager, &environment, &_temp);
}
