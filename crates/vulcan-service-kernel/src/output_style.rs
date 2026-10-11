//! Output-style boundary helpers shared by submission, compaction, and runtime projection.
//! 提交、压缩与运行时投影共用的输出风格边界辅助函数。

use vulcan_agent_runtime::provider_config::PersistedPromptOutputStyleSelection;
use vulcan_prompt_composer::{
    OUTPUT_STYLE_BALANCED_ID, OutputStyleCatalog, PromptOutputStyleSnapshot,
};
use vulcan_workbench_model::SessionRuntimeStateEntry;

use crate::{
    SharedServiceBridgeState, emit_workbench_event, mutate_provider_configuration,
    refresh_settings_projection_locked,
};
use vulcan_agent_runtime::provider_config::ProviderConfigurationFile;
use vulcan_service_api::{
    OutputStyleCatalogEntry, OutputStyleCatalogSummary, PromptOutputStyleSaveRequest,
    ServiceEventEmitter,
};

/// Returns the authoritative built-in output-style catalog.
/// 返回权威的内置输出风格目录。
pub(crate) fn output_style_catalog() -> OutputStyleCatalog {
    OutputStyleCatalog::built_in()
}

/// Projects the packaged output-style catalog with the persisted global selection.
/// 将打包输出风格目录与已持久化的全局选择投影为服务 API 摘要。
pub(crate) fn output_style_catalog_summary(
    provider_configuration: &ProviderConfigurationFile,
) -> OutputStyleCatalogSummary {
    let catalog = output_style_catalog();
    let entries = catalog
        .entries()
        .into_iter()
        .map(|entry| OutputStyleCatalogEntry {
            id: entry.id.clone(),
            label_zh: entry.label_zh,
            label_en: entry.label_en,
            constraint_level: entry.constraint_level,
            prompt_char_count: entry.text.chars().count(),
            is_default: entry.id == catalog.default_id,
        })
        .collect();
    OutputStyleCatalogSummary {
        default_id: catalog.default_id,
        selected_id: provider_configuration.prompt.output_style.id().to_string(),
        entries,
    }
}

/// Returns the current global output-style catalog and selection.
/// 返回当前全局输出风格目录与选择。
pub(crate) async fn service_output_style_catalog(
    state: SharedServiceBridgeState,
) -> Result<OutputStyleCatalogSummary, String> {
    let provider_configuration = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
        .provider_configuration
        .clone();
    Ok(output_style_catalog_summary(&provider_configuration))
}

/// Saves the global output-style selection without mutating active session snapshots.
/// 保存全局输出风格选择，但不改写已有会话的活动快照。
pub(crate) async fn service_save_prompt_output_style(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PromptOutputStyleSaveRequest,
) -> Result<OutputStyleCatalogSummary, String> {
    let selection = PersistedPromptOutputStyleSelection::from_id(&request.style_id)
        .ok_or_else(|| format!("unknown output style: {}", request.style_id))?;
    let provider_configuration = mutate_provider_configuration(
        &state,
        "Failed to save the output style settings",
        |configuration| {
            configuration.prompt.output_style = selection;
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration);
            configuration.clone()
        },
    )
    .await?;
    let summary = output_style_catalog_summary(&provider_configuration);
    if let Ok(mut guard) = state.lock() {
        guard.workbench.output_style_catalog = summary.clone();
        emit_workbench_event(&app, &guard.workbench);
    }
    Ok(summary)
}

/// Resolves a submitted session selection against the packaged catalog.
/// 将提交的会话选择按打包目录解析为安全边界快照。
pub(crate) fn resolve_output_style_snapshot(
    explicit_style_id: Option<&str>,
    global_selection: PersistedPromptOutputStyleSelection,
) -> Result<PromptOutputStyleSnapshot, String> {
    let style_id = explicit_style_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| global_selection.id());
    output_style_catalog()
        .resolve(style_id)
        .map_err(|error| error.to_string())
}

/// Applies one immutable snapshot to the session runtime state at a safe boundary.
/// 在安全边界把不可变快照写入会话运行态。
pub(crate) fn apply_output_style_snapshot(
    state: &mut SessionRuntimeStateEntry,
    snapshot: &PromptOutputStyleSnapshot,
    boundary: &str,
    compaction_id: Option<String>,
    applied_at: String,
) {
    state.output_style.set_applied_snapshot(
        snapshot.id.clone(),
        snapshot.label.clone(),
        snapshot.text.clone(),
        snapshot.fingerprint,
        snapshot.source_label.clone(),
        boundary.to_string(),
        compaction_id,
        applied_at,
    );
}

/// Reconstructs the exact applied snapshot without consulting mutable catalog resources.
/// 不读取可变目录资源，直接从会话运行态重建精确的已应用快照。
pub(crate) fn applied_output_style_snapshot(
    state: &SessionRuntimeStateEntry,
) -> Option<PromptOutputStyleSnapshot> {
    let output_style = &state.output_style;
    let id = output_style.applied_style_id.clone()?;
    let text = output_style.applied_prompt_text.clone().unwrap_or_default();
    Some(PromptOutputStyleSnapshot {
        label: output_style
            .applied_label
            .clone()
            .unwrap_or_else(|| id.clone()),
        source_label: output_style
            .applied_source_label
            .clone()
            .unwrap_or_else(|| String::from("Persisted session output style")),
        fingerprint: output_style.applied_fingerprint.unwrap_or_default(),
        id,
        text,
    })
}

/// Returns the default style identifier used when older sessions have no materialized state.
/// 返回旧会话尚未物化状态时使用的默认风格标识。
pub(crate) const fn default_output_style_id() -> &'static str {
    OUTPUT_STYLE_BALANCED_ID
}
