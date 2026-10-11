//! The actual service slot retains old Full publication through uncertain original predecessor transfer.
//! 实际服务槽跨不确定原前驱转交保留旧 Full 发布。

use super::*;
use vulcan_agent_runtime::agent_runtime::extension_generation::ExtensionGeneration;
use vulcan_agent_runtime::mcp::PredecessorPluginMcpRuntime;
use vulcan_plugin_runtime::{
    VulcanPluginActivationPredecessorPublication, VulcanPluginActivationTransaction,
    VulcanPluginActivationTransactionPhase, VulcanPluginCatalogSnapshot,
    VulcanPluginNativeMcpPredecessorHandoffAdapter, VulcanPluginPredecessorRestorationSource,
    VulcanPluginRegisteredNativeMcpHostAuthority, VulcanPluginRuntimeManager,
};

/// Genuine old Full publication pairs its original sealed catalog with the actual service commit.
/// 真实旧 Full 发布将其原封闭目录与实际服务提交配对。
/// This pair grants no admission; only same-source joint Transferred completion opens its actual owners.
/// 此配对不授予入场；仅同来源联合 Transferred 完成开放其实际所有者。
pub(in crate::skills_services) struct PredecessorPluginExtensionPublication {
    /// The original core publisher alone produces this exact successful old Full witness.
    /// 仅原核心发布器产出此精确成功旧 Full 凭据。
    publication: VulcanPluginActivationPredecessorPublication,
    /// The actual committed Full keeps the same catalog, inert handlers and actual original native lease together.
    /// 实际已提交 Full 将相同目录、惰性处理器及实际原原生租约保留在一起。
    committed: Arc<ExtensionGeneration>,
    /// The first service registration is pure identity and retains neither recovery File nor physical resource ownership.
    /// 首次服务注册为纯身份，不保留恢复文件或物理资源所有权。
    authority: VulcanPluginRegisteredNativeMcpHostAuthority,
}

impl PredecessorPluginExtensionPublication {
    /// Verify the same completed Full still occupies state for cleanup retry, without rerunning transfer or initialization.
    /// 校验相同已完成 Full 仍占据 state，供清理重试，不重新运行转交或初始化。
    /// runtime must be the original actual manager; return an error for substituted service slot, catalog or first issuer.
    /// runtime 必须为原实际管理器；替换服务槽、目录或首次签发者时返回错误。
    /// Caller must independently retain confirmed completion; this observation grants no completion or business authority.
    /// 调用方必须独立保留已确认完成；此观测不授予完成或业务权威。
    pub(in crate::skills_services) fn validate_current(
        &self,
        state: &SharedServiceBridgeState,
        runtime: &VulcanPluginRuntimeManager,
    ) -> Result<(), String> {
        // Observe the actual bridge under the same service exclusion used by original publication.
        // 在原发布使用的相同服务互斥下观测实际桥。
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock the service state: {error}"))?;
        // Missing bridge ownership cannot be reconstructed from the sealed catalog's copied values.
        // 缺失桥所有权不能从封闭目录复制值重建。
        let bridge = guard
            .agent_runtime
            .as_ref()
            .ok_or("The agent extension generation is not initialized yet.")?;
        if !self.authority.matches_manager(runtime)
            || !self
                .authority
                .matches_manager(&bridge.vulcan_plugin_runtime)
            || !self
                .authority
                .same_issuer(&bridge.vulcan_plugin_native_mcp_host)
            || !Arc::ptr_eq(&bridge.extension_generation, &self.committed)
            || !Arc::ptr_eq(&runtime.snapshot(), self.publication.catalog())
        {
            return Err(String::from(
                "The confirmed original predecessor publication no longer matches the actual service slot, catalog or first issuer.",
            ));
        }
        Ok(())
    }
}

/// Publish and jointly transfer an actual restored predecessor while retaining service exclusion and all original owners.
/// 保留服务互斥及全部原所有者时发布并联合转交实际恢复前驱。
/// state/transaction/source/staged identify the actual service, original Aborted request and exact closed old route.
/// state／transaction／source／staged 标识实际服务、原 Aborted 请求及精确关闭旧路由。
/// native is present exactly for declared global MCP; retained stores the real seal/Full pair before any fallible Transferred write.
/// 仅声明全局 MCP 时存在 native；retained 在任一可失败 Transferred 写入前存入真实封闭／Full 配对。
/// Errors preserve the pair and actual owners; retries complete only the same current Full without rollback or republication.
/// 错误保留配对及实际所有者；重试仅完成相同当前 Full，不回退或重新发布。
pub(in crate::skills_services) fn publish_activation_predecessor_plugin_extension_generation(
    state: &SharedServiceBridgeState,
    transaction: &VulcanPluginActivationTransaction,
    source: &Arc<VulcanPluginPredecessorRestorationSource>,
    staged: &Arc<VulcanPluginCatalogSnapshot>,
    native: Option<&PredecessorPluginMcpRuntime>,
    retained: &mut Option<PredecessorPluginExtensionPublication>,
) -> Result<(), String> {
    // Service exclusion precedes core registration and every native/source/resource guard.
    // 服务互斥先于核心注册及每个原生／来源／资源守卫。
    let mut guard = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
    crate::service_state_access::require_plugin_update_full_publication_admission(
        &guard,
        Some(transaction.record().request_id()),
    )?;
    // This is the actual bridge and single Full slot consumed by new executions.
    // 此为新执行消费的实际桥及单个 Full 槽。
    let bridge = guard
        .agent_runtime
        .as_mut()
        .ok_or("The agent extension generation is not initialized yet.")?;
    if !bridge
        .vulcan_plugin_native_mcp_host
        .matches_manager(&bridge.vulcan_plugin_runtime)
    {
        return Err(String::from(
            "The original predecessor manager disagrees with the first native issuer of the service.",
        ));
    }
    // Only original predecessor selection determines the old namespace, independently of the aborted target.
    // 仅原前驱选择决定旧命名空间，独立于中止目标。
    let old = transaction
        .record()
        .predecessor()
        .ok_or("The original predecessor has no complete managed selection.")?;
    // This immutable closed observation does not grant Source eligibility; the first actual core publisher checks it independently.
    // 此不可变关闭观测不授予 Source 资格；首次实际核心发布器独立检查它。
    let plugin = staged
        .plugins
        .get(&old.plugin_id)
        .ok_or("The original predecessor is not in the shutdown catalog.")?;
    if transaction.record().phase() != VulcanPluginActivationTransactionPhase::Aborted
        || source.request_id() != transaction.record().request_id()
        || !plugin
            .managed_package()
            .is_some_and(|managed| managed.matches_activation(old))
    {
        return Err(String::from(
            "The original predecessor publication requires the exact complete aborted old selection.",
        ));
    }
    // Enforce actual declarations on every attempt, including exact Full completion retries.
    // 在每次尝试中执行实际声明约束，包括精确 Full 完成重试。
    let has_global_mcp = plugin.mcp_servers.iter().any(|server| {
        server.manifest.scope == vulcan_plugin_runtime::VulcanPluginMcpScopeManifest::Global
    });
    if has_global_mcp != native.is_some() {
        return Err(String::from(
            "The global MCP declaration of the original predecessor disagrees with the actual owner.",
        ));
    }
    if let Some(native) = native {
        // A repeated issuer or equal-value Source cannot substitute for the actual original participant.
        // 重复签发者或相等值 Source 不能替代实际原参与者。
        let authority = native
            .authority()
            .ok_or("The original predecessor has no native issuer.")?;
        // Pure control observations identify the actual original allocation without acquiring its physical files.
        // 纯控制观测标识实际原分配，不取得其物理文件。
        let control = native.control();
        if !authority.same_issuer(&bridge.vulcan_plugin_native_mcp_host)
            || !control.matches_source(source)
            || control.request_id() != source.request_id()
            || control.plugin_id() != old.plugin_id
            || control.generation() != plugin.generation
        {
            return Err(String::from(
                "The original predecessor disagrees with the first issuer or actual source of the service.",
            ));
        }
    }
    // Clone the same manager, not its catalog or physical resource authority.
    // 克隆相同管理器，而非其目录或物理资源权威。
    let manager = bridge.vulcan_plugin_runtime.clone();
    if let Some(previous) = retained.as_ref() {
        // Unknown transfer may only reconcile the same successful Full; it never republishes a closed Target catalog.
        // 未知转交仅可对账相同成功 Full；绝不重新发布关闭 Target 目录。
        if !previous.authority.matches_manager(&manager)
            || !previous
                .authority
                .same_issuer(&bridge.vulcan_plugin_native_mcp_host)
            || !Arc::ptr_eq(&bridge.extension_generation, &previous.committed)
            || !Arc::ptr_eq(&manager.snapshot(), previous.publication.catalog())
        {
            return Err(String::from(
                "The actual Full state or final catalog of the original predecessor changed; keeping the original owner, it cannot be published again.",
            ));
        }
    } else {
        // Only this production publisher can produce the genuine successful core/Full pair.
        // 仅此生产发布器可产出真实成功核心／Full 配对。
        let (publication, committed) = bridge
            .publish_activation_predecessor_plugin_extension_generation(
                transaction,
                source,
                staged,
                native,
            )?;
        // Preserve the successful actual pair synchronously before the first fallible independent Transferred operation.
        // 在首个可失败独立 Transferred 操作前同步保留成功实际配对。
        *retained = Some(PredecessorPluginExtensionPublication {
            publication,
            committed,
            authority: bridge.vulcan_plugin_native_mcp_host.clone(),
        });
    }
    // Only the retained actual old Full seal may authorize joint same-source memory transfer and unique File release.
    // 仅保留实际旧 Full 封闭凭据可授权联合同来源内存转交及唯一文件释放。
    let publication = &retained
        .as_ref()
        .expect("successful actual predecessor publication is retained")
        .publication;
    match native {
        Some(native) => manager.complete_activation_predecessor_handoff_with(
            transaction,
            source,
            publication,
            native,
        ),
        None => manager.complete_activation_predecessor_handoff(transaction, source, publication),
    }
    .map_err(|error| error.to_string())
}
