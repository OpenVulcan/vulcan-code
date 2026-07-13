//! Git commit settings service logic.
//! Git 提交设置服务逻辑。

use serde::Deserialize;
use vulcan_agent_runtime::provider_config::{
    self, ProviderConfigurationFile, normalize_git_commit_new_branch_prefix,
};
use vulcan_provider_config::config::GitCommitSummaryModelSelection;
use vulcan_service_api::{
    GitCommitSettingsSummary, GitCommitSummaryModelSelectionSummary, ServiceEventEmitter,
    WorkbenchSnapshot,
};
use vulcan_workbench_model::SessionRuntimeStateEntry;

use crate::{
    SharedServiceBridgeState, emit_workbench_event, mutate_provider_configuration,
    refresh_settings_projection_locked,
};

/// Git commit settings save request sent by the WebUI settings page.
/// WebUI 设置页发送的 Git 提交设置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitCommitSettingsSaveRequest {
    /// Default namespace prefix used when the current branch has no namespace.
    /// 当前分支没有命名空间时使用的默认命名空间前缀。
    new_branch_prefix: String,
    /// Model selection used when commit messages are generated automatically.
    /// 自动生成提交信息时使用的模型选择。
    #[serde(default)]
    summary_model: GitCommitSummaryModelSelectionSummary,
    /// User-authored extra prompt appended to commit-message generation.
    /// 追加到提交信息生成中的用户额外提示词。
    #[serde(default)]
    summary_extra_prompt: String,
}

/// Saves Git commit dialog settings from the WebUI settings page.
/// 从 WebUI 设置页保存 Git 提交弹窗设置。
pub(crate) async fn service_save_git_commit_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: GitCommitSettingsSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let normalized_prefix = normalize_git_commit_new_branch_prefix(&request.new_branch_prefix);
    let summary_model = persisted_git_commit_summary_model_selection(request.summary_model);
    let summary_extra_prompt = request.summary_extra_prompt.trim().to_string();
    let workbench = mutate_provider_configuration(
        &state,
        "保存 Git 提交设置失败",
        |configuration| {
            validate_git_commit_summary_model_selection(&summary_model, configuration)?;
            configuration.git_commit.new_branch_prefix = normalized_prefix;
            configuration.git_commit.summary_model = summary_model;
            configuration.git_commit.summary_extra_prompt = summary_extra_prompt;
            configuration.git_commit.normalize();
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration, None);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Builds the WebUI Git commit settings snapshot from the shared provider configuration.
/// 从共享 Provider 配置构建 WebUI Git 提交设置快照。
pub(crate) fn git_commit_settings(
    provider_configuration: &ProviderConfigurationFile,
) -> GitCommitSettingsSummary {
    GitCommitSettingsSummary {
        new_branch_prefix: normalize_git_commit_new_branch_prefix(
            &provider_configuration.git_commit.new_branch_prefix,
        ),
        summary_model: api_git_commit_summary_model_selection(
            &provider_configuration.git_commit.summary_model,
        ),
        summary_extra_prompt: provider_configuration
            .git_commit
            .summary_extra_prompt
            .trim()
            .to_string(),
    }
}

/// Converts a persisted commit-summary model selection into the WebUI API shape.
/// 将持久化提交总结模型选择转换为 WebUI API 形状。
fn api_git_commit_summary_model_selection(
    selection: &GitCommitSummaryModelSelection,
) -> GitCommitSummaryModelSelectionSummary {
    match selection.normalized() {
        GitCommitSummaryModelSelection::Session => GitCommitSummaryModelSelectionSummary::Session,
        GitCommitSummaryModelSelection::Fixed {
            provider_id,
            model_id,
        } => GitCommitSummaryModelSelectionSummary::Fixed {
            provider_id,
            model_id,
        },
    }
}

/// Converts the WebUI API commit-summary model selection into the persisted shape.
/// 将 WebUI API 提交总结模型选择转换为持久化形状。
fn persisted_git_commit_summary_model_selection(
    selection: GitCommitSummaryModelSelectionSummary,
) -> GitCommitSummaryModelSelection {
    match selection {
        GitCommitSummaryModelSelectionSummary::Session => GitCommitSummaryModelSelection::Session,
        GitCommitSummaryModelSelectionSummary::Fixed {
            provider_id,
            model_id,
        } => GitCommitSummaryModelSelection::Fixed {
            provider_id,
            model_id,
        }
        .normalized(),
    }
}

/// Validates that a fixed commit-summary model exists in the current effective model list.
/// 校验固定提交总结模型存在于当前生效模型列表中。
fn validate_git_commit_summary_model_selection(
    selection: &GitCommitSummaryModelSelection,
    provider_configuration: &ProviderConfigurationFile,
) -> Result<(), String> {
    match selection.normalized() {
        GitCommitSummaryModelSelection::Session => Ok(()),
        GitCommitSummaryModelSelection::Fixed {
            provider_id,
            model_id,
        } => {
            let exists =
                provider_config::effective_model_choices(&provider_id, provider_configuration)
                    .into_iter()
                    .any(|model| model.id == model_id);
            if exists {
                Ok(())
            } else {
                Err(format!(
                    "Git 提交总结模型 `{provider_id}/{model_id}` 当前不可用，请重新选择模型。"
                ))
            }
        }
    }
}

/// Resolves the provider/model pair used for Git commit-message generation.
/// 解析 Git 提交信息生成使用的 Provider/模型组合。
pub(crate) fn resolve_git_commit_summary_model(
    selection: &GitCommitSummaryModelSelection,
    runtime_state: &SessionRuntimeStateEntry,
    provider_configuration: &ProviderConfigurationFile,
) -> Result<(String, String), String> {
    match selection.normalized() {
        GitCommitSummaryModelSelection::Session => {
            let provider_id = runtime_state
                .active_provider_id
                .as_ref()
                .ok_or_else(|| String::from("提交所属会话没有可用的当前 Provider。"))?;
            let model_id = runtime_state
                .active_model_id
                .as_ref()
                .ok_or_else(|| String::from("提交所属会话没有可用的当前模型。"))?;
            validate_git_commit_summary_model_pair(provider_id, model_id, provider_configuration)?;
            Ok((provider_id.clone(), model_id.clone()))
        }
        GitCommitSummaryModelSelection::Fixed {
            provider_id,
            model_id,
        } => {
            validate_git_commit_summary_model_pair(
                &provider_id,
                &model_id,
                provider_configuration,
            )?;
            Ok((provider_id, model_id))
        }
    }
}

/// Validates that one provider/model pair exists in the effective model choices.
/// 校验单个 Provider/模型组合存在于生效模型列表中。
fn validate_git_commit_summary_model_pair(
    provider_id: &str,
    model_id: &str,
    provider_configuration: &ProviderConfigurationFile,
) -> Result<(), String> {
    let exists = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .any(|model| model.id == model_id);
    if exists {
        Ok(())
    } else {
        Err(format!(
            "Git 提交总结模型 `{provider_id}/{model_id}` 当前不可用。"
        ))
    }
}
