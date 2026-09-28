//! Native service dispatch with immutable identity and the original operation budget.
//! 使用不可变身份及原始操作预算的原生服务分发。

use super::*;
use std::path::Path;

/// Dispatch validated `name` and business arguments with the retained `domain` and bounded `deadline`.
/// 使用保留的 `domain` 及有界 `deadline` 分发已校验 `name` 和业务参数。
/// The canonical `workspace` was selected by native admission from the module or exact request scope.
/// 规范 `workspace` 由原生入场从模块或精确请求作用域选择。
/// Preparation failures have no effect; service execution never invents a commit acknowledgement.
/// 准备失败没有副作用；服务执行绝不虚构提交确认。
pub(super) fn execute(
    state: &Arc<VulcanPluginManagerState>,
    domain: &ExecutionAuthority,
    workspace: &Path,
    name: &str,
    invocation: &CapabilityInvocation,
    deadline: Instant,
) -> CapabilityOutcome {
    // Preparation remains outside the effect boundary and releases all service locks.
    // 准备阶段位于副作用边界之外，并释放全部服务锁。
    let prepared = (|| {
        // The core schema guarantees an object; keep a diagnostic for invariant failure.
        // 核心 Schema 保证对象形状；保留不变量失败诊断。
        let arguments = invocation
            .arguments
            .as_object()
            .ok_or_else(|| internal("Native capability arguments are not an object."))?;
        match name {
            VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL => {
                Ok(effect(state, domain, name, invocation, deadline, || {
                    super::super::super::process::resolve_plugin_process_for_snapshot(
                        state,
                        &domain.plugin,
                        workspace,
                        arguments,
                    )
                }))
            }
            VULCAN_PLUGIN_CONFIG_GET_TOOL => {
                Ok(effect(state, domain, name, invocation, deadline, || {
                    VulcanPluginRuntimeManager {
                        state: Arc::clone(state),
                    }
                    .plugin_config_for_snapshot(&domain.plugin)
                    .map_err(|failure| failure.message)
                }))
            }
            VULCAN_PLUGIN_CONFIG_UPDATE_TOOL => {
                // A complete replacement document is required by the existing configuration contract.
                // 现有配置契约要求提供完整替换文档。
                let value = arguments.get("value").cloned().ok_or_else(|| {
                    error(
                        EmbeddedErrorCode::InvalidArgument,
                        "Plugin configuration update requires field `value`.",
                    )
                })?;
                Ok(effect(state, domain, name, invocation, deadline, || {
                    VulcanPluginRuntimeManager {
                        state: Arc::clone(state),
                    }
                    .update_plugin_config_for_snapshot(&domain.plugin, value)
                    .map_err(|failure| failure.message)
                }))
            }
            VULCAN_PLUGIN_UI_INVALIDATE_TOOL => {
                // Topic and revision are business data; the plugin channel is host-owned.
                // 主题及修订属于业务数据；插件通道由宿主持有。
                let topic = arguments.get("topic").and_then(Value::as_str)
                    .filter(|value| !value.is_empty() && value.len() <= UI_INVALIDATION_TOPIC_MAX_BYTES)
                    .ok_or_else(|| error(EmbeddedErrorCode::InvalidArgument, format!("Plugin UI invalidation requires a topic of 1 to {UI_INVALIDATION_TOPIC_MAX_BYTES} bytes.")))?;
                // Revisions retain the exact unsigned integer semantics of the legacy protocol.
                // 修订保留旧协议精确无符号整数语义。
                let revision = arguments
                    .get("revision")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        error(
                            EmbeddedErrorCode::InvalidArgument,
                            "Plugin UI invalidation requires u64 revision.",
                        )
                    })?;
                // No host service lock crosses the external publisher boundary.
                // 宿主服务锁不跨越外部发布边界。
                let publisher = state
                    .host_services
                    .read()
                    .map_err(|_| internal("Plugin host services lock is poisoned."))?
                    .ui_events
                    .clone()
                    .ok_or_else(|| {
                        error(
                            EmbeddedErrorCode::Closed,
                            "Plugin UI publisher is unavailable.",
                        )
                    })?;
                Ok(effect(state, domain, name, invocation, deadline, || {
                    publisher.publish(VulcanPluginUiInvalidation {
                        plugin_id: domain.plugin.id.clone(),
                        topic: topic.to_string(),
                        revision,
                    })?;
                    Ok(json!({"published":true}))
                }))
            }
            _ => {
                if let Some(operation) = plugin_storage_operation(name) {
                    // Service resolution is short; actual storage runs after the lock is released.
                    // 服务解析短暂；实际存储在锁释放后运行。
                    let storage = state
                        .host_services
                        .read()
                        .map_err(|_| internal("Plugin host services lock is poisoned."))?
                        .storage
                        .clone()
                        .ok_or_else(|| {
                            error(
                                EmbeddedErrorCode::Closed,
                                "Plugin storage service is unavailable.",
                            )
                        })?;
                    return Ok(effect(state, domain, name, invocation, deadline, || {
                        storage.execute(VulcanPluginStorageRequest {
                            deadline: Some(deadline),
                            plugin_id: domain.plugin.id.clone(),
                            operation,
                            payload: Value::Object(arguments.clone()),
                        })
                    }));
                }
                if let Some(operation) = plugin_vldb_operation(name) {
                    // The logical binding service receives trusted identity separately from its payload.
                    // 逻辑绑定服务在载荷之外单独接收可信身份。
                    let vldb = state
                        .host_services
                        .read()
                        .map_err(|_| internal("Plugin host services lock is poisoned."))?
                        .vldb
                        .clone()
                        .ok_or_else(|| {
                            error(
                                EmbeddedErrorCode::Closed,
                                "Plugin VLDB service is unavailable.",
                            )
                        })?;
                    return Ok(effect(state, domain, name, invocation, deadline, || {
                        vldb.execute(VulcanPluginVldbRequest {
                            deadline: Some(deadline),
                            plugin_id: domain.plugin.id.clone(),
                            operation,
                            payload: Value::Object(arguments.clone()),
                        })
                    }));
                }
                Err(internal(
                    "Native capability has no registered implementation.",
                ))
            }
        }
    })();
    match prepared {
        Ok(outcome) => outcome,
        Err(failure) => CapabilityOutcome {
            result: Err(failure),
            effects: EffectState::NotStarted,
        },
    }
}

/// Recheck live `invocation` authority and `deadline` immediately before running `call`.
/// 在运行 `call` 前立即重验实时 `invocation` 权威及 `deadline`。
/// `state` must still authorize the retained `domain` after any service lookup wait.
/// 任何服务查询等待结束后，`state` 仍必须授权保留的 `domain`。
/// name identifies the exact native capability to recheck before entering the service.
/// name 标识进入服务前需要重验的精确原生能力。
/// Opaque service results carry unknown effects even when their business result succeeds.
/// 即使业务结果成功，不透明服务结果仍携带未知副作用状态。
fn effect(
    state: &VulcanPluginManagerState,
    domain: &ExecutionAuthority,
    name: &str,
    invocation: &CapabilityInvocation,
    deadline: Instant,
    call: impl FnOnce() -> Result<Value, String>,
) -> CapabilityOutcome {
    if let Err(failure) = invocation.authorize() {
        return CapabilityOutcome {
            result: Err(failure),
            effects: EffectState::NotStarted,
        };
    }
    if let Err(failure) = validate_invocation_current(state, domain, name, invocation) {
        return CapabilityOutcome {
            result: Err(failure),
            effects: EffectState::NotStarted,
        };
    }
    if Instant::now() >= deadline {
        return CapabilityOutcome {
            result: Err(error(
                EmbeddedErrorCode::DeadlineExceeded,
                "Plugin host deadline expired before service execution.",
            )),
            effects: EffectState::NotStarted,
        };
    }
    CapabilityOutcome {
        result: call().map_err(|message| error(EmbeddedErrorCode::ExecutionFailed, message)),
        effects: EffectState::Unknown,
    }
}
