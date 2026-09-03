//! VMM config helpers.
//! VMM config 辅助逻辑。

use super::*;
use crate::{mutate_project_configuration, mutate_provider_configuration};

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
pub(crate) async fn save_vmm_memory_config(
    state: &SharedServiceBridgeState,
    mut request: VmmConfigSaveRequest,
) -> Result<(), String> {
    if request.draft.enabled {
        validate_vmm_required_inference(state)?;
    }
    let scope = request.scope.trim().to_ascii_lowercase();
    if scope == "project" {
        let project_path = vmm_project_scope_path(state, request.project_id.as_deref())?;
        return mutate_project_configuration(
            &PathBuf::from(&project_path),
            "保存项目 VMM 配置失败",
            |project_configuration| {
                project_configuration.vmm = if request.draft.has_override {
                    vmm_config_layer_from_draft(&request.draft, false)?
                } else {
                    vulcan_vmm::VmmConfigLayer::default()
                };
                Ok(())
            },
            |project_configuration| {
                update_cached_project_vmm_layer(&project_path, project_configuration.vmm.clone());
                refresh_exact_vmm_hook_project_projection(
                    state,
                    &project_path,
                    &project_configuration.vmm,
                )
            },
        )
        .await;
    }
    if scope != "global" {
        return Err(format!("不支持的 VMM 配置作用域：{scope}"));
    }
    // A saved global transport may point at a different endpoint, so no previous probe remains authoritative.
    // 保存后的全局传输可能指向其他端点，因此此前的探测结果不再具备权威性。
    clear_external_vmm_health_probe();
    mutate_provider_configuration(
        state,
        "保存 VMM 全局配置失败",
        |configuration| {
            if request.draft.grpc_api_key.trim() == WEBUI_REDACTED_SECRET_PLACEHOLDER {
                request.draft.grpc_api_key =
                    configuration.vmm.grpc_api_key.clone().unwrap_or_default();
            }
            if request.draft.management_bearer_token.trim() == WEBUI_REDACTED_SECRET_PLACEHOLDER {
                request.draft.management_bearer_token = configuration
                    .vmm
                    .management_bearer_token
                    .clone()
                    .unwrap_or_default();
            }
            configuration.vmm = vmm_config_layer_from_draft(&request.draft, true)?;
            Ok(())
        },
        refresh_guard_vmm_hook_snapshot,
    )
    .await??;
    let provider_configuration = provider_configuration_from_state(state)?;
    let ai_configuration = vulcan_provider_config::config::load_public_ai_connections_file()
        .map_err(|error| format!("重新加载 VMM 内部推理配置失败：{error}"))?
        .configuration;
    let mut runtime_logs = Vec::new();
    crate::service_bootstrap::reconcile_managed_memory_mesh_runtime(
        state,
        &ai_configuration,
        &provider_configuration.vmm,
        &mut runtime_logs,
    )
    .await;
    for message in runtime_logs {
        eprintln!("{message}");
    }
    Ok(())
}

/// Validates the exact embedding and foundational LLM routes before any VMM layer is enabled.
/// 在启用任何 VMM 配置层前校验精确向量路由与基础 LLM 路由。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns the current process-local model catalogs.
/// `state` 是持有当前进程内模型目录的共享服务状态。
///
/// # Returns
/// 返回值
/// `Ok(())` when VMM inference prerequisites are complete; otherwise an actionable error.
/// VMM 推理前置条件完整时返回 `Ok(())`，否则返回可操作错误。
fn validate_vmm_required_inference(state: &SharedServiceBridgeState) -> Result<(), String> {
    let ai_configuration = vulcan_provider_config::config::load_public_ai_connections_file()
        .map_err(|error| format!("加载 VMM 内部推理配置失败：{error}"))?
        .configuration;
    {
        let guard = state
            .lock()
            .map_err(|error| format!("读取 VMM 推理依赖失败：{error}"))?;
        let runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("公共 AI Runtime 尚未发布。"))?;
        crate::configured_inference::validate_vmm_required_inference_routes(
            &runtime.public_ai_runtime,
            &ai_configuration,
        )?;
        let optional_inference_routes =
            crate::configured_inference::inspect_vmm_optional_inference_routes(
                &runtime.public_ai_runtime,
                &ai_configuration,
            );
        let catalogs = runtime.public_ai_runtime.catalog_snapshot()?;
        crate::managed_memory_mesh_runtime::configured_vmm_capability_preflight(
            &ai_configuration,
            &catalogs,
            &optional_inference_routes,
        )?;
    }
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
    let managed_runtime_settings = include_transport
        .then(|| vmm_managed_runtime_settings_from_draft(&draft.managed_runtime_settings))
        .transpose()?;
    if let (Some(settings), Some(receive_timeout_ms)) = (
        managed_runtime_settings.as_ref(),
        include_transport
            .then(|| vmm_parse_optional_u64(&draft.receive_timeout_ms, "VMM 请求超时"))
            .transpose()?
            .flatten(),
    ) {
        let (_, intent_timeout_nanos) =
            vmm_positive_go_duration_value(&settings.precheck_intent_timeout, "PreCheck 意图超时")?;
        let request_timeout_nanos = receive_timeout_ms as f64 * 1_000_000.0;
        if request_timeout_nanos <= intent_timeout_nanos {
            return Err(String::from("VMM 请求超时必须严格大于 PreCheck 意图超时。"));
        }
    }
    let layer = vulcan_vmm::VmmConfigLayer {
        backend_mode: include_transport
            .then(|| vmm_backend_mode_from_draft(&draft.backend_mode))
            .transpose()?,
        enabled: Some(draft.enabled),
        grpc_target: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_target))
            .flatten(),
        grpc_api_key: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_api_key))
            .flatten(),
        management_base_url: include_transport
            .then(|| vmm_non_empty_text(&draft.management_base_url))
            .flatten(),
        management_bearer_token: include_transport
            .then(|| vmm_non_empty_text(&draft.management_bearer_token))
            .flatten(),
        management_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.management_timeout_ms, "VMM 管理请求超时"))
            .transpose()?
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
        user_id: vmm_parse_optional_positive_u64(&draft.user_id, "VMM 用户 ID")?,
        project_id: vmm_parse_optional_positive_u64(&draft.project_id, "VMM 项目 ID")?,
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
        install: if include_transport {
            Some(vulcan_vmm::VmmInstallReservation {
                managed_install_enabled: draft.managed_install_enabled,
                managed_binary_path: vmm_non_empty_text(&draft.managed_binary_path),
                auto_start_policy: Some(draft.auto_start_policy.trim().to_string())
                    .filter(|value| !value.is_empty()),
                runtime_settings: managed_runtime_settings
                    .ok_or_else(|| String::from("保存托管 VMM 配置时缺少运行时行为设置。"))?,
            })
        } else {
            None
        },
    };
    validate_external_management_layer(&layer)?;
    Ok(layer)
}

/// Parses the explicit global backend selector without accepting ambiguous aliases.
/// 解析明确的全局后端选择器，不接受含义不明确的别名。
fn vmm_backend_mode_from_draft(value: &str) -> Result<vulcan_vmm::VmmBackendMode, String> {
    match value.trim() {
        "" | "native" => Ok(vulcan_vmm::VmmBackendMode::Native),
        "external_grpc" => Ok(vulcan_vmm::VmmBackendMode::ExternalGrpc),
        other => Err(format!(
            "VMM 后端模式无效：{other}，只允许 native 或 external_grpc"
        )),
    }
}

/// Validates the optional external management endpoint and secret as one inseparable pair.
/// 将可选外部管理端点与密钥作为不可分割的一组进行校验。
fn validate_external_management_layer(layer: &vulcan_vmm::VmmConfigLayer) -> Result<(), String> {
    let base_url = layer.management_base_url.as_deref().map(str::trim);
    let token = layer.management_bearer_token.as_deref().map(str::trim);
    match (
        base_url.filter(|value| !value.is_empty()),
        token.filter(|value| !value.is_empty()),
    ) {
        (None, None) => return Ok(()),
        (Some(_), None) => return Err(String::from("外部 VMM 管理地址已填写，但缺少管理 Token。")),
        (None, Some(_)) => {
            return Err(String::from("外部 VMM 管理 Token 已填写，但缺少管理地址。"));
        }
        (Some(_), Some(_)) => {}
    }
    let base_url = reqwest::Url::parse(base_url.unwrap_or_default())
        .map_err(|error| format!("外部 VMM 管理地址无效：{error}"))?;
    if !base_url.username().is_empty() || base_url.password().is_some() {
        return Err(String::from("外部 VMM 管理地址不能包含账号或密码。"));
    }
    if base_url.query().is_some() || base_url.fragment().is_some() {
        return Err(String::from("外部 VMM 管理地址不能包含查询参数或片段。"));
    }
    let loopback = base_url
        .host_str()
        .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    if base_url.scheme() != "https" && !(base_url.scheme() == "http" && loopback) {
        return Err(String::from("非回环 VMM 管理地址必须使用 HTTPS。"));
    }
    if layer
        .management_timeout_ms
        .is_some_and(|value| !(1_000..=120_000).contains(&value))
    {
        return Err(String::from(
            "VMM 管理请求超时必须位于 1000 到 120000 毫秒之间。",
        ));
    }
    Ok(())
}

/// Converts and validates the complete host-authoritative managed VMM behavior draft.
/// 转换并校验完整的宿主权威托管 VMM 行为草稿。
///
/// # Parameters
/// 参数
/// `draft` is the editable settings received from the WebUI.
/// `draft` 是从 WebUI 收到的可编辑设置。
///
/// # Returns
/// 返回值
/// Returns one strongly typed settings object only when every value follows the exact VMM contract.
/// 仅当每个值都符合准确的 VMM 契约时返回强类型设置对象。
fn vmm_managed_runtime_settings_from_draft(
    draft: &vulcan_service_api::VmmManagedRuntimeSettingsDraft,
) -> Result<vulcan_vmm::VmmManagedRuntimeSettings, String> {
    let precheck_search_scope = vmm_required_choice(
        &draft.precheck_search_scope,
        "PreCheck 检索范围",
        &["team", "space", "project"],
    )?;
    let postaction_input_mode = vmm_required_choice(
        &draft.postaction_input_mode,
        "PostAction 输入模式",
        &["strict", "compat"],
    )?;
    let retention_priority_floor = vmm_required_choice_case_insensitive(
        &draft.retention_priority_floor,
        "保留策略优先级下限",
        &["P0", "P1", "P2"],
    )?;
    let retention_memory_level_floor = vmm_required_choice(
        &draft.retention_memory_level_floor,
        "保留策略记忆级别下限",
        &["session", "phase", "stable", "persistent"],
    )?;
    let precheck_intent_timeout =
        vmm_positive_go_duration(&draft.precheck_intent_timeout, "PreCheck 意图超时")?;
    let postaction_idle_timeout =
        vmm_positive_go_duration(&draft.postaction_idle_timeout, "PostAction 空闲超时")?;
    let (postaction_analysis_timeout, postaction_analysis_timeout_nanos) =
        vmm_positive_go_duration_value(
            &draft.postaction_analysis_timeout,
            "PostAction 后台分析超时",
        )?;
    const TWO_MINUTES_NANOS: f64 = 2.0 * 60.0 * 1_000_000_000.0;
    if postaction_analysis_timeout_nanos < TWO_MINUTES_NANOS {
        return Err(String::from("PostAction 后台分析超时必须大于或等于 2m。"));
    }
    let retention_scan_interval =
        vmm_positive_go_duration(&draft.retention_scan_interval, "保留扫描间隔")?;
    let (retention_session_idle_after, retention_session_idle_nanos) =
        vmm_positive_go_duration_value(&draft.retention_session_idle_after, "会话空闲回收时长")?;
    let retention_trash_duration =
        vmm_positive_go_duration(&draft.retention_trash_duration, "回收站保留时长")?;
    const THREE_HUNDRED_SIXTY_HOURS_NANOS: f64 = 360.0 * 60.0 * 60.0 * 1_000_000_000.0;
    if retention_session_idle_nanos < THREE_HUNDRED_SIXTY_HOURS_NANOS {
        return Err(String::from("会话空闲回收时长必须大于或等于 360h。"));
    }

    Ok(vulcan_vmm::VmmManagedRuntimeSettings {
        pii_default_language: vmm_required_text(&draft.pii_default_language, "PII 规则语言")?,
        noise_enabled: draft.noise_enabled,
        noise_default_language: vmm_required_text(&draft.noise_default_language, "噪声规则语言")?,
        noise_semantic_enabled: draft.noise_semantic_enabled,
        noise_semantic_threshold: vmm_required_f64_inclusive(
            &draft.noise_semantic_threshold,
            "噪声语义阈值",
            0.0,
            1.0,
        )?,
        prompt_language: vmm_required_text(&draft.prompt_language, "提示词包语言")?,
        precheck_intent_timeout,
        precheck_top_k: vmm_required_u32(&draft.precheck_top_k, "PreCheck 召回数量", 1, u32::MAX)?,
        precheck_search_scope,
        precheck_similarity_threshold: vmm_required_f64_inclusive(
            &draft.precheck_similarity_threshold,
            "PreCheck 相似度阈值",
            0.0,
            1.0,
        )?,
        postaction_input_mode,
        postaction_turn_threshold: vmm_required_u32(
            &draft.postaction_turn_threshold,
            "PostAction 轮次阈值",
            1,
            u32::MAX,
        )?,
        postaction_token_threshold: vmm_required_u32(
            &draft.postaction_token_threshold,
            "PostAction Token 阈值",
            1,
            u32::MAX,
        )?,
        postaction_idle_timeout,
        postaction_history_turns: vmm_required_u32(
            &draft.postaction_history_turns,
            "PostAction 历史轮数",
            1,
            u32::MAX,
        )?,
        postaction_max_input_tokens: vmm_required_u32(
            &draft.postaction_max_input_tokens,
            "PostAction 最大输入 Token",
            1,
            u32::MAX,
        )?,
        postaction_analysis_timeout,
        postaction_failure_pass_threshold: vmm_required_u32(
            &draft.postaction_failure_pass_threshold,
            "PostAction 连续失败 Pass 阈值",
            1,
            u32::MAX,
        )?,
        postaction_max_queue_workers: vmm_required_u32(
            &draft.postaction_max_queue_workers,
            "PostAction 队列工作数",
            1,
            64,
        )?,
        inference_max_connections_per_host: vmm_required_u32(
            &draft.inference_max_connections_per_host,
            "推理主机最大连接数",
            1,
            64,
        )?,
        pipeline_max_search_keywords: vmm_required_u32(
            &draft.pipeline_max_search_keywords,
            "最大召回关键词数量",
            1,
            10,
        )?,
        pipeline_min_similarity_score: vmm_required_f64_inclusive(
            &draft.pipeline_min_similarity_score,
            "最小召回相似度",
            0.0,
            1.0,
        )?,
        pipeline_replace_min_similarity_score: vmm_required_f64_inclusive(
            &draft.pipeline_replace_min_similarity_score,
            "最小替换相似度",
            0.0,
            1.0,
        )?,
        pipeline_hard_dedupe_threshold: vmm_required_f64_inclusive(
            &draft.pipeline_hard_dedupe_threshold,
            "硬去重阈值",
            0.0,
            1.0,
        )?,
        pipeline_hard_dedupe_pool_top_k: vmm_required_u32(
            &draft.pipeline_hard_dedupe_pool_top_k,
            "硬去重候选数量",
            1,
            u32::MAX,
        )?,
        pipeline_hybrid_enabled: draft.pipeline_hybrid_enabled,
        pipeline_lexical_top_k: vmm_required_u32(
            &draft.pipeline_lexical_top_k,
            "词法召回数量",
            1,
            u32::MAX,
        )?,
        pipeline_rrf_k: vmm_required_u32(&draft.pipeline_rrf_k, "倒数排名融合常量", 1, u32::MAX)?,
        pipeline_mmr_enabled: draft.pipeline_mmr_enabled,
        pipeline_mmr_lambda: vmm_required_f64_exclusive_inclusive(
            &draft.pipeline_mmr_lambda,
            "多样性平衡系数",
            0.0,
            1.0,
        )?,
        pipeline_weibull_enabled: draft.pipeline_weibull_enabled,
        pipeline_weibull_shape: vmm_required_positive_f64(
            &draft.pipeline_weibull_shape,
            "Weibull 形状参数",
        )?,
        pipeline_weibull_scale_hours: vmm_required_positive_f64(
            &draft.pipeline_weibull_scale_hours,
            "Weibull 小时尺度",
        )?,
        pipeline_weibull_min_multiplier: vmm_required_f64_inclusive(
            &draft.pipeline_weibull_min_multiplier,
            "最小衰减倍率",
            0.0,
            1.0,
        )?,
        pipeline_weibull_reinforce_weight: vmm_required_nonnegative_f64(
            &draft.pipeline_weibull_reinforce_weight,
            "强化权重",
        )?,
        pipeline_weibull_cross_session_boost: vmm_required_nonnegative_f64(
            &draft.pipeline_weibull_cross_session_boost,
            "跨会话增强系数",
        )?,
        retention_enabled: draft.retention_enabled,
        retention_scan_interval,
        retention_turn_keep_extra_turns: vmm_required_u32(
            &draft.retention_turn_keep_extra_turns,
            "热窗口额外轮数",
            0,
            u32::MAX,
        )?,
        retention_session_idle_after,
        retention_trash_duration,
        retention_priority_floor,
        retention_memory_level_floor,
        retention_skip_protected_shared: draft.retention_skip_protected_shared,
    })
}

/// Returns one required trimmed text value.
/// 返回一个必填且已去除首尾空白的文本值。
fn vmm_required_text(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label}不能为空。"));
    }
    Ok(value.to_string())
}

/// Parses one required unsigned integer inside an inclusive range.
/// 解析一个处于闭区间内的必填无符号整数。
fn vmm_required_u32(value: &str, label: &str, min: u32, max: u32) -> Result<u32, String> {
    let parsed = value
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("{label}必须是有效整数：{error}"))?;
    if parsed < min || parsed > max {
        return Err(format!("{label}必须位于 {min} 到 {max} 之间。"));
    }
    Ok(parsed)
}

/// Parses one required finite floating-point number.
/// 解析一个必填的有限浮点数。
fn vmm_required_f64(value: &str, label: &str) -> Result<f64, String> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|error| format!("{label}必须是有效数字：{error}"))?;
    if !parsed.is_finite() {
        return Err(format!("{label}必须是有限数字。"));
    }
    Ok(parsed)
}

/// Parses one required finite floating-point number inside an inclusive range.
/// 解析一个处于闭区间内的必填有限浮点数。
fn vmm_required_f64_inclusive(value: &str, label: &str, min: f64, max: f64) -> Result<f64, String> {
    let parsed = vmm_required_f64(value, label)?;
    if parsed < min || parsed > max {
        return Err(format!("{label}必须位于 {min} 到 {max} 之间。"));
    }
    Ok(parsed)
}

/// Parses one required finite floating-point number inside a left-open, right-closed range.
/// 解析一个处于左开右闭区间内的必填有限浮点数。
fn vmm_required_f64_exclusive_inclusive(
    value: &str,
    label: &str,
    min: f64,
    max: f64,
) -> Result<f64, String> {
    let parsed = vmm_required_f64(value, label)?;
    if parsed <= min || parsed > max {
        return Err(format!("{label}必须大于 {min} 且小于或等于 {max}。"));
    }
    Ok(parsed)
}

/// Parses one required positive finite floating-point number.
/// 解析一个必填的正有限浮点数。
fn vmm_required_positive_f64(value: &str, label: &str) -> Result<f64, String> {
    let parsed = vmm_required_f64(value, label)?;
    if parsed <= 0.0 {
        return Err(format!("{label}必须大于 0。"));
    }
    Ok(parsed)
}

/// Parses one required nonnegative finite floating-point number.
/// 解析一个必填的非负有限浮点数。
fn vmm_required_nonnegative_f64(value: &str, label: &str) -> Result<f64, String> {
    let parsed = vmm_required_f64(value, label)?;
    if parsed < 0.0 {
        return Err(format!("{label}必须大于或等于 0。"));
    }
    Ok(parsed)
}

/// Validates one case-sensitive enum token.
/// 校验一个区分大小写的枚举 token。
fn vmm_required_choice(value: &str, label: &str, supported: &[&str]) -> Result<String, String> {
    let value = value.trim();
    if supported.contains(&value) {
        return Ok(value.to_string());
    }
    Err(format!("{label}仅支持：{}。", supported.join("、")))
}

/// Validates one case-insensitive enum token and returns its canonical spelling.
/// 校验一个不区分大小写的枚举 token，并返回其规范拼写。
fn vmm_required_choice_case_insensitive(
    value: &str,
    label: &str,
    supported: &[&str],
) -> Result<String, String> {
    let value = value.trim();
    supported
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(value))
        .map(|candidate| (*candidate).to_string())
        .ok_or_else(|| format!("{label}仅支持：{}。", supported.join("、")))
}

/// Validates one positive Go duration string and returns its normalized text.
/// 校验一个正的 Go 时长字符串并返回规范文本。
fn vmm_positive_go_duration(value: &str, label: &str) -> Result<String, String> {
    vmm_positive_go_duration_value(value, label).map(|(value, _)| value)
}

/// Validates one positive Go duration string and returns text plus nanoseconds.
/// 校验一个正的 Go 时长字符串并返回文本与纳秒数。
fn vmm_positive_go_duration_value(value: &str, label: &str) -> Result<(String, f64), String> {
    let value = vmm_required_text(value, label)?;
    let bytes = value.as_bytes();
    let mut index = 0usize;
    let mut total_nanos = 0.0f64;
    while index < bytes.len() {
        let number_start = index;
        let mut decimal_seen = false;
        while index < bytes.len() {
            match bytes[index] {
                b'0'..=b'9' => index += 1,
                b'.' if !decimal_seen => {
                    decimal_seen = true;
                    index += 1;
                }
                _ => break,
            }
        }
        if number_start == index || bytes[number_start..index] == *b"." {
            return Err(format!(
                "{label}必须使用 Go 时长格式，例如 5s、15m 或 360h。"
            ));
        }
        let amount = value[number_start..index]
            .parse::<f64>()
            .map_err(|error| format!("{label}包含无效数字：{error}"))?;
        let remaining = &value[index..];
        let (unit, multiplier) = if remaining.starts_with("ms") {
            ("ms", 1_000_000.0)
        } else if remaining.starts_with("us") {
            ("us", 1_000.0)
        } else if remaining.starts_with("µs") {
            ("µs", 1_000.0)
        } else if remaining.starts_with("μs") {
            ("μs", 1_000.0)
        } else if remaining.starts_with("ns") {
            ("ns", 1.0)
        } else if remaining.starts_with('h') {
            ("h", 3_600_000_000_000.0)
        } else if remaining.starts_with('m') {
            ("m", 60_000_000_000.0)
        } else if remaining.starts_with('s') {
            ("s", 1_000_000_000.0)
        } else {
            return Err(format!(
                "{label}必须使用 Go 时长单位 ns、us、µs、μs、ms、s、m 或 h。"
            ));
        };
        index += unit.len();
        total_nanos += amount * multiplier;
    }
    if !total_nanos.is_finite() || total_nanos <= 0.0 {
        return Err(format!("{label}必须大于 0。"));
    }
    Ok((value, total_nanos))
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

/// Parses an optional positive u64 identity field.
/// 解析可选的正整数 u64 身份字段。
pub(crate) fn vmm_parse_optional_positive_u64(
    value: &str,
    label: &str,
) -> Result<Option<u64>, String> {
    let parsed = vmm_parse_optional_u64(value, label)?;
    match parsed {
        Some(0) => Err(format!("{label} 必须是正整数。")),
        value => Ok(value),
    }
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
