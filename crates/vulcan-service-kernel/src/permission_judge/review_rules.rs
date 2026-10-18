//! Permission reviewer rules: the system default and project replacements, applied to every
//! review the moment they are saved.
//! 权限复核器规则：系统默认值与项目替换值，保存后立即作用于每一次复核。

use std::path::Path;

use vulcan_permission_judge::system::{MAX_REVIEW_RULE_CHARS, ReviewRules};
use vulcan_provider_config::provider_config::{
    load_project_configuration_file, save_project_configuration_file,
};
use vulcan_service_api::{
    ProjectReviewRulesSaveRequest, ProjectReviewRulesSnapshot, ReviewRulesSystemDefaultSaveRequest,
    ReviewRulesSystemDefaultSnapshot,
};

use super::services::{project_directory, read_state, save_settings};
use crate::SharedServiceBridgeState;

/// Returns the saved system default rules.
/// 返回已保存的系统默认规则。
pub(crate) fn service_review_rules_system_default(
    state: SharedServiceBridgeState,
) -> Result<ReviewRulesSystemDefaultSnapshot, String> {
    let (_, settings) = read_state(&state)?;
    Ok(ReviewRulesSystemDefaultSnapshot {
        rules: settings.review_rules,
        max_chars: MAX_REVIEW_RULE_CHARS,
    })
}

/// Saves the system default rules; projects with their own rules keep them.
/// 保存系统默认规则；拥有自有规则的项目保留其规则。
pub(crate) async fn service_save_review_rules_system_default(
    state: SharedServiceBridgeState,
    request: ReviewRulesSystemDefaultSaveRequest,
) -> Result<ReviewRulesSystemDefaultSnapshot, String> {
    let rules = request.rules.normalized()?;
    let settings = save_settings(&state, |settings| {
        settings.review_rules = rules;
        Ok(())
    })
    .await?;
    Ok(ReviewRulesSystemDefaultSnapshot {
        rules: settings.review_rules,
        max_chars: MAX_REVIEW_RULE_CHARS,
    })
}

/// Projects one project's saved rules next to the system rules it would inherit.
/// 将一个项目已保存的规则与其将继承的系统规则一并投影。
fn project_snapshot(
    state: &SharedServiceBridgeState,
    project_id: String,
    rules: Option<ReviewRules>,
) -> Result<ProjectReviewRulesSnapshot, String> {
    let (_, settings) = read_state(state)?;
    Ok(ProjectReviewRulesSnapshot {
        project_id,
        rules,
        system_rules: settings.review_rules,
        max_chars: MAX_REVIEW_RULE_CHARS,
    })
}

/// Returns one project's rules.
/// 返回一个项目的规则。
pub(crate) fn service_project_review_rules(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<ProjectReviewRulesSnapshot, String> {
    let directory = project_directory(&state, &project_id)?;
    let configuration = load_project_configuration_file(&directory)?;
    project_snapshot(
        &state,
        project_id,
        configuration.permission_judge.review_rules,
    )
}

/// Saves one project's rules; `None` returns it to the system default.
/// 保存一个项目的规则；`None` 使其恢复采用系统默认值。
pub(crate) fn service_save_project_review_rules(
    state: SharedServiceBridgeState,
    request: ProjectReviewRulesSaveRequest,
) -> Result<ProjectReviewRulesSnapshot, String> {
    let rules = request
        .rules
        .as_ref()
        .map(ReviewRules::normalized)
        .transpose()?;
    let directory = project_directory(&state, &request.project_id)?;
    let mut configuration = load_project_configuration_file(&directory)?;
    configuration.permission_judge.review_rules = rules;
    save_project_configuration_file(&directory, &configuration)?;
    project_snapshot(
        &state,
        request.project_id,
        configuration.permission_judge.review_rules,
    )
}

/// The rules a review of a call in `root` applies: the project's own rules when it sets them,
/// else the system default. An unreadable project configuration is an error, because its rules
/// may forbid what the system default allows.
/// 复核 `root` 中调用时应用的规则：项目设置了自有规则时用项目规则，否则用系统默认值。项目配置无法
/// 读取时报错，因为其规则可能禁止系统默认值允许的内容。
pub(super) fn effective_review_rules(
    root: Option<&Path>,
    system: &ReviewRules,
) -> Result<ReviewRules, String> {
    let project = match root {
        Some(root) => {
            load_project_configuration_file(root)?
                .permission_judge
                .review_rules
        }
        None => None,
    };
    Ok(project.unwrap_or_else(|| system.clone()))
}
