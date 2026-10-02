//! The actual service slot retains selected-target publication through uncertain completion.
//! 实际服务槽在不确定完成期间保留已选目标发布。

use super::*;
use vulcan_agent_runtime::agent_runtime::extension_generation::ExtensionGeneration;
use vulcan_agent_runtime::mcp::TargetPluginMcpRuntime;
use vulcan_plugin_runtime::{
    VulcanPluginActivationTargetPublication, VulcanPluginActivationTransaction,
    VulcanPluginCatalogSnapshot, VulcanPluginNativeMcpHandoffAdapter, VulcanPluginRuntimeManager,
};

/// One original successful publisher binds its sealed catalog to the actual committed service pointer.
/// 单个原成功发布器将封闭目录绑定至实际已提交服务指针。
/// This retained witness grants no independent admission; only combined original completion transfers resources.
/// 此保留凭据不独立授予入场；仅组合原完成转交资源。
pub(in crate::skills_services) struct TargetPluginExtensionPublication {
    /// Only the real successful core publisher can produce this exact original request witness.
    /// 仅真实成功核心发布器可以产出此精确原请求凭据。
    publication: VulcanPluginActivationTargetPublication,
    /// The same actual service pointer pins its prepared Hook handlers and original MCP lease.
    /// 相同实际服务指针固定已准备 Hook 处理器及原 MCP 租约。
    committed: Arc<ExtensionGeneration>,
}

impl TargetPluginExtensionPublication {
    /// Verify confirmed original publication still occupies the service slot without re-running completion or native preparation.
    /// 校验已确认原发布仍占据服务槽，不重新运行完成或原生准备。
    /// state is the actual publisher and runtime is the retained original manager; returns a conflict when either authority changed.
    /// state 为实际发布器，runtime 为保留原管理器；任一权威改变时返回冲突。
    pub(in crate::skills_services) fn validate_current(
        &self,
        state: &SharedServiceBridgeState,
        runtime: &VulcanPluginRuntimeManager,
    ) -> Result<(), String> {
        // This short service observation follows the same publication lock order as the actual commit.
        // 此短暂服务观察遵循与实际提交相同的发布锁序。
        let guard = state
            .lock()
            .map_err(|error| format!("锁定服务状态失败：{error}"))?;
        // Actual pointer identity cannot be replaced by equal catalog fields or diagnostic revision numbers.
        // 实际指针身份不能由相等目录字段或诊断修订号替代。
        let bridge = guard
            .agent_runtime
            .as_ref()
            .ok_or("Agent 扩展代次尚未初始化。")?;
        if !bridge
            .vulcan_plugin_native_mcp_host
            .matches_manager(runtime)
            || !Arc::ptr_eq(&bridge.extension_generation, &self.committed)
            || !Arc::ptr_eq(&runtime.snapshot(), self.publication.catalog())
        {
            return Err(String::from(
                "原更新已确认发布不再匹配实际服务槽或原管理器。",
            ));
        }
        Ok(())
    }
}

/// Publish and complete a selected update while retaining the same service exclusion and all original owners.
/// 持续保留相同服务互斥及全部原所有者，发布并完成已选更新。
/// state owns the actual slot; transaction/staged/native are the original request, closed catalog and declared MCP participant.
/// state 拥有实际槽；transaction／staged／native 为原请求、关闭目录及已声明 MCP 参与者。
/// retained is stored in the original service request before completion; errors preserve it for exact recovery without replay.
/// retained 在完成之前存入原服务请求；错误保留它，以供不重放的精确恢复。
/// Success confirms both durable completion and same-instance memory transfer; this function never starts work or awaits.
/// 成功确认持久完成及同实例内存转交；此函数绝不启动工作或等待。
pub(in crate::skills_services) fn publish_activation_target_plugin_extension_generation(
    state: &SharedServiceBridgeState,
    transaction: &mut VulcanPluginActivationTransaction,
    staged: &Arc<VulcanPluginCatalogSnapshot>,
    native: Option<&TargetPluginMcpRuntime>,
    retained: &mut Option<TargetPluginExtensionPublication>,
) -> Result<(), String> {
    // Service ownership precedes manager registration and every real native/source guard.
    // 服务所有权先于管理器注册及每个真实原生／来源守卫。
    let mut guard = state
        .lock()
        .map_err(|error| format!("锁定服务状态失败：{error}"))?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
    crate::service_state_access::require_plugin_update_full_publication_admission(
        &guard,
        Some(transaction.record().request_id()),
    )?;
    // Installation exclusion is checked before manager registration, without acquiring another owner's resource slot.
    // 安装互斥在管理器注册之前检查，不取得另一所有者的资源槽。
    let current_catalog = guard
        .agent_runtime
        .as_ref()
        .and_then(|bridge| bridge.extension_generation.plugin_catalog());
    guard
        .pending_local_plugin_installs
        .require_catalog_publication(current_catalog.as_deref(), staged)?;
    // This original bridge holds the sole pointer consumed by new executions.
    // 此原桥持有新执行消费的唯一指针。
    let bridge = guard
        .agent_runtime
        .as_mut()
        .ok_or("Agent 扩展代次尚未初始化。")?;
    if !bridge
        .vulcan_plugin_native_mcp_host
        .matches_manager(&bridge.vulcan_plugin_runtime)
    {
        return Err(String::from("原更新管理器与服务首次原生签发者不一致。"));
    }
    if let Some(native) = native {
        // Recovery cannot substitute a second issuer registered on the same manager.
        // 恢复不能替换为相同管理器上登记的第二签发者。
        let authority = native.authority().ok_or("原更新目标缺少原生签发者。")?;
        if !authority.same_issuer(&bridge.vulcan_plugin_native_mcp_host) {
            return Err(String::from("原更新目标与服务首次原生签发者不一致。"));
        }
    }
    // Retain only the actual manager authority; the original transaction never leaves its request slot.
    // 仅保留实际管理器权威；原事务绝不离开请求槽。
    let manager = bridge.vulcan_plugin_runtime.clone();
    // A prior full commit may be followed only by completion or by exact closed-Arc recovery.
    // 先前完整提交之后，仅可继续完成或精确关闭 Arc 恢复。
    let publish = if let Some(previous) = retained.as_ref() {
        if !Arc::ptr_eq(&bridge.extension_generation, &previous.committed) {
            return Err(String::from(
                "原更新已提交完整代次不再是实际服务槽，保留原所有者。",
            ));
        }
        // Current manager identity distinguishes retryable original rollback from unrelated route changes.
        // 当前管理器身份区分可重试原回退及无关路由变化。
        let current = manager.snapshot();
        if Arc::ptr_eq(&current, previous.publication.catalog()) {
            false
        } else if Arc::ptr_eq(&current, staged) {
            true
        } else {
            return Err(String::from(
                "原更新目录已被其他发布改变，不能重建目标或替换原凭据。",
            ));
        }
    } else {
        true
    };
    if publish {
        // Only this real production publisher produces the exact full/sealed pair; no no-op publisher substitutes it.
        // 仅此真实生产发布器产出精确完整／封闭配对；无操作发布器不能替代它。
        let (publication, committed) = bridge
            .publish_activation_target_plugin_extension_generation(transaction, staged, native)?;
        // Preserve successful publication before the first fallible Completed operation.
        // 在首个可失败 Completed 操作之前保留成功发布。
        *retained = Some(TargetPluginExtensionPublication {
            publication,
            committed,
        });
    }
    // Only this exact successful pair may authorize original native and Hook transfer.
    // 仅此精确成功配对可以授权原原生及 Hook 转交。
    let publication = &retained
        .as_ref()
        .expect("successful actual target publication is retained")
        .publication;
    match native {
        Some(native) => manager
            .complete_activation_target_handoff_with(transaction, publication, native)
            .map_err(|error| error.to_string()),
        None => manager
            .complete_activation_target_handoff(transaction, publication)
            .map_err(|error| error.to_string()),
    }
}
