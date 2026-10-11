//! Original lifecycle observation is not physical retirement or whole-plugin drain.
//! 原生命周期观测不是物理退役或整个插件排空。

use super::*;
use owner::UninstallResources;

/// Prepare once, retain before start, and consume only after real retirement succeeds.
/// 仅准备一次，在启动之前保留，仅在真实退役成功后消费。
/// resources is the original exclusive borrow; returns its recorded retired result without replay.
/// resources 为原排他借用；返回其记录的已退役结果，不重放。
pub(super) async fn retire_callback(resources: &mut UninstallResources) -> Result<(), String> {
    if resources.callback_result.is_some() {
        return Ok(());
    }
    if resources.callback.is_none() {
        // The original removal reservation authorizes the sole guarded callback factory.
        // 原移除预留授权唯一受保护回调工厂。
        let original = resources
            .original
            .as_ref()
            .ok_or("The physical owner of the original uninstall was released.")?;
        let call = resources
            .runtime
            .prepare_deactivation_lifecycle_call(&original.selection)
            .map_err(|error| error.to_string())?;
        resources.callback = Some(call);
    }
    // Calling start again is the original protocol's idempotent once-only entry, never another preparation.
    // 再次调用 start 为原协议幂等一次性入口，绝不是再次准备。
    let call = resources
        .callback
        .as_mut()
        .ok_or("The callback handle of the original uninstall is missing.")?;
    call.start().map_err(|error| error.to_string())?;
    // Business errors remain distinct from outer retirement failures.
    // 业务错误保持独立于外层退役失败。
    let _observed_business = call.observe().await;
    call.drain_cleanup()
        .await
        .map_err(|error| error.to_string())?;
    // The same handle owns the only result, including failed business after successful physical retirement.
    // 相同句柄持有唯一结果，包括成功物理退役之后的业务失败。
    let retired_business = call
        .confirm_retired()
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string());
    resources.callback_result = Some(retired_business);
    resources.callback.take();
    Ok(())
}

/// Drain actual current, retired and unpublished MCP owners plus every Core session owner.
/// 排空实际当前、退役及未发布 MCP 所有者，以及每个核心会话所有者。
/// plugin/resources select the captured participants; returns only after true zero sessions and empty cleanup diagnostics.
/// plugin／resources 选择捕获参与者；仅在真实零会话及空清理诊断之后返回。
pub(super) async fn drain(plugin: &str, resources: &mut UninstallResources) -> Result<(), String> {
    if let Some(mcp) = resources.mcp.as_ref() {
        mcp.begin_plugin_mcp_drain(plugin)?;
        mcp.wait_plugin_mcp_drain(
            plugin,
            AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
        )
        .await?;
    } else if resources.committed_full.is_none() {
        return Err(String::from(
            "The original uninstall has no actual MCP drain owner.",
        ));
    }
    // The real manager union reports errors instead of hiding them behind a descriptor state.
    // 真实管理器集合报告错误，而非将其隐藏于描述符状态之后。
    let report = resources
        .runtime
        .drain_plugin_sessions(plugin)
        .await
        .map_err(|error| error.to_string())?;
    if !report.failures.is_empty() {
        return Err(format!(
            "Failed to drain the original uninstall resources: {}",
            report.failures.join("; ")
        ));
    }
    if resources
        .runtime
        .active_session_count(plugin)
        .map_err(|error| error.to_string())?
        != 0
        || resources
            .runtime
            .has_cleanup_diagnostics(plugin)
            .map_err(|error| error.to_string())?
    {
        return Err(String::from(
            "The actual resources of the original uninstall are not fully retired yet.",
        ));
    }
    Ok(())
}
