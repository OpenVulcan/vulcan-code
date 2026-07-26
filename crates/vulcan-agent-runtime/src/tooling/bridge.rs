//! host-side bridge that adapts native, LuaSkills, and MCP runtime sources into the standalone VulcanToolsManager crate.
//! 将原生工具、LuaSkills 与 MCP 运行时来源适配到独立 VulcanToolsManager crate 的宿主侧桥接层。

use crate::agent_runtime::ai_tool_router::{AiToolRouteKind, resolve_ai_tool_plan};
use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::agent_runtime::question_runtime::{QuestionRuntime, build_question_tool_candidate};
use crate::agent_runtime::task_runtime::{TaskRuntime, build_subagent_tool_candidates};
use crate::image_understanding_compat::{
    configured_understand_image_parameters_schema, execute_configured_understand_image,
};
use crate::luaskills_runtime::{
    LUASKILL_SID_PARAMETER_NAME, LUASKILLS_HOST_MANAGED_SID_PREFIX, LuaSkillsRuntimeHandle,
    PWD_PARAMETER_NAME, build_luaskills_tool_invocation_context, build_runlua_tool_candidates,
    call_luaskills_tool_entry, luaskills_entry_args_schema,
    render_luaskills_invocation_result_text,
};
use crate::mcp::{
    HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY, MANAGED_CODEBASE_MEMORY_SERVER_ID,
    ManagedCodebaseMemoryExecutionContext, McpRuntimeToolBinding,
    inject_managed_codebase_memory_context, managed_codebase_memory_public_tool_id,
    model_visible_managed_codebase_memory_schema,
};
use crate::media_generation_compat::{
    configured_image_generation_parameters_schema, configured_music_cover_parameters_schema,
    configured_music_generation_parameters_schema, configured_speech_synthesis_parameters_schema,
    configured_video_generation_parameters_schema, execute_configured_image_generation,
    execute_configured_music_cover, execute_configured_music_generation,
    execute_configured_speech_synthesis, execute_configured_video_generation,
};
use crate::provider_config::{
    AiToolKind, IMAGE_GENERATION_HOST_BINDING_TOOL_ID, MUSIC_COVER_HOST_BINDING_TOOL_ID,
    MUSIC_GENERATION_HOST_BINDING_TOOL_ID, PersistedAiToolConfiguration,
    SPEECH_SYNTHESIS_HOST_BINDING_TOOL_ID, UNDERSTAND_IMAGE_HOST_BINDING_TOOL_ID,
    VIDEO_GENERATION_HOST_BINDING_TOOL_ID, WEB_SEARCH_HOST_BINDING_TOOL_ID,
};
use crate::router_tool_bridge::append_router_contract_tool_candidates;
use crate::tooling::{
    NativeToolCatalog, NativeToolDescriptor, NativeToolExecutionRequest,
    allocate_unique_public_tool_name, build_direct_public_tool_name,
    build_prefixed_public_tool_name,
};
use crate::web_search_compat::{
    configured_web_search_parameters_schema, execute_configured_web_search,
};
use crate::workspace_path::native_workspace_path_text_from_path;
use luaskills::RuntimeEntryDescriptor;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_luaskills_prompt::{
    LUASKILL_HELP_TOOL_NAME, LuaSkillPromptEntry, render_luaskill_help_output,
};
use vulcan_plugin_runtime::{
    VulcanPluginInvocationRequest, VulcanPluginRuntimeManager, VulcanPluginToolDescriptor,
};
use vulcan_tools_luaskills_call::{
    CALL_LUASKILL_TOOL_NAME, LuaSkillCallEntryView, call_luaskill_args_schema,
    parse_call_luaskill_request, resolve_call_luaskill_entry_by_name,
    restore_call_luaskill_args_for_entry,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};
use vulcan_workspace_store::data_plane::ControllerRuntimeHandle;

mod ai_host_binding;
mod descriptors;
mod luaskills_dispatch;

#[cfg(test)]
use ai_host_binding::configured_understand_image_candidate;
use ai_host_binding::{
    append_ai_tool_host_binding_aliases, append_configured_ai_tool_backing_candidates,
};
use descriptors::{
    entry_declares_luaskill_sid, entry_declares_pwd, luaskills_descriptor_to_vulcan,
    managed_codebase_memory_tool_descriptor, mcp_tool_binding_to_vulcan,
    native_descriptor_to_vulcan, native_plan_policy, native_tool_id_is_knowledge_tool,
    native_tool_id_is_vmm_memory_tool,
};
#[cfg(test)]
use luaskills_dispatch::luaskills_entry_to_call_view;
use luaskills_dispatch::{
    build_luaskills_dispatch_candidate, build_luaskills_help_candidate,
    inject_host_managed_luaskill_sid_if_needed, inject_host_managed_pwd_if_needed,
};

#[cfg(test)]
mod tests;

/// Shared description suffix that explains how generated-media tools expose export controls and follow-up file paths to the model.
/// 生成媒体工具共享的描述后缀，用于向模型说明导出控制项以及可复用的后续文件路径。
const GENERATED_MEDIA_OUTPUT_DESCRIPTION_SUFFIX: &str = " Optional `output_dir` exports a caller-chosen copy. Optional `output_file_stem` suggests the file name without extension. Generated results return `outputs[*].local_path` for follow-up tool calls.";

/// One semantic AI-tool alias bound to one explicitly selected host tool.
/// 绑定到一个显式选择宿主工具的语义 AI 工具别名。
#[derive(Debug, Clone)]
pub struct AiToolHostBindingAlias {
    /// Stable semantic AI-specialized tool kind exposed to the model.
    /// 暴露给模型的稳定 AI 专用语义工具类型。
    pub kind: AiToolKind,
    /// Concrete host tool identifier that should back the semantic alias.
    /// 承载该语义别名的具体宿主工具标识。
    pub host_tool_id: String,
}

/// Resolves semantic host-tool bindings from one provider/model blueprint.
/// 从单个 Provider/Model 蓝图解析语义宿主工具绑定。
pub fn resolve_ai_tool_host_binding_aliases(
    blueprint: &ProviderRuntimeBlueprint,
) -> Vec<AiToolHostBindingAlias> {
    resolve_ai_tool_plan(blueprint)
        .decisions
        .into_iter()
        .filter(|decision| decision.route == AiToolRouteKind::HostBinding)
        .filter_map(|decision| {
            decision
                .host_tool_id
                .map(|host_tool_id| AiToolHostBindingAlias {
                    kind: decision.kind,
                    host_tool_id,
                })
        })
        .collect()
}

/// Builds one standalone host-owned tools manager from the current host runtime surfaces.
/// 根据当前宿主运行时工具面构建独立宿主自有工具管理器。
pub fn build_vulcan_tools_manager(
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    native_catalog: NativeToolCatalog,
    vulcan_plugin_runtime: Option<VulcanPluginRuntimeManager>,
    candidate_vulcan_plugin_tools: Vec<VulcanPluginToolDescriptor>,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    workspace_root: Option<PathBuf>,
    controller_runtime: Option<ControllerRuntimeHandle>,
    knowledge_access: Option<crate::agent_runtime::ExecutionKnowledgeAccessContext>,
    conversation_attachments: Vec<ConversationAttachment>,
    candidate_native_tool_ids: Vec<String>,
    candidate_luaskills_tools: Vec<RuntimeEntryDescriptor>,
    candidate_luaskills_dispatch_tools: Vec<RuntimeEntryDescriptor>,
    candidate_mcp_tools: Vec<McpRuntimeToolBinding>,
    managed_codebase_memory: Option<ManagedCodebaseMemoryExecutionContext>,
    ai_tool_configuration: PersistedAiToolConfiguration,
    ai_tool_host_binding_aliases: Vec<AiToolHostBindingAlias>,
    enabled_prompt_skill_names: Vec<String>,
    prepared_prompt_skills: Vec<vulcan_prompt_skills::PromptSkill>,
    prompt_skill_user_root: Option<PathBuf>,
    implicit_luaskill_prompt_entries: Vec<LuaSkillPromptEntry>,
    project_vmm: vulcan_vmm::VmmConfigLayer,
    session_vmm: vulcan_vmm::VmmConfigLayer,
) -> VulcanToolsManager {
    let mut candidates = Vec::new();
    let mut occupied_public_tool_ids = BTreeSet::new();
    let vmm_effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        native_catalog.vmm_global_config(),
        Some(&project_vmm),
        Some(&session_vmm),
    );
    let vmm_ai_tools_enabled = vmm_effective.ai_tools_enabled
        && vmm_effective.diagnose_memory_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;

    for subagent_candidate in build_subagent_tool_candidates(task_runtime) {
        occupied_public_tool_ids.insert(subagent_candidate.descriptor.id.clone());
        candidates.push(subagent_candidate);
    }
    let ask_candidate = build_question_tool_candidate(question_runtime, "ask");
    occupied_public_tool_ids.insert(ask_candidate.descriptor.id.clone());
    candidates.push(ask_candidate);

    for tool_id in candidate_native_tool_ids {
        if native_tool_id_is_vmm_memory_tool(&tool_id) && !vmm_ai_tools_enabled {
            continue;
        }
        if native_tool_id_is_knowledge_tool(&tool_id)
            && knowledge_access
                .as_ref()
                .is_none_or(|context| context.library_ids.is_empty())
        {
            continue;
        }
        let Some(definition) = native_catalog.definition(&tool_id) else {
            continue;
        };
        let descriptor = native_descriptor_to_vulcan(&definition.descriptor);
        occupied_public_tool_ids.insert(descriptor.id.clone());
        let catalog = native_catalog.clone();
        let tool_id = definition.descriptor.id.clone();
        let workspace_root_for_context = workspace_root.clone();
        let controller_runtime_for_context = controller_runtime.clone();
        let knowledge_access_for_context = knowledge_access.clone();
        let enabled_prompt_skill_names_for_context = enabled_prompt_skill_names.clone();
        let prepared_prompt_skills_for_context = prepared_prompt_skills.clone();
        let prompt_skill_user_root_for_context = prompt_skill_user_root.clone();
        let project_vmm_for_context = project_vmm.clone();
        let session_vmm_for_context = session_vmm.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema: definition.args_schema.clone(),
            ai_visible: definition.descriptor.ai_visible,
            plan_policy: native_plan_policy(&definition.descriptor),
            request_streaming: definition.request_streaming.clone(),
            invoke: std::sync::Arc::new(move |args, session_id| {
                let catalog = catalog.clone();
                let tool_id = tool_id.clone();
                let workspace_root = workspace_root_for_context.clone();
                let controller_runtime = controller_runtime_for_context.clone();
                let knowledge_access = knowledge_access_for_context.clone();
                let prompt_skill_user_root = prompt_skill_user_root_for_context.clone();
                let enabled_prompt_skill_names = enabled_prompt_skill_names_for_context.clone();
                let prepared_prompt_skills = prepared_prompt_skills_for_context.clone();
                let project_vmm = project_vmm_for_context.clone();
                let session_vmm = session_vmm_for_context.clone();
                Box::pin(async move {
                    catalog
                        .invoke(
                            &tool_id,
                            NativeToolExecutionRequest {
                                arguments: args,
                                session_id: session_id
                                    .as_deref()
                                    .and_then(|value| Uuid::parse_str(value).ok()),
                                spill_root: None,
                                workspace_root,
                                controller_runtime,
                                knowledge_access,
                                enabled_prompt_skill_names,
                                prepared_prompt_skills,
                                prompt_skill_user_root,
                                project_vmm,
                                session_vmm,
                            },
                        )
                        .await
                        .map_err(|error| error.to_string())
                })
            }),
        });
    }

    if let Some(runtime) = luaskills_runtime.clone() {
        // runlua_candidates are engine-level LuaSkills tools rather than manifest entries.
        // runlua_candidates 是引擎级 LuaSkills 工具，而不是清单入口。
        let runlua_candidates = build_runlua_tool_candidates(runtime, workspace_root.clone());
        for candidate in runlua_candidates {
            // runlua_tool_id reserves the exact public id before manifest-backed entries are projected.
            // runlua_tool_id 在投影清单入口前保留精确公开标识。
            let runlua_tool_id = candidate.descriptor.id.clone();
            if occupied_public_tool_ids.insert(runlua_tool_id.clone()) {
                candidates.push(candidate);
            }
        }
    }

    let mut plugin_groups = BTreeMap::<(String, String), Vec<VulcanPluginToolDescriptor>>::new();
    for tool in candidate_vulcan_plugin_tools {
        plugin_groups
            .entry((tool.plugin_id.clone(), tool.group.clone()))
            .or_default()
            .push(tool);
    }
    for ((_plugin_id, _group_id), mut tools) in plugin_groups {
        tools.sort_by(|left, right| left.id.cmp(&right.id));
        if tools
            .iter()
            .any(|tool| occupied_public_tool_ids.contains(&tool.id))
        {
            continue;
        }
        for tool in tools {
            let descriptor = VulcanToolDescriptor {
                id: tool.id.clone(),
                name: tool.id.clone(),
                description: tool.description.clone(),
                help_text: None,
                source: VulcanToolSourceKind::VulcanPlugin,
            };
            occupied_public_tool_ids.insert(tool.id.clone());
            let runtime = vulcan_plugin_runtime.clone();
            let tool_id = tool.id.clone();
            let workspace_root = workspace_root.clone();
            candidates.push(VulcanToolCandidate {
                descriptor,
                parameters_schema: tool.parameters_schema,
                ai_visible: true,
                plan_policy: if tool.available_in_plan {
                    VulcanPlanToolPolicy::Visible
                } else {
                    VulcanPlanToolPolicy::Hidden
                },
                request_streaming: None,
                invoke: Arc::new(move |arguments, owner_session_id| {
                    let runtime = runtime.clone();
                    let tool_id = tool_id.clone();
                    let workspace_root = workspace_root.clone();
                    Box::pin(async move {
                        let runtime = runtime.ok_or_else(|| {
                            format!("Vulcan Plugin runtime is unavailable for tool `{tool_id}`.")
                        })?;
                        let owner_session_id = owner_session_id.ok_or_else(|| {
                            format!("Vulcan Plugin tool `{tool_id}` requires a conversation owner.")
                        })?;
                        let workspace_root = workspace_root.ok_or_else(|| {
                            format!("Vulcan Plugin tool `{tool_id}` requires a workspace root.")
                        })?;
                        let output = runtime
                            .invoke(VulcanPluginInvocationRequest {
                                tool_id,
                                arguments,
                                owner_session_id,
                                workspace_root,
                            })
                            .await
                            .map_err(|error| error.to_string())?;
                        Ok(VulcanToolInvocationResult::from_model_output(output))
                    })
                }),
            });
        }
    }

    if !candidate_luaskills_dispatch_tools.is_empty() {
        let luaskill_help_candidate =
            build_luaskills_help_candidate(implicit_luaskill_prompt_entries);
        occupied_public_tool_ids.insert(luaskill_help_candidate.descriptor.id.clone());
        candidates.push(luaskill_help_candidate);
        let dispatcher_candidate = build_luaskills_dispatch_candidate(
            luaskills_runtime.clone(),
            workspace_root.clone(),
            candidate_luaskills_dispatch_tools,
        );
        occupied_public_tool_ids.insert(dispatcher_candidate.descriptor.id.clone());
        candidates.push(dispatcher_candidate);
    }

    let mut candidate_luaskills_tools = candidate_luaskills_tools;
    candidate_luaskills_tools.sort_by(|left, right| left.canonical_name.cmp(&right.canonical_name));

    for entry in candidate_luaskills_tools {
        let descriptor = luaskills_descriptor_to_vulcan(&entry, &mut occupied_public_tool_ids);
        let host_managed_pwd_available = workspace_root
            .as_deref()
            .is_some_and(|path| !path.as_os_str().is_empty());
        let parameters_schema =
            luaskills_entry_args_schema(&entry, true, host_managed_pwd_available);
        let canonical_name = entry.canonical_name.clone();
        let skill_id = entry.skill_id.clone();
        let entry_declares_luaskill_sid = entry_declares_luaskill_sid(&entry);
        let entry_declares_pwd = entry_declares_pwd(&entry);
        let runtime = luaskills_runtime.clone();
        let workspace_root = workspace_root.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(canonical_name.clone()),
            request_streaming: None,
            invoke: std::sync::Arc::new(move |args, session_id| {
                let runtime = runtime.clone();
                let canonical_name = canonical_name.clone();
                let skill_id = skill_id.clone();
                let workspace_root = workspace_root.clone();
                Box::pin(async move {
                    let Some(handle) = runtime else {
                        return Err(format!(
                            "The LuaSkills runtime is currently unavailable, so tool `{}` cannot be invoked.",
                            canonical_name
                        ));
                    };
                    let args = inject_host_managed_luaskill_sid_if_needed(
                        args,
                        entry_declares_luaskill_sid,
                        session_id.as_deref(),
                        &canonical_name,
                    )?;
                    let args = inject_host_managed_pwd_if_needed(
                        args,
                        entry_declares_pwd,
                        workspace_root.as_deref(),
                        &canonical_name,
                    )?;
                    let parsed_session_id = session_id
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok());
                    let invocation_context = build_luaskills_tool_invocation_context(
                        parsed_session_id,
                        "host-tool",
                        &canonical_name,
                    );
                    let call_handle = handle.clone();
                    let result = call_luaskills_tool_entry(
                        call_handle,
                        canonical_name.clone(),
                        args,
                        Some(invocation_context),
                    )
                    .await
                    .map_err(|error| {
                        format!("LuaSkills tool `{}` failed: {}.", canonical_name, error)
                    })?;
                    let model_output = Value::String(render_luaskills_invocation_result_text(
                        &handle,
                        &canonical_name,
                        Some(&skill_id),
                        &result,
                        workspace_root.as_deref(),
                    )?);
                    let invocation_result = if let Some(host_result) = result.host_result.as_ref() {
                        VulcanToolInvocationResult::from_model_output(model_output)
                            .with_host_result(VulcanToolHostResult {
                                kind: host_result.kind.clone(),
                                payload: host_result.payload.clone(),
                            })
                    } else {
                        VulcanToolInvocationResult::from_model_output(model_output)
                    };
                    Ok(invocation_result)
                })
            }),
        });
    }

    let mut candidate_mcp_tools = candidate_mcp_tools;
    candidate_mcp_tools.sort_by(|left, right| left.id().cmp(right.id()));

    for tool in candidate_mcp_tools {
        let is_managed_codebase_memory =
            tool.partition_key() == HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY;
        let managed_public_tool_id = if is_managed_codebase_memory {
            let Some(context) = managed_codebase_memory.as_ref() else {
                continue;
            };
            if tool.server_id() != Some(MANAGED_CODEBASE_MEMORY_SERVER_ID)
                || context.authorization.resolve(context.session_id).is_err()
            {
                continue;
            }
            let Some(public_tool_id) = tool
                .origin_tool_name()
                .and_then(managed_codebase_memory_public_tool_id)
            else {
                continue;
            };
            Some(public_tool_id)
        } else {
            None
        };
        let descriptor = if let Some(public_tool_id) = managed_public_tool_id {
            managed_codebase_memory_tool_descriptor(public_tool_id, tool.description())
        } else {
            mcp_tool_binding_to_vulcan(&tool)
        };
        occupied_public_tool_ids.insert(descriptor.id.clone());
        let parameters_schema = if is_managed_codebase_memory {
            model_visible_managed_codebase_memory_schema(tool.args_schema().clone())
        } else {
            tool.args_schema().clone()
        };
        let tool_name = tool.name().to_string();
        let managed_context = managed_codebase_memory.clone();
        candidates.push(VulcanToolCandidate {
            descriptor,
            parameters_schema,
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(tool_name.clone()),
            request_streaming: None,
            invoke: std::sync::Arc::new(move |args, session_id| {
                let tool = tool.clone();
                let managed_context = managed_context.clone();
                Box::pin(async move {
                    let args = if is_managed_codebase_memory {
                        let context = managed_context.ok_or_else(|| {
                            String::from(
                                "Codebase Memory authorization is unavailable for this execution.",
                            )
                        })?;
                        let session_id = session_id
                            .as_deref()
                            .ok_or_else(|| {
                                String::from(
                                    "Codebase Memory requires a host execution session identifier.",
                                )
                            })?
                            .parse()
                            .map_err(|error| {
                                format!(
                                    "Codebase Memory received an invalid host execution session identifier: {error}"
                                )
                            })?;
                        // The root-session route is re-resolved on every invocation so disablement, removal, or reassignment invalidates old bindings.
                        // 每次调用都会重新解析根会话路由，使停用、移除或重新归属立即使旧绑定失效。
                        let root_route = context.authorization.resolve(context.session_id)?;
                        if root_route.project_id != context.project_id {
                            return Err(String::from(
                                "Codebase Memory rejected the call because the session project route changed.",
                            ));
                        }
                        let route = context
                            .authorization
                            .resolve_project(&context.project_id)?;
                        inject_managed_codebase_memory_context(args, &route, session_id)?
                    } else {
                        args
                    };
                    tool.invoke(args)
                        .await
                        .map(VulcanToolInvocationResult::from_model_output)
                })
            }),
        });
    }

    let router_tool_configuration = ai_tool_configuration.router.clone();
    let router_workspace_root = workspace_root.clone();
    append_configured_ai_tool_backing_candidates(
        &mut candidates,
        ai_tool_configuration,
        workspace_root,
        conversation_attachments,
    );
    append_router_contract_tool_candidates(
        &mut candidates,
        &router_tool_configuration,
        router_workspace_root,
    );
    append_ai_tool_host_binding_aliases(&mut candidates, ai_tool_host_binding_aliases);

    VulcanToolsManager::new(candidates)
}
