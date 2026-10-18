// Mirror of `service/permission-judge-service.ts`.
// `service/permission-judge-service.ts` 的镜像。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

// A registry or settings change refreshes the page snapshot and the live status together.
// 登记表或设置变化会同时刷新页面快照与实时状态。
const JUDGE: &[Slot] = &[Slot::PermissionJudgeSettings, Slot::PermissionJudgeStatus];

/// Builds the request for `service_permission_judge_settings`, including device and CUDA probing.
/// 构建 `service_permission_judge_settings` 的请求，包括设备与 CUDA 探测。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_permission_judge_settings() -> ServiceCall {
    ServiceCall::load(
        "load_permission_judge_settings",
        "service_permission_judge_settings",
        json!({}),
        Slot::PermissionJudgeSettings,
    )
}

/// Builds the request for `service_permission_judge_status`, the live worker and install status.
/// 构建 `service_permission_judge_status` 的请求，即实时 worker 与安装状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_permission_judge_status() -> ServiceCall {
    ServiceCall::load(
        "load_permission_judge_status",
        "service_permission_judge_status",
        json!({}),
        Slot::PermissionJudgeStatus,
    )
}

/// Builds the request for `service_save_permission_judge_settings`.
/// 构建 `service_save_permission_judge_settings` 的请求。
/// `request` is the complete `PermissionJudgeSettingsSaveRequest`.
/// `request` 为完整的 `PermissionJudgeSettingsSaveRequest`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_permission_judge_settings(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_permission_judge_settings",
        "service_save_permission_judge_settings",
        json!({ "request": request }),
        JUDGE,
    )
}

/// Builds the request for `service_inspect_permission_judge_package`; the result replaces the
/// registration form's check slot and nothing is registered.
/// 构建 `service_inspect_permission_judge_package` 的请求；结果替换登记表单的检查槽位，不登记任何内容。
/// `request` is `{path}`, collected by the check form.
/// `request` 为检查表单收集的 `{path}`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn inspect_permission_judge_package(request: Value) -> ServiceCall {
    ServiceCall::load(
        "inspect_permission_judge_package",
        "service_inspect_permission_judge_package",
        json!({ "request": request }),
        Slot::PermissionJudgePackageInspection,
    )
}

/// Builds the request for `service_register_permission_judge_model`.
/// 构建 `service_register_permission_judge_model` 的请求。
/// `request` is `{path, name, custom}`, collected by the registration form.
/// `request` 为注册表单收集的 `{path, name, custom}`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn register_permission_judge_model(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "register_permission_judge_model",
        "service_register_permission_judge_model",
        json!({ "request": request }),
        JUDGE,
    )
}

/// Builds the request for `service_activate_permission_judge_model`.
/// 构建 `service_activate_permission_judge_model` 的请求。
/// `model_id` is the registered model to verify, warm and switch to.
/// `model_id` 为要验证、预热并切换到的已登记模型。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn activate_permission_judge_model(model_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "activate_permission_judge_model",
        "service_activate_permission_judge_model",
        json!({ "request": { "modelId": model_id } }),
        JUDGE,
    )
}

/// Builds the request for `service_rollback_permission_judge_model`.
/// 构建 `service_rollback_permission_judge_model` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn rollback_permission_judge_model() -> ServiceCall {
    ServiceCall::mutate(
        "rollback_permission_judge_model",
        "service_rollback_permission_judge_model",
        json!({}),
        JUDGE,
    )
}

/// Builds the request for `service_remove_permission_judge_model`; files stay on disk.
/// 构建 `service_remove_permission_judge_model` 的请求；文件保留在磁盘上。
/// `model_id` is the registration to remove; it must not be in use.
/// `model_id` 为要移除的登记；它不能正在使用。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn remove_permission_judge_model(model_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "remove_permission_judge_model",
        "service_remove_permission_judge_model",
        json!({ "request": { "modelId": model_id } }),
        JUDGE,
    )
}

/// Builds the request for `service_reload_permission_judge`, retrying the model in use.
/// 构建 `service_reload_permission_judge` 的请求，重新加载使用中模型。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn reload_permission_judge() -> ServiceCall {
    ServiceCall::mutate(
        "reload_permission_judge",
        "service_reload_permission_judge",
        json!({}),
        &[Slot::PermissionJudgeStatus],
    )
}

/// Builds the request for `service_benchmark_permission_judge`; the result replaces the page's
/// speed test slot and is never saved.
/// 构建 `service_benchmark_permission_judge` 的请求；结果替换本页速度测试槽位，不会持久化。
/// `batch_size` is 1 or 10.
/// `batch_size` 为 1 或 10。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn benchmark_permission_judge(batch_size: u8) -> ServiceCall {
    ServiceCall::load(
        "benchmark_permission_judge",
        "service_benchmark_permission_judge",
        json!({ "request": { "batchSize": batch_size } }),
        Slot::PermissionJudgeBenchmark,
    )
}

/// Builds the request for `service_install_permission_judge_cuda`.
/// 构建 `service_install_permission_judge_cuda` 的请求。
/// `component` is `onnxruntime` or `cuda_libraries`, as published by the CUDA snapshot.
/// `component` 为 CUDA 快照发布的 `onnxruntime` 或 `cuda_libraries`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn install_permission_judge_cuda(component: &str) -> ServiceCall {
    ServiceCall::mutate(
        "install_permission_judge_cuda",
        "service_install_permission_judge_cuda",
        json!({ "request": { "component": component } }),
        &[Slot::PermissionJudgeStatus],
    )
}

/// Builds the request for `service_permission_judge_records`, one page of the decision log.
/// 构建 `service_permission_judge_records` 的请求，即判定记录的一页。
/// `request` is the complete `PermissionJudgeRecordsRequest` with filters, cursor and limit.
/// `request` 为包含筛选、游标与数量的完整 `PermissionJudgeRecordsRequest`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_permission_judge_records(request: Value) -> ServiceCall {
    ServiceCall::load(
        "load_permission_judge_records",
        "service_permission_judge_records",
        json!({ "request": request }),
        Slot::PermissionJudgeRecords,
    )
}

/// Builds the request for `service_mark_permission_judge_record`.
/// 构建 `service_mark_permission_judge_record` 的请求。
/// `correction` is the label the judge should have given, or `None` to clear the mark.
/// `correction` 为判定器本应给出的标签；为 `None` 时清除标记。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn mark_permission_judge_record(id: &str, correction: Option<&str>) -> ServiceCall {
    ServiceCall::mutate(
        "mark_permission_judge_record",
        "service_mark_permission_judge_record",
        json!({ "request": { "id": id, "correction": correction } }),
        &[Slot::PermissionJudgeRecords],
    )
}

/// Builds the request for `service_clear_permission_judge_records`, removing every record.
/// 构建 `service_clear_permission_judge_records` 的请求，删除全部记录。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn clear_permission_judge_records() -> ServiceCall {
    ServiceCall::mutate(
        "clear_permission_judge_records",
        "service_clear_permission_judge_records",
        json!({}),
        &[Slot::PermissionJudgeRecords],
    )
}

/// Builds the request for `service_project_permission_judge_policy`.
/// 构建 `service_project_permission_judge_policy` 的请求。
/// `project_id` is the project whose choices for the model in use are read.
/// `project_id` 为读取其针对使用中模型之选择的项目。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_project_permission_judge_policy(project_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_project_permission_judge_policy",
        "service_project_permission_judge_policy",
        json!({ "projectId": project_id }),
        Slot::ProjectPermissionJudgePolicy,
    )
}

/// Builds the request for `service_save_project_permission_judge_policy`.
/// 构建 `service_save_project_permission_judge_policy` 的请求。
/// `options` is the complete choice map; omitted options follow the default.
/// `options` 为完整选择映射；未列出的选项跟随默认值。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_project_permission_judge_policy(project_id: &str, options: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_project_permission_judge_policy",
        "service_save_project_permission_judge_policy",
        json!({ "request": { "projectId": project_id, "options": options } }),
        &[Slot::ProjectPermissionJudgePolicy],
    )
}

// A system default change also changes what a project that follows it shows.
// 系统默认值变化也会改变跟随它的项目所显示的内容。
const UNATTENDED: &[Slot] = &[
    Slot::UnattendedSystemDefault,
    Slot::ProjectUnattendedDefault,
];

/// Builds the request for `service_unattended_system_default`.
/// 构建 `service_unattended_system_default` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_unattended_system_default() -> ServiceCall {
    ServiceCall::load(
        "load_unattended_system_default",
        "service_unattended_system_default",
        json!({}),
        Slot::UnattendedSystemDefault,
    )
}

/// Builds the request for `service_save_unattended_system_default`; sessions that are already
/// unattended keep the text they confirmed.
/// 构建 `service_save_unattended_system_default` 的请求；已处于无人值守的会话保留其确认的文本。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_unattended_system_default(deny_policy: &str) -> ServiceCall {
    ServiceCall::mutate(
        "save_unattended_system_default",
        "service_save_unattended_system_default",
        json!({ "request": { "denyPolicy": deny_policy } }),
        UNATTENDED,
    )
}

/// Builds the request for `service_project_unattended_default`.
/// 构建 `service_project_unattended_default` 的请求。
/// `project_id` is the project whose default deny policy is read.
/// `project_id` 为读取其默认拒绝策略的项目。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_project_unattended_default(project_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_project_unattended_default",
        "service_project_unattended_default",
        json!({ "projectId": project_id }),
        Slot::ProjectUnattendedDefault,
    )
}

/// Builds the request for `service_save_project_unattended_default`.
/// 构建 `service_save_project_unattended_default` 的请求。
/// `deny_policy` is the project's own text, or `None` to follow the system default.
/// `deny_policy` 为项目自有文本；为 `None` 时跟随系统默认值。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_project_unattended_default(project_id: &str, deny_policy: Option<&str>) -> ServiceCall {
    ServiceCall::mutate(
        "save_project_unattended_default",
        "service_save_project_unattended_default",
        json!({ "request": { "projectId": project_id, "denyPolicy": deny_policy } }),
        &[Slot::ProjectUnattendedDefault],
    )
}

// A system rules change also changes what a project that follows them shows.
// 系统规则变化也会改变跟随它的项目所显示的内容。
const REVIEW_RULES: &[Slot] = &[Slot::ReviewRulesSystemDefault, Slot::ProjectReviewRules];

/// Builds the request for `service_review_rules_system_default`.
/// 构建 `service_review_rules_system_default` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_review_rules_system_default() -> ServiceCall {
    ServiceCall::load(
        "load_review_rules_system_default",
        "service_review_rules_system_default",
        json!({}),
        Slot::ReviewRulesSystemDefault,
    )
}

/// Builds the request for `service_save_review_rules_system_default`; the next review reads them.
/// 构建 `service_save_review_rules_system_default` 的请求；下一次复核即读取它们。
/// `rules` is the complete `{ allow, ask, deny }` object.
/// `rules` 为完整的 `{ allow, ask, deny }` 对象。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_review_rules_system_default(rules: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_review_rules_system_default",
        "service_save_review_rules_system_default",
        json!({ "request": { "rules": rules } }),
        REVIEW_RULES,
    )
}

/// Builds the request for `service_project_review_rules`.
/// 构建 `service_project_review_rules` 的请求。
/// `project_id` is the project whose review rules are read.
/// `project_id` 为读取其复核规则的项目。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_project_review_rules(project_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_project_review_rules",
        "service_project_review_rules",
        json!({ "projectId": project_id }),
        Slot::ProjectReviewRules,
    )
}

/// Builds the request for `service_save_project_review_rules`.
/// 构建 `service_save_project_review_rules` 的请求。
/// `rules` is the project's complete rules object, or `Value::Null` to follow the system rules.
/// `rules` 为项目完整的规则对象；为 `Value::Null` 时跟随系统规则。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_project_review_rules(project_id: &str, rules: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_project_review_rules",
        "service_save_project_review_rules",
        json!({ "request": { "projectId": project_id, "rules": rules } }),
        &[Slot::ProjectReviewRules],
    )
}
