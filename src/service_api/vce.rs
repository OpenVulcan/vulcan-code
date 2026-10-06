// Mirror of `service/vce-service.ts`: the semantic code engine.
// `service/vce-service.ts` 的镜像：语义代码引擎。

use serde_json::{Value, json};

use super::{CallKind, Preview, ServiceCall, Slot, fresh_command_id};

const PROJECT: &[Slot] = &[Slot::VceStatus, Slot::VceOverview, Slot::VceJob];

/// Previews the exact route and explicit dimension, including removing the current route.
/// 预检准确路由与明确维度，包括移除当前路由。
/// Builds the request for `service_preflight_vce_embedding_route`.
/// 构建 `service_preflight_vce_embedding_route` 的请求。
/// `route` is the catalog route object; Null explicitly removes the route.
/// `route` 为目录路由对象；Null 明确移除路由。
/// `dimension` is the explicit positive embedding dimension, omitted only for route removal.
/// `dimension` 为明确的正向量维度；仅移除路由时省略。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn preflight_vce_embedding_route(route: Value, dimension: Option<u32>) -> ServiceCall {
    // Exact protocol request.
    // 协议请求文档。
    let mut request = json!({"route":route});
    if let Some(dimension) = dimension {
        request["dimension"] = json!(dimension);
    }
    ServiceCall {
        function: "preflight_vce_embedding_route",
        method: "service_preflight_vce_embedding_route",
        params: json!({"request":request}),
        kind: CallKind::Preview(Preview::VceEmbedding),
    }
}

/// Freezes the reviewed dimension, revision and per-project compatibility/rebuild decisions.
/// 冻结已审查的维度、修订与逐项目兼容或重建决定。
/// Builds the request for `service_save_public_ai_inference_route`.
/// 构建 `service_save_public_ai_inference_route` 的请求。
/// `review` is the server-issued embedding preflight with revision and project impacts.
/// `review` 为含修订及项目影响的服务端向量预检。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn save_reviewed_embedding(review: &Value) -> Option<ServiceCall> {
    // A server-issued revision is required even when the selected route is removed.
    // 即使移除已选路由，也必须携带服务端签发的修订标识。
    let revision = review.get("revision")?.as_str()?;
    if revision.is_empty() {
        return None;
    }
    // The candidate is explicitly nullable according to VceEmbeddingRoutePreflight.
    // 根据 VceEmbeddingRoutePreflight，候选路由明确允许为空。
    let candidate = review.get("candidate")?;
    // Preserve the consumer and operation keys of PublicAiInferenceRouteSaveRequest.
    // 保留 PublicAiInferenceRouteSaveRequest 的消费者与操作键。
    let mut request = json!({"consumerProfileId":"vce","operation":"embedding_create",
        "route":null,"vceEmbeddingConfirmation":{"preflightRevision":revision,"decisions":[]}});
    if !candidate.is_null() {
        // The model dimension is a positive u32 in the upstream embedding contract.
        // 上游向量模型协议中的维度为正 u32 数值。
        let dimension = u32::try_from(candidate.get("dimension")?.as_u64()?).ok()?;
        // A present route must be the declared route object rather than an untyped scalar.
        // 非空路由必须为协议规定的路由对象，不能为无类型标量。
        let route = candidate.get("route")?;
        if dimension == 0 || !route.is_object() {
            return None;
        }
        request["route"] = route.clone();
        request["embeddingDimensions"] = json!(dimension);
    }
    // Each project appears once, so an approval cannot silently contradict another one.
    // 每个项目仅出现一次，防止一项批准与另一项批准静默冲突。
    let mut project_ids = std::collections::BTreeSet::new();
    // Defaults remain editable in the review form unless a hard rebuild is mandatory.
    // 默认决定可在审查表单中修改，但强制重建情形除外。
    let mut decisions = Vec::new();
    for project in review.get("projects")?.as_array()? {
        // Identity and compatibility are emitted by the project preflight, not inferred locally.
        // 身份与兼容性由项目预检产出，不在本地推断。
        let project_id = project.pointer("/impact/scope/project_id")?.as_str()?;
        // Compatible.
        // 预检声明的兼容性。
        let compatible = project.get("compatible")?.as_bool()?;
        // Hard rebuild.
        // 预检中的强制重建原因。
        let hard_rebuild = project.pointer("/impact/hardRebuildReason")?;
        if project_id.is_empty()
            || !project_ids.insert(project_id)
            || (!hard_rebuild.is_null() && !hard_rebuild.is_string())
        {
            return None;
        }
        decisions.push(json!({"projectId":project_id,
            "decision":if compatible && hard_rebuild.is_null() {"compatible"} else {"rebuild"}}));
    }
    request["vceEmbeddingConfirmation"]["decisions"] = json!(decisions);
    Some(ServiceCall::mutate(
        "save_public_ai_inference_route",
        "service_save_public_ai_inference_route",
        json!({"request":request}),
        &[Slot::PublicAiSettings, Slot::VceStatus, Slot::VceOverview],
    ))
}

/// Loads the latest job, or the exact supplied job id, for `project_id`.
/// 加载 `project_id` 最近的任务，或所传入准确标识的任务。
/// Builds the request for `service_get_vce_job`.
/// 构建 `service_get_vce_job` 的请求。
/// `project_id` is the exact host project identity.
/// `project_id` 为准确宿主项目身份。
/// `job_id` is the exact job identity; None reads the latest job where allowed.
/// `job_id` 为准确任务身份；协议允许时 None 读取最近任务。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_vce_job(project_id: &str, job_id: Option<&str>) -> ServiceCall {
    ServiceCall::load(
        "load_vce_job",
        "service_get_vce_job",
        json!({"request":{"projectId":project_id,"jobId":job_id}}),
        Slot::VceJob,
    )
}

/// Builds the request for `service_get_vce_configuration_status`.
/// 构建 `service_get_vce_configuration_status` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_vce_configuration_status() -> ServiceCall {
    ServiceCall::load(
        "load_vce_configuration_status",
        "service_get_vce_configuration_status",
        Value::Null,
        Slot::VceStatus,
    )
}

/// Builds the request for `service_get_vce_project_overview`.
/// 构建 `service_get_vce_project_overview` 的请求。
/// `project_id` is the exact host project identity.
/// `project_id` 为准确宿主项目身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_vce_project_overview(project_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_vce_project_overview",
        "service_get_vce_project_overview",
        json!({ "request": { "projectId": project_id } }),
        Slot::VceOverview,
    )
}

/// `settings` is a whole `VceProjectSettingsInput`; `expected_revision` guards concurrent edits.
/// `settings` 为完整的 `VceProjectSettingsInput`；`expected_revision` 防止并发编辑覆盖。
/// Builds the request for `service_update_vce_project_settings`.
/// 构建 `service_update_vce_project_settings` 的请求。
/// `project_id` is the exact host project identity.
/// `project_id` 为准确宿主项目身份。
/// `expected_revision` is the observed project settings revision for optimistic concurrency.
/// `expected_revision` 为用于乐观并发控制的已观察项目设置修订。
/// `settings` is the settings document accepted by the matching save API.
/// `settings` 为对应保存 API 接收的设置文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn update_vce_project_settings(
    project_id: &str,
    expected_revision: i64,
    settings: Value,
) -> ServiceCall {
    ServiceCall::mutate(
        "update_vce_project_settings",
        "service_update_vce_project_settings",
        json!({ "request": {
            "projectId": project_id,
            "expectedRevision": expected_revision,
            "settings": settings,
        } }),
        PROJECT,
    )
}

/// `request` is a `VceJobSubmitRequest`.
/// `request` 为 `VceJobSubmitRequest`。
/// Builds the request for `service_submit_vce_job`.
/// 构建 `service_submit_vce_job` 的请求。
/// `request` is the complete request document declared by the matching WebUI API.
/// `request` 为对应 WebUI API 声明的完整请求文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn submit_vce_job(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "submit_vce_job",
        "service_submit_vce_job",
        json!({ "request": request }),
        PROJECT,
    )
}

/// Builds the request for `service_cancel_vce_job`.
/// 构建 `service_cancel_vce_job` 的请求。
/// `project_id` is the exact host project identity.
/// `project_id` 为准确宿主项目身份。
/// `job_id` is the exact job identity; None reads the latest job where allowed.
/// `job_id` 为准确任务身份；协议允许时 None 读取最近任务。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn cancel_vce_job(project_id: &str, job_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "cancel_vce_job",
        "service_cancel_vce_job",
        json!({ "request": { "projectId": project_id, "jobId": job_id } }),
        PROJECT,
    )
}

/// A read-only plan; `settings_revision` is the overview revision the confirmation freezes.
/// 只读计划；`settings_revision` 为确认时冻结的概览设置修订号。
///
/// `mode` is `repair`, `rebuild`, `cleanup_staging` or `compact`.
/// `mode` 为 `repair`、`rebuild`、`cleanup_staging` 或 `compact`。
/// Builds the request for `service_plan_vce_maintenance`.
/// 构建 `service_plan_vce_maintenance` 的请求。
/// `project_id` is the exact host project identity.
/// `project_id` 为准确宿主项目身份。
/// `mode` is the declared display or maintenance mode for this operation.
/// `mode` 为本操作声明的显示或维护模式。
/// `settings_revision` is the project settings revision frozen by the maintenance review.
/// `settings_revision` 为维护审查冻结的项目设置修订。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn plan_vce_maintenance(project_id: &str, mode: &str, settings_revision: i64) -> ServiceCall {
    ServiceCall {
        function: "plan_vce_maintenance",
        method: "service_plan_vce_maintenance",
        params: json!({ "request": { "projectId": project_id, "mode": mode } }),
        kind: CallKind::Preview(Preview::VceMaintenance { settings_revision }),
    }
}

/// `executeVceMaintenance` of an accepted plan, with a fresh idempotency key.
/// 以新的幂等键执行已接受的计划。
/// Builds the request for `service_execute_vce_maintenance`.
/// 构建 `service_execute_vce_maintenance` 的请求。
/// `plan` is the maintenance preview call owning the project and mode.
/// `plan` 为持有项目及模式的维护预检调用。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// `settings_revision` is the project settings revision frozen by the maintenance review.
/// `settings_revision` 为维护审查冻结的项目设置修订。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn execute_vce_maintenance(
    plan: &ServiceCall,
    result: &Value,
    settings_revision: i64,
) -> Option<ServiceCall> {
    // Action ids.
    // 已冻结维护动作身份。
    let mut action_ids = Vec::new();
    for action in result.get("actions")?.as_array()? {
        // Exact entity identity.
        // 准确实体身份。
        let id = action.get("actionId")?.as_str()?;
        if id.is_empty() || action_ids.iter().any(|existing| existing == id) {
            return None;
        }
        action_ids.push(id.to_string());
    }
    if action_ids.is_empty() {
        return None;
    }
    // Fingerprint.
    // 服务签发的内容或预检指纹。
    let fingerprint = result.get("embeddingFingerprint")?.as_str()?;
    // Checkpoint.
    // 已观察检查点版本。
    let checkpoint = result.get("checkpointVersion")?;
    if fingerprint.is_empty() || (!checkpoint.is_null() && checkpoint.as_u64().is_none()) {
        return None;
    }
    // Mode.
    // 已声明操作模式。
    let mode = plan.params.pointer("/request/mode")?.as_str()?;
    if result.get("mode")?.as_str()? != mode || (checkpoint.is_null() && mode != "rebuild") {
        return None;
    }
    Some(ServiceCall::mutate(
        "execute_vce_maintenance",
        "service_execute_vce_maintenance",
        json!({"request":{
            "idempotencyKey":fresh_command_id(),
            "expectedEmbeddingFingerprint":fingerprint,
            "projectId":plan.params.pointer("/request/projectId")?.as_str()?,
            "mode":mode,"expectedSettingsRevision":settings_revision,
            "expectedCheckpointVersion":checkpoint,"actionIds":action_ids
        }}),
        PROJECT,
    ))
}
