//! Secret edits of managed and bundled plugins stage a new credential revision, prove the plugin's global MCP
//! servers ready with it, and only then select it.
//! 受管及内建插件的秘密编辑暂存新凭据修订，证明插件全局 MCP 服务器以其就绪后才选定它。

use super::*;
use crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
use vulcan_agent_runtime::mcp::RetainedPluginMcpReplacement;
use vulcan_plugin_runtime::{
    VulcanPluginCredentialEdit, VulcanPluginCredentialKey, VulcanPluginCredentialValues,
    VulcanPluginRuntimeManager,
};
use vulcan_service_api::VulcanPluginCredentialChange;

/// Apply `changes` to the declared MCP secrets of `plugin_id` as one new credential revision.
/// 将 `changes` 作为单个新凭据修订应用到 `plugin_id` 的声明 MCP 秘密。
/// An enabled plugin's global MCP servers restart with the new secrets; a candidate that cannot become ready
/// rejects the whole edit. Returns the refreshed catalog.
/// 已启用插件的全局 MCP 服务器以新秘密重启；无法就绪的候选拒绝整个编辑。返回已刷新目录。
pub(crate) async fn service_set_vulcan_plugin_credentials(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    plugin_id: String,
    changes: Vec<VulcanPluginCredentialChange>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    let plugin_id = plugin_id.trim().to_owned();
    if plugin_id.is_empty() {
        return Err(String::from("插件标识不能为空。"));
    }
    let values = credential_values(changes)?;
    run_luaskills_management_owned("set plugin credentials", move || async move {
        set_plugin_credentials_owned(&app, &state, &plugin_id, &values).await
    })
    .await
}

/// Convert wire changes into slot values; each slot may be named once.
/// 将线协议变更转换为槽位值；每个槽位只能被命名一次。
fn credential_values(
    changes: Vec<VulcanPluginCredentialChange>,
) -> Result<VulcanPluginCredentialValues, String> {
    if changes.is_empty() {
        return Err(String::from("至少需要一项插件凭据变更。"));
    }
    let mut values = VulcanPluginCredentialValues::new();
    for change in changes {
        let key = VulcanPluginCredentialKey {
            server_id: change.server_id.trim().to_owned(),
            secret_key: change.secret_key.trim().to_owned(),
        };
        if key.server_id.is_empty() || key.secret_key.is_empty() {
            return Err(String::from("MCP Server 与 Secret 键均不能为空。"));
        }
        if values.insert(key, change.value).is_some() {
            return Err(String::from("同一 Secret 键只能变更一次。"));
        }
    }
    Ok(values)
}

/// Stage, prove ready, commit and publish one credential edit under the management guard.
/// 在管理守卫下暂存、证明就绪、提交并发布单个凭据编辑。
async fn set_plugin_credentials_owned(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    values: &VulcanPluginCredentialValues,
) -> Result<LuaSkillsCatalogUpdate, String> {
    let runtime = service_vulcan_plugin_runtime(state)?;
    let backend = OsPluginMcpSecretStore;
    let edit = runtime
        .prepare_plugin_credentials(plugin_id, values, &backend)
        .map_err(|error| error.to_string())?;
    let replacement = match ready_candidate_mcp(state, &runtime, &edit, &backend).await {
        Ok(replacement) => replacement,
        Err(error) => {
            let _ = runtime.discard_plugin_credentials(edit, &backend);
            return Err(format!("插件凭据未变更：{error}"));
        }
    };
    let catalog = match runtime.commit_plugin_credentials(edit, &backend) {
        Ok(catalog) => catalog,
        Err(error) => {
            if let Some(replacement) = replacement.as_ref() {
                let _ = replacement.abort_and_drain().await;
            }
            return Err(format!("插件凭据未变更：{error}"));
        }
    };
    if let Some(replacement) = replacement.as_ref()
        && let Err(error) = vulcan_plugin_lifecycle::publish_credential_extension_generation(
            state,
            plugin_id,
            catalog,
            replacement,
        )
    {
        let _ = replacement.abort_and_drain().await;
        return Err(format!(
            "插件凭据已保存，但运行中的 MCP 服务器未能切换到新凭据：{error}；停用后重新启用该插件即可生效。"
        ));
    }
    // Revisions still read by draining servers are collected by a later edit or the next startup.
    // 排空中服务器仍在读取的修订由后续编辑或下次启动回收。
    let _ = runtime.collect_unreachable_plugin_credentials(plugin_id, &backend);
    refresh_plugin_catalog_local_state_and_emit(app, state).await
}

/// Start the enabled candidate's global MCP servers and wait until they are ready.
/// 启动已启用候选的全局 MCP 服务器并等待其就绪。
/// Returns None when the plugin is disabled or has no global server; a failed start is drained before returning.
/// 插件已停用或没有全局服务器时返回 None；启动失败会在返回前排空。
async fn ready_candidate_mcp(
    state: &SharedServiceBridgeState,
    runtime: &VulcanPluginRuntimeManager,
    edit: &VulcanPluginCredentialEdit,
    backend: &OsPluginMcpSecretStore,
) -> Result<Option<RetainedPluginMcpReplacement>, String> {
    let candidate = edit.candidate();
    if candidate.runtime_state != vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled {
        return Ok(None);
    }
    let server_configs =
        plugin_mcp_server_configs_for_descriptors(runtime, &candidate.mcp_servers, backend)?;
    if server_configs.is_empty() {
        return Ok(None);
    }
    let current_mcp_runtime = state
        .lock()
        .map_err(|error| format!("锁定服务状态失败：{error}"))?
        .agent_runtime
        .as_ref()
        .map(|agent_runtime| agent_runtime.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("Agent 扩展代次尚未初始化。"))?;
    let replacement = RetainedPluginMcpReplacement::prepare(
        &current_mcp_runtime,
        &candidate.id,
        candidate.generation,
        server_configs,
    )?;
    let ready = match replacement.dispatch() {
        Ok(()) => replacement.wait_until_ready().await,
        Err(error) => Err(error),
    };
    if let Err(error) = ready {
        let _ = replacement.abort_and_drain().await;
        return Err(error);
    }
    Ok(Some(replacement))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one wire change.
    /// 构造单个线协议变更。
    fn change(
        server_id: &str,
        secret_key: &str,
        value: Option<&str>,
    ) -> VulcanPluginCredentialChange {
        VulcanPluginCredentialChange {
            server_id: server_id.to_owned(),
            secret_key: secret_key.to_owned(),
            value: value.map(str::to_owned),
        }
    }

    /// Requests name each slot once with nonempty identifiers; set and clear map to Some and None.
    /// 请求以非空标识对每个槽位只命名一次；设置与清除分别映射为 Some 与 None。
    #[test]
    fn credential_requests_name_each_slot_once() {
        let values = credential_values(vec![
            change(" remote ", " token ", Some("value")),
            change("remote", "region", None),
        ])
        .expect("valid changes");
        let token = VulcanPluginCredentialKey {
            server_id: String::from("remote"),
            secret_key: String::from("token"),
        };
        assert_eq!(values.len(), 2);
        assert_eq!(values[&token].as_deref(), Some("value"));

        for invalid in [
            Vec::new(),
            vec![change("", "token", None)],
            vec![change("remote", " ", None)],
            vec![
                change("remote", "token", Some("a")),
                change("remote", "token", None),
            ],
        ] {
            credential_values(invalid).expect_err("invalid credential request");
        }
    }

    /// The wire change rejects unknown fields and never prints its value.
    /// 线协议变更拒绝未知字段，且绝不打印其值。
    #[test]
    fn credential_change_wire_format_is_strict_and_redacted() {
        let parsed: VulcanPluginCredentialChange = serde_json::from_value(serde_json::json!({
            "serverId": "remote",
            "secretKey": "token",
            "value": "fixture-secret",
        }))
        .expect("parse change");
        assert!(!format!("{parsed:?}").contains("fixture-secret"));
        let cleared: VulcanPluginCredentialChange = serde_json::from_value(serde_json::json!({
            "serverId": "remote",
            "secretKey": "token",
            "value": null,
        }))
        .expect("parse clear");
        assert_eq!(cleared.value, None);
        serde_json::from_value::<VulcanPluginCredentialChange>(serde_json::json!({
            "serverId": "remote",
            "secretKey": "token",
            "value": "x",
            "extra": true,
        }))
        .expect_err("unknown field");
    }
}
