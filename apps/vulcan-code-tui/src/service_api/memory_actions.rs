// Mirror of `service/memory-actions-service.ts`: native VMM actions through `runVmmMemoryAction`.
// `service/memory-actions-service.ts` 的镜像：经 `runVmmMemoryAction` 调用原生 VMM 动作。
//
// The memory area runs every action in global scope, exactly as the WebUI memory controller does
// when no project scope is active. Results arrive as `result.data.{kind, value}`. Destructive
// actions are previewed first; the confirmation reuses the preview's `commandId`, as
// `nativeProjectConfirmationRequest` does.
// 记忆区域在全局作用域运行所有动作，与 WebUI 记忆控制器未启用项目作用域时一致。结果以
// `result.data.{kind, value}` 返回。破坏性动作先预检；确认请求复用预检的 `commandId`，与
// `nativeProjectConfirmationRequest` 一致。

use serde_json::{Value, json};

use super::{CallKind, Preview, ServiceCall, Slot, fresh_command_id};

const METHOD: &str = "service_run_vmm_memory_action";
const FUNCTION: &str = "run_vmm_memory_action";
// Project writes can change bindings, record owners and derived index observations.
// 项目写入可能改变绑定、记录归属及派生索引观察结果。
const PROJECTS: &[Slot] = &[
    Slot::VmmProjects,
    Slot::VmmRecords,
    Slot::VmmProfile,
    Slot::VmmVector,
    Slot::VmmLexical,
    Slot::VmmRetention,
    Slot::VmmRecycleBin,
];
// Source removal changes both active content and retained-content/index counts.
// 来源移除同时改变活动内容与保留内容、索引计数。
const RECORDS: &[Slot] = &[
    Slot::VmmRecords,
    Slot::VmmRecycleBin,
    Slot::VmmVector,
    Slot::VmmLexical,
    Slot::VmmRetention,
];
const MAINTENANCE: &[Slot] = &[
    Slot::VmmVector,
    Slot::VmmLexical,
    Slot::VmmRetention,
    Slot::VmmRecycleBin,
    Slot::VmmRecords,
];

/// `runVmmMemoryAction({action, commandId, scope: "global", projectId: null, payload})`
/// Builds the native VMM action envelope with an explicit command identity and payload.
/// 使用明确命令身份及载荷构建原生 VMM 动作信封。
/// `action` is the declared native VMM action name.
/// `action` 为已声明的原生 VMM 动作名。
/// `command_id` is the stable native command identity.
/// `command_id` 为稳定的原生命令身份。
/// `payload` is the declared request or event payload.
/// `payload` 为已声明的请求或事件载荷。
/// Returns `Value`.
/// 返回 `Value`。
fn request(action: &str, command_id: String, payload: Value) -> Value {
    json!({
        "request": {
            "action": action,
            "commandId": command_id,
            "scope": "global",
            "projectId": null,
            "payload": payload,
        }
    })
}

/// Builds a read-only request whose result replaces exactly one declared slot.
/// 构建只读请求，其结果仅替换一个声明的槽位。
/// `action` is the declared native VMM action name.
/// `action` 为已声明的原生 VMM 动作名。
/// `payload` is the declared request or event payload.
/// `payload` 为已声明的请求或事件载荷。
/// `slot` is the declared result slot.
/// `slot` 为已声明的结果槽位。
/// Returns `ServiceCall`.
/// 返回 `ServiceCall`。
fn load(action: &str, payload: Value, slot: Slot) -> ServiceCall {
    ServiceCall::load(
        FUNCTION,
        METHOD,
        request(action, fresh_command_id(), payload),
        slot,
    )
}

/// Builds a write request with its explicit authoritative reload set.
/// 构建写入请求及其明确的权威重读集合。
/// `action` is the declared native VMM action name.
/// `action` 为已声明的原生 VMM 动作名。
/// `payload` is the declared request or event payload.
/// `payload` 为已声明的请求或事件载荷。
/// `reload` is the authoritative slots invalidated by success.
/// `reload` 为成功后失效的权威槽位。
/// Returns `ServiceCall`.
/// 返回 `ServiceCall`。
fn mutate(action: &str, payload: Value, reload: &'static [Slot]) -> ServiceCall {
    ServiceCall::mutate(
        FUNCTION,
        METHOD,
        request(action, fresh_command_id(), payload),
        reload,
    )
}

/// Builds the read-only first step with the declared confirmation selector.
/// 构建具有已声明确认选择器的只读第一步。
/// `action` is the declared native VMM action name.
/// `action` 为已声明的原生 VMM 动作名。
/// `payload` is the declared request or event payload.
/// `payload` 为已声明的请求或事件载荷。
/// `preview` is the declared review/confirmation family.
/// `preview` 为已声明的审查或确认类别。
/// Returns `ServiceCall`.
/// 返回 `ServiceCall`。
fn preview(action: &str, payload: Value, preview: Preview) -> ServiceCall {
    ServiceCall {
        function: FUNCTION,
        method: METHOD,
        params: request(action, fresh_command_id(), payload),
        kind: CallKind::Preview(preview),
    }
}

/// The VMM action a call runs, e.g. `native_project_catalog`; empty for other calls.
/// 调用运行的 VMM 动作，例如 `native_project_catalog`；其他调用为空。
/// `call` is the service call whose native action is inspected.
/// `call` 为需要检查原生动作的服务调用。
/// Returns `&str`.
/// 返回 `&str`。
pub fn action_of(call: &ServiceCall) -> &str {
    match call.method {
        METHOD => call.param("/request/action").as_str().unwrap_or_default(),
        _ => "",
    }
}

/// The `value` of a VMM action result.
/// VMM 动作结果中的 `value`。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// Returns `&Value`.
/// 返回 `&Value`。
pub fn value_of(result: &Value) -> &Value {
    result.pointer("/data/value").unwrap_or(&Value::Null)
}

// ---- Loaders ----

/// Builds the request for `native_project_catalog`.
/// 构建 `native_project_catalog` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_project_catalog() -> ServiceCall {
    load("native_project_catalog", json!({}), Slot::VmmProjects)
}

/// Memories, sessions and turns of one VMM project.
/// 一个 VMM 项目的记忆、会话与轮次。
/// Builds the request for `native_memory_records`.
/// 构建 `native_memory_records` 的请求。
/// `vmm_project_id` is the stable native memory-project ID.
/// `vmm_project_id` 为稳定的原生记忆项目标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_memory_records(vmm_project_id: i64) -> ServiceCall {
    load(
        "native_memory_records",
        json!({ "vmmProjectId": vmm_project_id }),
        Slot::VmmRecords,
    )
}

/// `target` is `user` or `project`.
/// `target` 为 `user` 或 `project`。
/// Builds the request for `native_profile_records`.
/// 构建 `native_profile_records` 的请求。
/// `vmm_project_id` is the stable native memory-project ID.
/// `vmm_project_id` 为稳定的原生记忆项目标识。
/// `target` is the declared user/project profile target or managed-runtime selector.
/// `target` 为已声明的用户或项目画像目标，或受管理运行时选择器。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_profile_records(vmm_project_id: i64, target: &str) -> ServiceCall {
    load(
        "native_profile_records",
        json!({ "vmmProjectId": vmm_project_id, "target": target }),
        Slot::VmmProfile,
    )
}

/// Builds the request for `native_resource_catalog`.
/// 构建 `native_resource_catalog` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_resource_catalog() -> ServiceCall {
    load("native_resource_catalog", json!({}), Slot::VmmResources)
}

/// Builds the request for `native_runtime_policy`.
/// 构建 `native_runtime_policy` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_runtime_policy() -> ServiceCall {
    load("native_runtime_policy", json!({}), Slot::VmmRuntimePolicy)
}

/// Builds the request for `native_vector_status`.
/// 构建 `native_vector_status` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_vector_status() -> ServiceCall {
    load("native_vector_status", json!({}), Slot::VmmVector)
}

/// Builds the request for `native_lexical_status`.
/// 构建 `native_lexical_status` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_lexical_status() -> ServiceCall {
    load("native_lexical_status", json!({}), Slot::VmmLexical)
}

/// Builds the request for `native_retention_status`.
/// 构建 `native_retention_status` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_retention_status() -> ServiceCall {
    load("native_retention_status", json!({}), Slot::VmmRetention)
}

/// Builds the request for `native_recycle_bin`.
/// 构建 `native_recycle_bin` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_recycle_bin() -> ServiceCall {
    load("native_recycle_bin", json!({}), Slot::VmmRecycleBin)
}

// ---- Projects ----

/// Builds the request for `native_project_create_and_bind`.
/// 构建 `native_project_create_and_bind` 的请求。
/// `vulcan_project_id` is the host project receiving the new memory-owner binding.
/// `vulcan_project_id` 为接收新记忆归属绑定的宿主项目。
/// `project_name` is the requested native memory-project display name.
/// `project_name` 为请求的原生记忆项目显示名称。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_project_create_and_bind(vulcan_project_id: &str, project_name: &str) -> ServiceCall {
    mutate(
        "native_project_create_and_bind",
        json!({ "vulcanProjectId": vulcan_project_id, "projectName": project_name }),
        PROJECTS,
    )
}

/// Builds the request for `native_project_rename`.
/// 构建 `native_project_rename` 的请求。
/// `vmm_project_id` is the stable native memory-project ID.
/// `vmm_project_id` 为稳定的原生记忆项目标识。
/// `name` is the requested display name or catalog-owned skill name.
/// `name` 为请求的显示名称或目录持有的技能名称。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_project_rename(vmm_project_id: i64, name: &str) -> ServiceCall {
    mutate(
        "native_project_rename",
        json!({ "vmmProjectId": vmm_project_id, "name": name }),
        PROJECTS,
    )
}

/// Builds the request for `native_project_clear_preview`.
/// 构建 `native_project_clear_preview` 的请求。
/// `vmm_project_id` is the stable native memory-project ID.
/// `vmm_project_id` 为稳定的原生记忆项目标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_project_clear_preview(vmm_project_id: i64) -> ServiceCall {
    preview(
        "native_project_clear_preview",
        json!({ "vmmProjectId": vmm_project_id }),
        Preview::VmmProject,
    )
}

/// Builds the request for `native_project_delete_preview`.
/// 构建 `native_project_delete_preview` 的请求。
/// `vmm_project_id` is the stable native memory-project ID.
/// `vmm_project_id` 为稳定的原生记忆项目标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_project_delete_preview(vmm_project_id: i64) -> ServiceCall {
    preview(
        "native_project_delete_preview",
        json!({ "vmmProjectId": vmm_project_id }),
        Preview::VmmProject,
    )
}

/// Builds the request for `native_project_migration_preview`.
/// 构建 `native_project_migration_preview` 的请求。
/// `source_project_id` is the native project whose records will be moved.
/// `source_project_id` 为其记录将被移动的原生项目。
/// `target_project_id` is the distinct native project receiving the records.
/// `target_project_id` 为接收记录的另一原生项目。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_project_migration_preview(
    source_project_id: i64,
    target_project_id: i64,
) -> ServiceCall {
    preview(
        "native_project_migration_preview",
        json!({ "sourceProjectId": source_project_id, "targetProjectId": target_project_id }),
        Preview::VmmProject,
    )
}

/// `nativeProjectConfirmationRequest`: confirms a project clear, delete or migration preview,
/// keeping the preview's `commandId`.
/// 确认项目清空、删除或迁移预检，保留预检的 `commandId`。
/// Builds the request for `native_project_migration_preview`.
/// 构建 `native_project_migration_preview` 的请求。
/// `preview` is the read-only first-step call or its declared preview selector.
/// `preview` 为只读第一步调用或其声明的预检选择器。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn native_project_confirmation(preview: &ServiceCall, result: &Value) -> Option<ServiceCall> {
    // Command id.
    // 稳定原生命令身份。
    let command_id = preview
        .params
        .pointer("/request/commandId")?
        .as_str()?
        .to_string();
    if command_id.is_empty() {
        return None;
    }
    // Value.
    // 当前协议值。
    let value = value_of(result);
    // Storage version.
    // 原生内容版本。
    let storage_version = value.get("storageVersion")?.as_i64()?;
    // Action; Payload.
    // 已声明动作；已声明载荷。
    let (action, payload) = match action_of(preview) {
        "native_project_migration_preview" => {
            // Source.
            // 当前来源。
            let source = value.get("sourceProjectId")?.as_i64()?;
            // Target.
            // 已声明目标。
            let target = value.get("targetProjectId")?.as_i64()?;
            // Fingerprint.
            // 服务签发的内容或预检指纹。
            let fingerprint = value.get("previewFingerprint")?.as_str()?;
            if source == target || fingerprint.is_empty() {
                return None;
            }
            if preview
                .params
                .pointer("/request/payload/sourceProjectId")?
                .as_i64()?
                != source
                || preview
                    .params
                    .pointer("/request/payload/targetProjectId")?
                    .as_i64()?
                    != target
            {
                return None;
            }
            (
                "native_project_migrate",
                json!({
                    "sourceProjectId":source, "targetProjectId":target,
                    "expectedStorageVersion":storage_version,
                    "expectedPreviewFingerprint":fingerprint,
                    "acceptSessionRenames":!value.get("sessionRenames")?.as_array()?.is_empty()
                }),
            )
        }
        "native_project_clear_preview" => {
            // Project.
            // 所选项目数据。
            let project = value.get("projectId")?.as_i64()?;
            if preview
                .params
                .pointer("/request/payload/vmmProjectId")?
                .as_i64()?
                != project
            {
                return None;
            }
            (
                "native_project_clear",
                json!({"vmmProjectId":project,"expectedStorageVersion":storage_version}),
            )
        }
        "native_project_delete_preview" => {
            // Project.
            // 所选项目数据。
            let project = value.get("project")?;
            // Exact entity identity.
            // 准确实体身份。
            let id = project.get("projectId")?.as_i64()?;
            if project.get("protected")?.as_bool()?
                || preview
                    .params
                    .pointer("/request/payload/vmmProjectId")?
                    .as_i64()?
                    != id
            {
                return None;
            }
            // Bound.
            // 是否存在已观察绑定。
            let bound = !project.get("ownerVulcanProjectIds")?.as_array()?.is_empty()
                || !project
                    .get("linkedVulcanProjectIds")?
                    .as_array()?
                    .is_empty();
            (
                "native_project_delete",
                json!({"vmmProjectId":id,
                "expectedStorageVersion":storage_version,"confirmDetachBindings":bound}),
            )
        }
        _ => return None,
    };
    Some(ServiceCall::mutate(
        FUNCTION,
        METHOD,
        request(action, command_id, payload),
        PROJECTS,
    ))
}

// ---- Records ----

/// `kind` is `sessions` or `turns`: native removal works on whole sessions or turns.
/// `kind` 为 `sessions` 或 `turns`：原生删除以整个会话或轮次为单位。
/// Builds the request for `native_removal_preview`.
/// 构建 `native_removal_preview` 的请求。
/// `vmm_project_id` is the stable native memory-project ID.
/// `vmm_project_id` 为稳定的原生记忆项目标识。
/// `kind` is the declared sessions/turns removal family.
/// `kind` 为已声明的会话或轮次移除类别。
/// `ids` is the distinct canonical string identities in that family.
/// `ids` 为该类别中互不重复的规范字符串身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_removal_preview(vmm_project_id: i64, kind: &str, ids: Vec<Value>) -> ServiceCall {
    preview(
        "native_removal_preview",
        json!({ "vmmProjectId": vmm_project_id, "selection": { "kind": kind, "ids": ids } }),
        Preview::VmmRemoval,
    )
}

/// Moves the previewed sessions or turns to the recycle bin.
/// 将预检的会话或轮次移入回收站。
/// Builds the request for `native_removal_recycle`.
/// 构建 `native_removal_recycle` 的请求。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn native_removal_recycle(result: &Value) -> Option<ServiceCall> {
    // Preview id.
    // 服务签发的单次预检身份。
    let preview_id = value_of(result).get("previewId")?.as_str()?;
    if preview_id.is_empty() {
        return None;
    }
    Some(mutate(
        "native_removal_recycle",
        json!({"previewId":preview_id,"confirmed":true}),
        RECORDS,
    ))
}

// ---- Resources ----

/// Builds the request for `native_resource_reset`.
/// 构建 `native_resource_reset` 的请求。
/// `path` is the exact resource catalog path.
/// `path` 为准确的资源目录路径。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_resource_reset(path: &str) -> ServiceCall {
    mutate(
        "native_resource_reset",
        json!({ "path": path }),
        &[Slot::VmmResources, Slot::VmmResourceFile],
    )
}

/// Reads the effective resource at the exact catalog path.
/// 在准确目录路径读取生效资源。
/// Builds the request for `native_resource_read`.
/// 构建 `native_resource_read` 的请求。
/// `path` is the exact resource catalog path.
/// `path` 为准确的资源目录路径。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_resource_read(path: &str) -> ServiceCall {
    load(
        "native_resource_read",
        json!({"path":path}),
        Slot::VmmResourceFile,
    )
}

/// Validates and activates the complete resource composition document.
/// 校验并启用完整资源组合文档。
/// Builds the request for `native_resource_settings_update`.
/// 构建 `native_resource_settings_update` 的请求。
/// `settings` is the settings document accepted by the matching save API.
/// `settings` 为对应保存 API 接收的设置文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_resource_settings_update(settings: Value) -> ServiceCall {
    mutate(
        "native_resource_settings_update",
        settings,
        &[Slot::VmmResources],
    )
}

/// Writes only after the UI has confirmed both the resource purpose and its new text.
/// 仅在界面确认资源用途与新正文后写入。
/// Builds the request for `native_resource_write`.
/// 构建 `native_resource_write` 的请求。
/// `path` is the exact resource catalog path.
/// `path` 为准确的资源目录路径。
/// `content` is the complete resource text, preserving whitespace.
/// `content` 为保留空白的完整资源正文。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_resource_write(path: &str, content: &str) -> ServiceCall {
    mutate(
        "native_resource_write",
        json!({"path":path,"content":content,
        "confirmedPurpose":true,"confirmedModification":true}),
        &[Slot::VmmResources, Slot::VmmResourceFile],
    )
}

/// Saves one whole recall/extraction group; sibling fields are retained by its editor.
/// 保存完整召回与提炼配置组；编辑器保留同级字段。
/// Builds the request for `native_runtime_policy_update`.
/// 构建 `native_runtime_policy_update` 的请求。
/// `policy` is the complete declared native policy group, retaining sibling fields.
/// `policy` 为保留同级字段的完整原生策略组。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_runtime_policy_update(policy: Value) -> ServiceCall {
    mutate(
        "native_runtime_policy_update",
        policy,
        &[Slot::VmmRuntimePolicy],
    )
}

/// Saves the complete ranking group, including the exact string representation of rrfK.
/// 保存完整排序组，包括 rrfK 的准确字符串表示。
/// Builds the request for `native_retrieval_policy_update`.
/// 构建 `native_retrieval_policy_update` 的请求。
/// `policy` is the complete declared native policy group, retaining sibling fields.
/// `policy` 为保留同级字段的完整原生策略组。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_retrieval_policy_update(policy: Value) -> ServiceCall {
    mutate(
        "native_retrieval_policy_update",
        policy,
        &[Slot::VmmRuntimePolicy],
    )
}

/// Saves editable cleanup rules without copying internal worker limits from status.
/// 保存可编辑清理规则，不从状态中复制内部工作器限制。
/// Builds the request for `native_retention_settings_update`.
/// 构建 `native_retention_settings_update` 的请求。
/// `settings` is the settings document accepted by the matching save API.
/// `settings` 为对应保存 API 接收的设置文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_retention_settings_update(settings: Value) -> ServiceCall {
    mutate(
        "native_retention_settings_update",
        settings,
        &[Slot::VmmRuntimePolicy, Slot::VmmRetention],
    )
}

/// Saves the two intake-reduction controls from the native text policy.
/// 保存原生文本策略中的两个入库缩减控制项。
/// Builds the request for `native_text_policy_update`.
/// 构建 `native_text_policy_update` 的请求。
/// `policy` is the complete declared native policy group, retaining sibling fields.
/// `policy` 为保留同级字段的完整原生策略组。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_text_policy_update(policy: Value) -> ServiceCall {
    mutate(
        "native_text_policy_update",
        policy,
        &[Slot::VmmRuntimePolicy],
    )
}

/// Saves both user and project profile lifetime groups.
/// 保存用户与项目两组画像有效期。
/// Builds the request for `native_profile_policy_update`.
/// 构建 `native_profile_policy_update` 的请求。
/// `policy` is the complete declared native policy group, retaining sibling fields.
/// `policy` 为保留同级字段的完整原生策略组。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_profile_policy_update(policy: Value) -> ServiceCall {
    mutate(
        "native_profile_policy_update",
        policy,
        &[Slot::VmmRuntimePolicy],
    )
}

/// Obtains the exact rebuild generation and fingerprint without changing vectors.
/// 获取准确重建代次与指纹，不修改向量。
/// Builds the request for `native_vector_rebuild_preview`.
/// 构建 `native_vector_rebuild_preview` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_vector_rebuild_preview() -> ServiceCall {
    preview(
        "native_vector_rebuild_preview",
        json!({}),
        Preview::VmmVector,
    )
}

/// Executes only the storage version and target set authorized in a rebuild preview.
/// 仅执行重建预检中已授权的存储版本与目标集合。
/// Builds the request for `native_vector_rebuild`.
/// 构建 `native_vector_rebuild` 的请求。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn native_vector_rebuild(result: &Value) -> Option<ServiceCall> {
    // Plan.
    // 已审查操作计划。
    let plan = value_of(result);
    // Fingerprint.
    // 服务签发的内容或预检指纹。
    let fingerprint = plan.get("previewFingerprint")?.as_str()?;
    if fingerprint.is_empty() {
        return None;
    }
    Some(mutate(
        "native_vector_rebuild",
        json!({
        "expectedStorageVersion":plan.get("storageVersion")?.as_i64()?,
        "expectedPreviewFingerprint":fingerprint,
        "confirmed":true}),
        MAINTENANCE,
    ))
}

// ---- Maintenance ----

/// Builds the request for `native_lexical_rebuild`.
/// 构建 `native_lexical_rebuild` 的请求。
/// `expected_storage_version` is the observed native content version.
/// `expected_storage_version` 为已观察的原生内容版本。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_lexical_rebuild(expected_storage_version: i64) -> ServiceCall {
    mutate(
        "native_lexical_rebuild",
        json!({ "expectedStorageVersion": expected_storage_version, "confirmed": true }),
        MAINTENANCE,
    )
}

/// Builds the request for `native_retention_preview`.
/// 构建 `native_retention_preview` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_retention_preview() -> ServiceCall {
    preview("native_retention_preview", json!({}), Preview::VmmRetention)
}

/// Builds the request for `native_retention_run`.
/// 构建 `native_retention_run` 的请求。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn native_retention_run(result: &Value) -> Option<ServiceCall> {
    // Preview id.
    // 服务签发的单次预检身份。
    let preview_id = value_of(result).get("previewId")?.as_str()?;
    if preview_id.is_empty() {
        return None;
    }
    Some(mutate(
        "native_retention_run",
        json!({"previewId":preview_id,"confirmed":true}),
        MAINTENANCE,
    ))
}

/// Builds the request for `native_recycle_restore`.
/// 构建 `native_recycle_restore` 的请求。
/// `batch_id` is the exact retained-content batch identity.
/// `batch_id` 为准确的保留内容批次身份。
/// `storage_version` is the observed recycle-bin content version.
/// `storage_version` 为已观察的回收站内容版本。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_recycle_restore(batch_id: &str, storage_version: i64) -> ServiceCall {
    mutate(
        "native_recycle_restore",
        json!({ "batchId": batch_id, "storageVersion": storage_version }),
        MAINTENANCE,
    )
}

/// Builds the request for `native_recycle_purge`.
/// 构建 `native_recycle_purge` 的请求。
/// `batch_id` is the exact retained-content batch identity.
/// `batch_id` 为准确的保留内容批次身份。
/// `storage_version` is the observed recycle-bin content version.
/// `storage_version` 为已观察的回收站内容版本。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn native_recycle_purge(batch_id: &str, storage_version: i64) -> ServiceCall {
    mutate(
        "native_recycle_purge",
        json!({ "batchId": batch_id, "storageVersion": storage_version }),
        MAINTENANCE,
    )
}
