//! VMM config helpers.
//! VMM config 辅助逻辑。

use super::*;

/// Saves one VMM memory configuration draft.
/// 保存单个 VMM 记忆配置草稿。
///
/// # Parameters
/// 参数
/// `state` is the shared service state used to resolve and refresh persisted settings.
/// `state` 是用于解析并刷新持久化设置的共享服务状态。
/// `request` is the WebUI configuration save request.
/// `request` 是 WebUI 配置保存请求。
///
/// # Returns
/// 返回值
/// `Ok(())` after the configuration file and hook snapshot have been updated.
/// 配置文件与 hook 快照更新完成后返回 `Ok(())`。
pub(crate) fn save_vmm_memory_config(
    state: &SharedServiceBridgeState,
    mut request: VmmConfigSaveRequest,
) -> Result<(), String> {
    let scope = request.scope.trim().to_ascii_lowercase();
    if scope == "project" {
        let project_path = vmm_project_scope_path(state, request.project_id.as_deref())?;
        let mut project_configuration =
            vulcan_provider_config::config::load_project_configuration_file(&PathBuf::from(
                &project_path,
            ))
            .unwrap_or_default();
        project_configuration.vmm = if request.draft.has_override {
            vmm_config_layer_from_draft(&request.draft, false)?
        } else {
            vulcan_vmm::VmmConfigLayer::default()
        };
        vulcan_provider_config::config::save_project_configuration_file(
            &PathBuf::from(&project_path),
            &project_configuration,
        )?;
        update_cached_project_vmm_layer(&project_path, project_configuration.vmm.clone());
        refresh_service_vmm_hook_snapshot(state)?;
        return Ok(());
    }
    let mut configuration = provider_configuration_from_state(state)?;
    if request.draft.grpc_api_key.trim() == WEBUI_REDACTED_SECRET_PLACEHOLDER {
        request.draft.grpc_api_key = configuration.vmm.grpc_api_key.clone().unwrap_or_default();
    }
    configuration.vmm = vmm_config_layer_from_draft(&request.draft, true)?;
    vulcan_provider_config::config::save_configuration_file(&configuration)?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(runtime) = guard.agent_runtime.as_mut() {
        runtime.provider_configuration = configuration.clone();
    }
    refresh_guard_vmm_hook_snapshot(&mut guard, &configuration);
    Ok(())
}

/// Loads one persisted project VMM layer and resolves its effective editable fallback.
/// 加载单个已持久化的项目 VMM 层，并解析其可编辑回退值。
pub(crate) fn load_project_vmm_config(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<VmmConfigDraftSummary, String> {
    let project_path = vmm_project_scope_path(state, Some(project_id))?;
    let provider_configuration = provider_configuration_from_state(state)?;
    let project_layer = cached_project_vmm_layer(&project_path);
    let project_effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        &provider_configuration.vmm,
        Some(&project_layer),
        None,
    );
    Ok(vmm_config_draft_summary(
        &project_layer,
        &project_effective,
        !project_layer.is_empty(),
        false,
    ))
}

/// Converts a WebUI VMM draft into a persisted configuration layer.
/// 将 WebUI VMM 草稿转换为持久化配置层。
///
/// # Parameters
/// 参数
/// `draft` is the editable WebUI VMM configuration summary.
/// `draft` 是 WebUI 可编辑的 VMM 配置摘要。
/// `include_transport` controls whether transport fields should be persisted for this layer.
/// `include_transport` 控制当前配置层是否持久化传输字段。
///
/// # Returns
/// 返回值
/// The persisted VMM configuration layer represented by the draft.
/// 返回该草稿表示的持久化 VMM 配置层。
pub(crate) fn vmm_config_layer_from_draft(
    draft: &VmmConfigDraftSummary,
    include_transport: bool,
) -> Result<vulcan_vmm::VmmConfigLayer, String> {
    Ok(vulcan_vmm::VmmConfigLayer {
        enabled: Some(draft.enabled),
        grpc_target: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_target))
            .flatten(),
        grpc_api_key: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_api_key))
            .flatten(),
        handshake_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.handshake_timeout_ms, "VMM 握手超时"))
            .transpose()?
            .flatten(),
        receive_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.receive_timeout_ms, "VMM 请求超时"))
            .transpose()?
            .flatten(),
        max_receive_message_bytes: include_transport
            .then(|| {
                vmm_parse_optional_usize(&draft.max_receive_message_bytes, "VMM 最大接收字节数")
            })
            .transpose()?
            .flatten(),
        grpc_keepalive_time_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.grpc_keepalive_time_ms, "VMM keepalive 间隔"))
            .transpose()?
            .flatten(),
        grpc_keepalive_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.grpc_keepalive_timeout_ms, "VMM keepalive 超时"))
            .transpose()?
            .flatten(),
        grpc_keepalive_permit_without_calls: include_transport
            .then_some(draft.grpc_keepalive_permit_without_calls),
        user_id: vmm_parse_optional_u64(&draft.user_id, "VMM 用户 ID")?,
        project_id: vmm_parse_optional_u64(&draft.project_id, "VMM 项目 ID")?,
        visible_memory_injection: Some(false),
        implicit_memory_turns: vmm_parse_optional_u32(
            &draft.implicit_memory_turns,
            "临时记忆 TTL",
        )?,
        profile_refresh_turns: vmm_parse_optional_u32(
            &draft.profile_refresh_turns,
            "画像刷新间隔",
        )?,
        profile_node_limit: vmm_parse_optional_u32(&draft.profile_node_limit, "画像节点上限")?,
        session_compact_recall: Some(true),
        precheck_enabled: vmm_policy_from_draft_mode(
            &draft.precheck_mode,
            draft.precheck_enabled,
            include_transport,
            "前置记忆注入",
        )?,
        memory_writeback_enabled: vmm_policy_from_draft_mode(
            &draft.memory_writeback_mode,
            draft.memory_writeback_enabled,
            include_transport,
            "后置记忆提交",
        )?,
        ai_tools_enabled: Some(draft.ai_tools_enabled),
        injection_tag: vmm_non_empty_text(&draft.injection_tag),
        injection_description: vmm_non_empty_text(&draft.injection_description),
        install: None,
    })
}

/// Converts one WebUI policy mode into an optional persisted layer override.
/// 将单个 WebUI 策略模式转换为可选持久化层级覆盖。
fn vmm_policy_from_draft_mode(
    mode: &str,
    enabled: bool,
    force_explicit: bool,
    label: &str,
) -> Result<Option<bool>, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "" => Ok(Some(enabled)),
        "inherit" | "inherit_project" | "inheritproject" | "inherit_system" | "inheritsystem" => {
            Ok(force_explicit.then_some(enabled))
        }
        "enabled" | "on" | "true" => Ok(Some(true)),
        "disabled" | "off" | "false" => Ok(Some(false)),
        other => Err(format!("{label} 策略模式不受支持：{other}")),
    }
}

/// Parses one profile target key from WebUI.
/// 从 WebUI 解析单个画像目标键。
pub(crate) fn parse_vmm_profile_target(value: &str) -> Result<vulcan_vmm::ProfileTarget, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetUser),
        "project" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetProject),
        "team" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetTeam),
        "space" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetSpace),
        "all" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetAll),
        other => Err(format!("不支持的 VMM 画像目标：{other}")),
    }
}

/// Reads one optional string from an action payload.
/// 从动作载荷读取一个可选字符串。
pub(crate) fn vmm_payload_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Reads one boolean from an action payload.
/// 从动作载荷读取一个布尔值。
pub(crate) fn vmm_payload_bool(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Reads one optional u32 from an action payload.
/// 从动作载荷读取一个可选 u32。
pub(crate) fn vmm_payload_u32(payload: &Value, key: &str) -> Option<u32> {
    payload
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
        })
        .and_then(|value| u32::try_from(value).ok())
}

/// Reads one optional i32 from an action payload.
/// 从动作载荷读取一个可选 i32。
pub(crate) fn vmm_payload_i32(payload: &Value, key: &str) -> Option<i32> {
    payload
        .get(key)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
        })
        .and_then(|value| i32::try_from(value).ok())
}

/// Reads one optional u64 from an action payload.
/// 从动作载荷读取一个可选 u64。
pub(crate) fn vmm_payload_u64(payload: &Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
    })
}

/// Reads a string list from an action payload.
/// 从动作载荷读取字符串列表。
pub(crate) fn vmm_payload_string_list(payload: &Value, key: &str) -> Vec<String> {
    if let Some(values) = payload.get(key).and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(Value::as_str)
            .flat_map(vmm_split_text_list)
            .collect();
    }
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(vmm_split_text_list)
        .unwrap_or_default()
}

/// Reads a u64 list from an action payload.
/// 从动作载荷读取 u64 列表。
pub(crate) fn vmm_payload_u64_list(payload: &Value, key: &str) -> Result<Vec<u64>, String> {
    if let Some(values) = payload.get(key).and_then(Value::as_array) {
        return values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
                    .ok_or_else(|| format!("无法解析 VMM u64 id：{value}"))
            })
            .collect();
    }
    vmm_payload_string_list(payload, key)
        .into_iter()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("无法解析 VMM u64 id：{value}"))
        })
        .collect()
}

/// Splits compact text list syntax used by the WebUI forms.
/// 拆分 WebUI 表单使用的紧凑文本列表语法。
pub(crate) fn vmm_split_text_list(value: &str) -> Vec<String> {
    value
        .split(['|', ',', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Returns normalized non-empty text.
/// 返回规范化后的非空文本。
pub(crate) fn vmm_non_empty_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Parses an optional u64 draft field.
/// 解析可选 u64 草稿字段。
pub(crate) fn vmm_parse_optional_u64(value: &str, label: &str) -> Result<Option<u64>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u64>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

/// Parses an optional u32 draft field.
/// 解析可选 u32 草稿字段。
pub(crate) fn vmm_parse_optional_u32(value: &str, label: &str) -> Result<Option<u32>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是 32 位非负整数。"))
}

/// Parses an optional usize draft field.
/// 解析可选 usize 草稿字段。
pub(crate) fn vmm_parse_optional_usize(value: &str, label: &str) -> Result<Option<usize>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<usize>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}
