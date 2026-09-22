use super::*;
use vulcan_session_core::v1::child_execution::{
    ChildExecutionRequestOverrides, ChildKnowledgeAccess, ChildMediaUnderstandingSelection,
    ChildOutputStyleSnapshot,
};
use vulcan_session_core::v1::config::{
    KnowledgeRetrievalMode, ProviderMediaUnderstandingModelTargetConfig, ProviderToolTargetConfig,
    VmmSessionPolicyConfig,
};
use vulcan_workbench_model::{
    SessionMediaUnderstandingModelSelection, SessionMediaUnderstandingSelection,
    SessionProviderToolTargetSelection,
};

/// Captures actual non-secret overrides from `request`, preserving every optional selection boundary.
/// 从 `request` 捕获真实非敏感覆盖，保留每种可选选择边界。
/// Returns the durable contract or rejects unsupported session-layer configuration explicitly.
/// 返回可靠契约，或明确拒绝不支持的会话层配置。
pub(super) fn capture_child_request_overrides(
    request: &ExecutionPreparationRequest,
) -> Result<ChildExecutionRequestOverrides, String> {
    // V1 session policy permits exactly these two fields; never serialize transport credentials.
    // V1 会话策略仅允许这两个字段；绝不序列化传输凭据。
    let session_vmm = vulcan_vmm::VmmConfigLayer {
        precheck_enabled: request.session_vmm.precheck_enabled,
        memory_writeback_enabled: request.session_vmm.memory_writeback_enabled,
        ..Default::default()
    };
    if request.session_vmm != session_vmm {
        return Err(String::from(
            "Child profile capture requires the V1 session memory policy without transport or project fields.",
        ));
    }
    // Capture optional maps independently; explicit empty maps must not become inheritance.
    // 独立捕获可选映射；显式空映射不能变成继承。
    let captured = ChildExecutionRequestOverrides {
        provider_tool_selections: request.provider_tool_selections_override.as_ref().map(
            |selections| {
                selections
                    .iter()
                    .map(|(capability, target)| (capability.clone(), capture_target(target)))
                    .collect()
            },
        ),
        media_understanding_selections: request
            .provider_tool_media_understanding_selections_override
            .as_ref()
            .map(|selections| {
                selections
                    .iter()
                    .map(|(capability, selection)| {
                        (capability.clone(), capture_media_selection(selection))
                    })
                    .collect()
            }),
        enabled_capabilities: request.provider_tool_enabled_capabilities_override.clone(),
        extension_target_sets: request
            .provider_extension_target_sets_override
            .as_ref()
            .map(|sets| {
                sets.iter()
                    .map(|(capability, targets)| {
                        (
                            capability.clone(),
                            targets.iter().map(capture_target).collect(),
                        )
                    })
                    .collect()
            }),
        session_vmm: VmmSessionPolicyConfig {
            precheck_enabled: session_vmm.precheck_enabled,
            memory_writeback_enabled: session_vmm.memory_writeback_enabled,
        },
        output_style: request.output_style_snapshot.as_ref().map(|style| {
            ChildOutputStyleSnapshot {
                id: style.id.clone(),
                label: style.label.clone(),
                text: style.text.clone(),
                source_label: style.source_label.clone(),
            }
        }),
        knowledge_access: request
            .knowledge_access
            .as_ref()
            .map(|access| {
                if request.project_id.as_deref() != Some(access.project_id.as_str()) {
                    return Err(String::from(
                        "Child knowledge access belongs to a different project.",
                    ));
                }
                // Reject unknown policies rather than changing retrieval semantics during restoration.
                // 拒绝未知策略，防止恢复时改变检索语义。
                let retrieval_mode = match access.retrieval_mode.as_deref() {
                    None => None,
                    Some("raw_bm25") => Some(KnowledgeRetrievalMode::RawBm25),
                    Some("display_bm25") => Some(KnowledgeRetrievalMode::DisplayBm25),
                    Some("display_hybrid") => Some(KnowledgeRetrievalMode::DisplayHybrid),
                    Some(_) => {
                        return Err(String::from(
                            "Child knowledge access has an unsupported retrieval mode.",
                        ));
                    }
                };
                Ok(ChildKnowledgeAccess {
                    project_id: access.project_id.clone(),
                    library_ids: access.library_ids.iter().cloned().collect(),
                    retrieval_mode,
                })
            })
            .transpose()?,
    };
    captured.validate().map_err(|error| error.to_string())?;
    Ok(captured)
}

/// Replaces request session overrides with the already validated `captured` child settings.
/// 用已校验的 `captured` 子设置替换请求会话覆盖。
/// Mutates only request-local settings; current host ports and project configuration remain authoritative.
/// 仅修改请求局部设置；当前宿主端口及项目配置保持权威。
pub(super) fn restore_child_request_overrides(
    request: &mut ExecutionPreparationRequest,
    captured: &ChildExecutionRequestOverrides,
) {
    request.output_style_snapshot = captured.output_style.as_ref().map(|style| {
        vulcan_prompt_composer::PromptOutputStyleSnapshot::new(
            style.id.clone(),
            style.label.clone(),
            style.text.clone(),
            style.source_label.clone(),
        )
    });
    // The host must reauthorize the captured ceiling; never retain the preparation template's access.
    // 宿主必须重新授权捕获的上限；绝不保留准备模板的访问权限。
    request.knowledge_access = None;
    request.provider_tool_selections_override =
        captured
            .provider_tool_selections
            .as_ref()
            .map(|selections| {
                selections
                    .iter()
                    .map(|(capability, target)| (capability.clone(), restore_target(target)))
                    .collect()
            });
    request.provider_tool_media_understanding_selections_override = captured
        .media_understanding_selections
        .as_ref()
        .map(|selections| {
            selections
                .iter()
                .map(|(capability, selection)| {
                    (capability.clone(), restore_media_selection(selection))
                })
                .collect()
        });
    request.provider_tool_enabled_capabilities_override = captured.enabled_capabilities.clone();
    request.provider_extension_target_sets_override =
        captured.extension_target_sets.as_ref().map(|sets| {
            sets.iter()
                .map(|(capability, targets)| {
                    (
                        capability.clone(),
                        targets.iter().map(restore_target).collect(),
                    )
                })
                .collect()
        });
    request.session_vmm = vulcan_vmm::VmmConfigLayer {
        precheck_enabled: captured.session_vmm.precheck_enabled,
        memory_writeback_enabled: captured.session_vmm.memory_writeback_enabled,
        ..Default::default()
    };
}

/// Converts `target` to a durable connection/target pair without secrets.
/// 将 `target` 转换为不包含秘密的可靠连接与目标对。
fn capture_target(target: &SessionProviderToolTargetSelection) -> ProviderToolTargetConfig {
    ProviderToolTargetConfig {
        connection_id: target.connection_id.clone(),
        target_id: target.target_id.clone(),
    }
}

/// Restores the exact `target` pair for current host authorization.
/// 恢复精确 `target` 对，交由当前宿主授权。
fn restore_target(target: &ProviderToolTargetConfig) -> SessionProviderToolTargetSelection {
    SessionProviderToolTargetSelection {
        connection_id: target.connection_id.clone(),
        target_id: target.target_id.clone(),
    }
}

/// Captures `selection` with its exact route family and optional reasoning level.
/// 按精确路由类型及可选推理档位捕获 `selection`。
fn capture_media_selection(
    selection: &SessionMediaUnderstandingSelection,
) -> ChildMediaUnderstandingSelection {
    match selection {
        SessionMediaUnderstandingSelection::ProviderTool { selection } => {
            ChildMediaUnderstandingSelection::ProviderTool {
                selection: capture_target(selection),
            }
        }
        SessionMediaUnderstandingSelection::ModelReplacement {
            selection,
            reasoning_effort,
        } => ChildMediaUnderstandingSelection::ModelReplacement {
            selection: ProviderMediaUnderstandingModelTargetConfig {
                provider_id: selection.provider_id.clone(),
                model_id: selection.model_id.clone(),
                connection_id: selection.connection_id.clone(),
            },
            reasoning_effort: reasoning_effort.map(Into::into),
        },
    }
}

/// Restores `selection` without substituting provider tools for a model route or changing reasoning.
/// 恢复 `selection`，不以供应商工具替代模型路由，也不改变推理设置。
fn restore_media_selection(
    selection: &ChildMediaUnderstandingSelection,
) -> SessionMediaUnderstandingSelection {
    match selection {
        ChildMediaUnderstandingSelection::ProviderTool { selection } => {
            SessionMediaUnderstandingSelection::ProviderTool {
                selection: restore_target(selection),
            }
        }
        ChildMediaUnderstandingSelection::ModelReplacement {
            selection,
            reasoning_effort,
        } => SessionMediaUnderstandingSelection::ModelReplacement {
            selection: SessionMediaUnderstandingModelSelection {
                provider_id: selection.provider_id.clone(),
                model_id: selection.model_id.clone(),
                connection_id: selection.connection_id.clone(),
            },
            reasoning_effort: reasoning_effort.map(Into::into),
        },
    }
}
