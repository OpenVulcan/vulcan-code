// Mirror of `service/skills-actions-service.ts`: Agent Skills, subagents, LuaSkills and plugins.
// `service/skills-actions-service.ts` 的镜像：Agent Skills、子代理、LuaSkills 与插件。

use serde_json::{Value, json};

use super::{CallKind, Preview, ServiceCall, Slot};

const WORKBENCH: &[Slot] = &[Slot::Workbench];

fn mutate(function: &'static str, method: &'static str, params: Value) -> ServiceCall {
    ServiceCall::mutate(function, method, params, WORKBENCH)
}

// ---- LuaSkills ----

/// Builds the request for `service_refresh_luaskills_catalog`.
/// 构建 `service_refresh_luaskills_catalog` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn refresh_luaskills_catalog() -> ServiceCall {
    mutate(
        "refresh_luaskills_catalog",
        "service_refresh_luaskills_catalog",
        Value::Null,
    )
}

/// `target` is `node`, `python` or `all`.
/// `target` 为 `node`、`python` 或 `all`。
/// Builds the request for `service_retry_luaskills_managed_runtimes`.
/// 构建 `service_retry_luaskills_managed_runtimes` 的请求。
/// `target` is the declared user/project profile target or managed-runtime selector.
/// `target` 为已声明的用户或项目画像目标，或受管理运行时选择器。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn retry_luaskills_managed_runtimes(target: &str) -> ServiceCall {
    mutate(
        "retry_luaskills_managed_runtimes",
        "service_retry_luaskills_managed_runtimes",
        json!({ "target": target }),
    )
}

/// `settings` is the whole `LuaSkillsRuntimeSettings` document.
/// `settings` 为完整的 `LuaSkillsRuntimeSettings` 文档。
/// Builds the request for `service_save_luaskills_runtime_settings`.
/// 构建 `service_save_luaskills_runtime_settings` 的请求。
/// `settings` is the settings document accepted by the matching save API.
/// `settings` 为对应保存 API 接收的设置文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_luaskills_runtime_settings(settings: Value) -> ServiceCall {
    mutate(
        "save_luaskills_runtime_settings",
        "service_save_luaskills_runtime_settings",
        json!({ "settings": settings }),
    )
}

/// An empty `skill_id` lets the service derive it from the source, as the WebUI does.
/// `skill_id` 为空时由服务从来源推导，与 WebUI 一致。
/// Builds the request for `service_install_luaskill`.
/// 构建 `service_install_luaskill` 的请求。
/// `skill_id` is the LuaSkills catalog identity.
/// `skill_id` 为LuaSkills 目录身份。
/// `source` is the explicit installation source admitted by LuaSkills.
/// `source` 为LuaSkills 准入的明确安装来源。
/// `source_type` is the declared installation source family.
/// `source_type` 为已声明的安装来源类别。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn install_luaskill(skill_id: &str, source: &str, source_type: &str) -> ServiceCall {
    // Skill id.
    // 准确技能身份。
    let skill_id = skill_id.trim();
    mutate(
        "install_luaskill",
        "service_install_luaskill",
        json!({
            "skillId": (!skill_id.is_empty()).then_some(skill_id),
            "source": source,
            "sourceType": source_type,
        }),
    )
}

/// Builds the request for `service_search_luaskills_hub`.
/// 构建 `service_search_luaskills_hub` 的请求。
/// `query` is the search text; empty lists the Hub's default ranking.
/// `query` 为搜索文本；为空时列出 Hub 默认排序。
/// `cursor` continues a previous page, `None` starts over.
/// `cursor` 继续上一页，`None` 表示重新开始。
/// `locale` is the interface language the Hub localizes summaries into.
/// `locale` 是 Hub 本地化摘要所用的界面语言。
/// Returns an unexecuted ServiceCall whose page replaces the Hub result slot.
/// 返回尚未执行的 ServiceCall，其结果页替换 Hub 结果槽位。
pub fn search_luaskills_hub(query: &str, cursor: Option<&str>, locale: &str) -> ServiceCall {
    // Search text sent as null when blank, as the WebUI does.
    // 空白时以 null 发送的搜索文本，与 WebUI 一致。
    let query = query.trim();
    ServiceCall::load(
        "search_luaskills_hub",
        "service_search_luaskills_hub",
        json!({
            "query": (!query.is_empty()).then_some(query),
            "cursor": cursor,
            "locale": locale,
        }),
        Slot::LuaSkillsHub,
    )
}

/// Builds the request for `service_get_luaskills_hub_skill`.
/// 构建 `service_get_luaskills_hub_skill` 的请求。
/// `skill_id` is the exact Hub skill identity; `locale` is the interface language.
/// `skill_id` 为准确的 Hub 技能身份；`locale` 为界面语言。
/// Returns an unexecuted ServiceCall whose detail replaces the Hub detail slot.
/// 返回尚未执行的 ServiceCall，其详情替换 Hub 详情槽位。
pub fn get_luaskills_hub_skill(skill_id: &str, locale: &str) -> ServiceCall {
    ServiceCall::load(
        "get_luaskills_hub_skill",
        "service_get_luaskills_hub_skill",
        json!({
            "skillId": skill_id,
            "locale": locale,
        }),
        Slot::LuaSkillsHubSkill,
    )
}

/// Builds the request for `service_update_luaskill`.
/// 构建 `service_update_luaskill` 的请求。
/// `skill_id` is the LuaSkills catalog identity.
/// `skill_id` 为LuaSkills 目录身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn update_luaskill(skill_id: &str) -> ServiceCall {
    mutate(
        "update_luaskill",
        "service_update_luaskill",
        json!({ "skillId": skill_id }),
    )
}

/// Builds the request for `service_set_luaskill_enabled`.
/// 构建 `service_set_luaskill_enabled` 的请求。
/// `skill_id` is the LuaSkills catalog identity.
/// `skill_id` 为LuaSkills 目录身份。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn set_luaskill_enabled(skill_id: &str, enabled: bool) -> ServiceCall {
    mutate(
        "set_luaskill_enabled",
        "service_set_luaskill_enabled",
        json!({ "skillId": skill_id, "enabled": enabled }),
    )
}

/// Builds the request for `service_uninstall_luaskill`.
/// 构建 `service_uninstall_luaskill` 的请求。
/// `skill_id` is the LuaSkills catalog identity.
/// `skill_id` 为LuaSkills 目录身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn uninstall_luaskill(skill_id: &str) -> ServiceCall {
    mutate(
        "uninstall_luaskill",
        "service_uninstall_luaskill",
        json!({ "skillId": skill_id }),
    )
}

/// `registration_state` is `explicit`, `implicit` or `disabled`.
/// `registration_state` 为 `explicit`、`implicit` 或 `disabled`。
/// Builds the request for `service_set_luaskill_tool_registration_state`.
/// 构建 `service_set_luaskill_tool_registration_state` 的请求。
/// `skill_id` is the LuaSkills catalog identity.
/// `skill_id` 为LuaSkills 目录身份。
/// `local_name` is the local tool name owned by the selected skill.
/// `local_name` 为所选技能持有的本地工具名。
/// `registration_state` is explicit, implicit or disabled tool registration.
/// `registration_state` 为显式、隐式或禁用工具注册状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn set_luaskill_tool_registration_state(
    skill_id: &str,
    local_name: &str,
    registration_state: &str,
) -> ServiceCall {
    mutate(
        "set_luaskill_tool_registration_state",
        "service_set_luaskill_tool_registration_state",
        json!({
            "skillId": skill_id,
            "localName": local_name,
            "registrationState": registration_state,
        }),
    )
}

// ---- Vulcan plugins ----

/// Renderer nonce sent with every plugin action, one per process like the WebUI's per-window
/// `pluginActionClientId`; it ties status queries and cancellation to this client.
/// 每次插件 Action 携带的渲染器随机标识，与 WebUI 每窗口一个的 `pluginActionClientId` 一致，按进程
/// 生成一次；它把状态查询与取消绑定到本客户端。
fn plugin_action_client_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(super::fresh_command_id)
}

/// Builds the request for `service_load_plugin_ui_module`: the layout or module source of one
/// mount, with the configuration schema and plugin messages.
/// 构建 `service_load_plugin_ui_module` 的请求：单个挂载的版面或模块源码，以及配置 Schema 与插件文案。
/// `plugin_id` and `mount_id` are the exact identities the workbench publishes.
/// `plugin_id` 与 `mount_id` 为工作台发布的准确身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_plugin_ui_module(plugin_id: &str, mount_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_plugin_ui_module",
        "service_load_plugin_ui_module",
        json!({ "pluginId": plugin_id, "mountId": mount_id }),
        Slot::PluginUi,
    )
}

/// Builds the request for `service_read_plugin_text_asset`.
/// 构建 `service_read_plugin_text_asset` 的请求。
/// `plugin_id` owns the package and `path` is one exact `required_files` entry.
/// `plugin_id` 为所属插件，`path` 为一个准确的 `required_files` 条目。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn read_plugin_text_asset(plugin_id: &str, path: &str) -> ServiceCall {
    ServiceCall::load(
        "read_plugin_text_asset",
        "service_read_plugin_text_asset",
        json!({ "pluginId": plugin_id, "path": path }),
        Slot::PluginAsset,
    )
}

/// Builds the request for `service_invoke_plugin_action` from a global plugin page, which sends
/// no workspace path. Each call carries a fresh invocation identity; replays renew it.
/// 构建全局插件页面的 `service_invoke_plugin_action` 请求，不发送工作区路径。每次调用携带新的调用
/// 身份；重放时会更新它。
/// `plugin_id`, `action_id` and `runtime_identity` come from the loaded mount; `arguments` is
/// the action input; `kind` routes the result (a read loads a slot, a write reloads slots).
/// `plugin_id`、`action_id` 与 `runtime_identity` 来自已加载挂载；`arguments` 为 Action 输入；
/// `kind` 决定结果去向（读取写入槽位，写入重新加载槽位）。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn invoke_plugin_action(
    plugin_id: &str,
    action_id: &str,
    arguments: Value,
    runtime_identity: &str,
    kind: CallKind,
) -> ServiceCall {
    ServiceCall {
        function: "invoke_plugin_action",
        method: "service_invoke_plugin_action",
        params: json!({
            "identity": {
                "clientId": plugin_action_client_id(),
                "invocationId": super::fresh_command_id(),
            },
            "runtimeIdentity": runtime_identity,
            "pluginId": plugin_id,
            "actionId": action_id,
            "arguments": arguments,
            "workspacePath": null,
        }),
        kind,
    }
}

/// Builds the request for `service_set_vulcan_plugin_enabled`.
/// 构建 `service_set_vulcan_plugin_enabled` 的请求。
/// `plugin_id` is the plugin ID published by the workbench.
/// `plugin_id` 为工作台发布的插件标识。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn set_vulcan_plugin_enabled(plugin_id: &str, enabled: bool) -> ServiceCall {
    mutate(
        "set_vulcan_plugin_enabled",
        "service_set_vulcan_plugin_enabled",
        json!({ "pluginId": plugin_id, "enabled": enabled }),
    )
}

/// First step of a local install: the service reads the package and returns the permissions and
/// fingerprint the user must approve.
/// 本地安装的第一步：服务读取插件包，返回需要用户批准的权限与指纹。
/// Builds the request for `service_inspect_local_plugin`.
/// 构建 `service_inspect_local_plugin` 的请求。
/// `source_path` is the exact local plugin package path to inspect.
/// `source_path` 为需要检查的准确本地插件包路径。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn inspect_local_plugin(source_path: &str) -> ServiceCall {
    ServiceCall {
        function: "inspect_local_plugin",
        method: "service_inspect_local_plugin",
        params: json!({ "sourcePath": source_path }),
        kind: CallKind::Preview(Preview::PluginInstall),
    }
}

/// Second step: `installLocalPlugin`, or `updateLocalPlugin` when the package replaces an
/// installed user plugin. Confirming the dialog approves every listed permission, including
/// registry dependencies, as the WebUI's authorization checkboxes do.
/// 第二步：`installLocalPlugin`，或在替换已安装用户插件时使用 `updateLocalPlugin`。确认对话框即
/// 批准所列全部权限（包括注册表依赖），与 WebUI 的授权勾选一致。
/// Builds the request for `service_update_local_plugin`, `service_install_local_plugin`.
/// 构建 `service_update_local_plugin`、`service_install_local_plugin` 的请求。
/// `preview` is the read-only first-step call or its declared preview selector.
/// `preview` 为只读第一步调用或其声明的预检选择器。
/// `result` is the exact service response for the preceding preview.
/// `result` 为前序预检的准确服务响应。
/// `update` is whether the reviewed package replaces an installed user plugin.
/// `update` 为已审查插件包是否替换已安装用户插件。
/// Returns an unexecuted call; None rejects incomplete or invalid preview evidence.
/// 返回尚未执行的调用；None 拒绝缺失或无效的预检证据。
pub fn install_local_plugin(
    preview: &ServiceCall,
    result: &Value,
    update: bool,
) -> Option<ServiceCall> {
    // Permissions.
    // 预检声明权限。
    let permissions = result.get("permissions")?.as_array()?;
    if permissions
        .iter()
        .any(|permission| permission.as_str().is_none())
    {
        return None;
    }
    // Registry.
    // 是否批准注册表依赖访问。
    let registry = permissions
        .iter()
        .any(|permission| permission == "dependency_registry_network");
    // Fingerprint.
    // 服务签发的内容或预检指纹。
    let fingerprint = result.get("contentFingerprint")?.as_str()?;
    if fingerprint.is_empty() {
        return None;
    }
    // One-shot installation secret inputs.
    // 仅供安装时单次发送的密钥输入。
    let mut initial_secrets = Vec::new();
    for requirement in result.get("installSecretRequirements")?.as_array()? {
        initial_secrets.push(json!({
            "serverId":requirement.get("serverId")?.as_str()?,
            "secretKey":requirement.get("secretKey")?.as_str()?, "value":""
        }));
    }
    // Function; Method.
    // 准确 WebUI 函数名；准确 RPC 方法名。
    let (function, method) = if update {
        ("update_local_plugin", "service_update_local_plugin")
    } else {
        ("install_local_plugin", "service_install_local_plugin")
    };
    Some(mutate(
        function,
        method,
        json!({
            "sourcePath":preview.params.get("sourcePath")?.as_str()?,
            "contentFingerprint":fingerprint,
            "approvedPermissions":permissions,
            "allowRegistryDependencies":registry, "initialSecrets":initial_secrets,
        }),
    ))
}

/// Builds the request for `service_uninstall_user_plugin`.
/// 构建 `service_uninstall_user_plugin` 的请求。
/// `plugin_id` is the plugin ID published by the workbench.
/// `plugin_id` 为工作台发布的插件标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn uninstall_user_plugin(plugin_id: &str) -> ServiceCall {
    mutate(
        "uninstall_user_plugin",
        "service_uninstall_user_plugin",
        json!({ "pluginId": plugin_id }),
    )
}

// ---- Agent Skills ----

/// Builds the request for `service_refresh_agent_skills_catalog`.
/// 构建 `service_refresh_agent_skills_catalog` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn refresh_agent_skills_catalog() -> ServiceCall {
    mutate(
        "refresh_agent_skills_catalog",
        "service_refresh_agent_skills_catalog",
        Value::Null,
    )
}

/// Builds the request for `service_open_agent_skills_user_directory`.
/// 构建 `service_open_agent_skills_user_directory` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn open_agent_skills_user_directory() -> ServiceCall {
    ServiceCall::mutate(
        "open_agent_skills_user_directory",
        "service_open_agent_skills_user_directory",
        Value::Null,
        &[],
    )
}

/// Builds the request for `service_set_agent_skill_enabled`.
/// 构建 `service_set_agent_skill_enabled` 的请求。
/// `name` is the requested display name or catalog-owned skill name.
/// `name` 为请求的显示名称或目录持有的技能名称。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn set_agent_skill_enabled(name: &str, enabled: bool) -> ServiceCall {
    mutate(
        "set_agent_skill_enabled",
        "service_set_agent_skill_enabled",
        json!({ "name": name, "enabled": enabled }),
    )
}

/// Builds the request for `service_delete_agent_skill`.
/// 构建 `service_delete_agent_skill` 的请求。
/// `name` is the requested display name or catalog-owned skill name.
/// `name` 为请求的显示名称或目录持有的技能名称。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn delete_agent_skill(name: &str) -> ServiceCall {
    mutate(
        "delete_agent_skill",
        "service_delete_agent_skill",
        json!({ "name": name }),
    )
}

/// Builds the request for `service_restore_agent_skill_default`.
/// 构建 `service_restore_agent_skill_default` 的请求。
/// `name` is the requested display name or catalog-owned skill name.
/// `name` 为请求的显示名称或目录持有的技能名称。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn restore_agent_skill_default(name: &str) -> ServiceCall {
    mutate(
        "restore_agent_skill_default",
        "service_restore_agent_skill_default",
        json!({ "name": name }),
    )
}

// ---- Subagents ----

/// Builds the request for `service_refresh_subagent_catalog`.
/// 构建 `service_refresh_subagent_catalog` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn refresh_subagent_catalog() -> ServiceCall {
    mutate(
        "refresh_subagent_catalog",
        "service_refresh_subagent_catalog",
        Value::Null,
    )
}

/// Builds the request for `service_open_subagent_user_directory`.
/// 构建 `service_open_subagent_user_directory` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn open_subagent_user_directory() -> ServiceCall {
    ServiceCall::mutate(
        "open_subagent_user_directory",
        "service_open_subagent_user_directory",
        Value::Null,
        &[],
    )
}

/// Builds the request for `service_set_subagent_definition_enabled`.
/// 构建 `service_set_subagent_definition_enabled` 的请求。
/// `agent_id` is the subagent definition identity.
/// `agent_id` 为子代理定义身份。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn set_subagent_definition_enabled(agent_id: &str, enabled: bool) -> ServiceCall {
    mutate(
        "set_subagent_definition_enabled",
        "service_set_subagent_definition_enabled",
        json!({ "agentId": agent_id, "enabled": enabled }),
    )
}

/// Builds the request for `service_delete_subagent_definition`.
/// 构建 `service_delete_subagent_definition` 的请求。
/// `agent_id` is the subagent definition identity.
/// `agent_id` 为子代理定义身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn delete_subagent_definition(agent_id: &str) -> ServiceCall {
    mutate(
        "delete_subagent_definition",
        "service_delete_subagent_definition",
        json!({ "agentId": agent_id }),
    )
}

/// Builds the request for `service_restore_subagent_definition_default`.
/// 构建 `service_restore_subagent_definition_default` 的请求。
/// `agent_id` is the subagent definition identity.
/// `agent_id` 为子代理定义身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn restore_subagent_definition_default(agent_id: &str) -> ServiceCall {
    mutate(
        "restore_subagent_definition_default",
        "service_restore_subagent_definition_default",
        json!({ "agentId": agent_id }),
    )
}
