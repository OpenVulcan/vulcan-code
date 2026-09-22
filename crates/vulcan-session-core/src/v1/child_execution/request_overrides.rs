use super::DomainError;
use crate::v1::config::{
    KnowledgeRetrievalMode, ProviderMediaUnderstandingModelTargetConfig,
    ProviderMediaUnderstandingReasoningEffort, ProviderToolTargetConfig, VmmSessionPolicyConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Preserves actual request overrides, including the difference between inheritance and explicit emptiness.
/// 保留真实请求覆盖，包括继承与显式空集合之间的区别。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildExecutionRequestOverrides {
    /// Optional direct tool routes; None inherits configuration, Some(empty) selects none.
    /// 可选直接工具路由；None 继承配置，Some(空集合) 表示不选择任何目标。
    pub provider_tool_selections: Option<BTreeMap<String, ProviderToolTargetConfig>>,
    /// Optional exact media routes with replacement-model reasoning preferences.
    /// 可选精确媒体路由及替代模型推理偏好。
    pub media_understanding_selections: Option<BTreeMap<String, ChildMediaUnderstandingSelection>>,
    /// Optional enabled capability set, retaining an explicit disabled-all selection.
    /// 可选启用能力集合，保留显式全部禁用选择。
    pub enabled_capabilities: Option<BTreeSet<String>>,
    /// Optional extension target sets preserving stable connection and target identities.
    /// 可选扩展目标集合，保留稳定连接与目标身份。
    pub extension_target_sets: Option<BTreeMap<String, BTreeSet<ProviderToolTargetConfig>>>,
    /// Session-level memory behavior only; transport and credentials belong to the live host.
    /// 仅包含会话级记忆行为；传输和凭据归当前宿主持有。
    pub session_vmm: VmmSessionPolicyConfig,
    /// Exact output-style text; absence means no captured style override.
    /// 精确输出风格正文；缺省表示没有捕获风格覆盖。
    pub output_style: Option<ChildOutputStyleSnapshot>,
    /// Original knowledge access ceiling, to intersect with current project authorization.
    /// 原知识访问上限，须与当前项目授权取交集。
    pub knowledge_access: Option<ChildKnowledgeAccess>,
}

/// Non-executable output-style snapshot without a process-specific fingerprint.
/// 不包含进程相关指纹的不可执行输出风格快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildOutputStyleSnapshot {
    /// Stable style identity.
    /// 稳定风格身份。
    pub id: String,
    /// Display label retained for diagnostics.
    /// 为诊断保留的显示标签。
    pub label: String,
    /// Exact normalized prompt text.
    /// 精确规范化提示词正文。
    pub text: String,
    /// Resource provenance label retained for diagnostics.
    /// 为诊断保留的资源来源标签。
    pub source_label: String,
}

/// Non-executable knowledge access captured from the original prepared child request.
/// 从原已准备子请求捕获的不可执行知识访问范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildKnowledgeAccess {
    /// Exact project owning the original authorized libraries.
    /// 拥有原已授权知识库的精确项目。
    pub project_id: String,
    /// Canonical original library set; empty means no accessible library.
    /// 规范原知识库集合；空集合表示没有可访问知识库。
    pub library_ids: BTreeSet<String>,
    /// Original optional retrieval policy.
    /// 原可选检索策略。
    pub retrieval_mode: Option<KnowledgeRetrievalMode>,
}

/// Exact non-secret media override captured from the child request.
/// 从子请求捕获的精确非敏感媒体覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChildMediaUnderstandingSelection {
    /// Invoke the captured provider tool route after current host admission.
    /// 经当前宿主准入后调用捕获的供应商工具路由。
    ProviderTool {
        /// Non-secret connection and target identifiers.
        /// 非敏感连接与目标标识。
        selection: ProviderToolTargetConfig,
    },
    /// Invoke the captured replacement model after current host admission.
    /// 经当前宿主准入后调用捕获的替代模型。
    ModelReplacement {
        /// Non-secret provider, model, and connection identifiers.
        /// 非敏感供应商、模型及连接标识。
        selection: ProviderMediaUnderstandingModelTargetConfig,
        /// Exact optional reasoning preference, including explicit disabled reasoning.
        /// 精确可选推理偏好，包括显式关闭推理。
        reasoning_effort: Option<ProviderMediaUnderstandingReasoningEffort>,
    },
}

impl ChildExecutionRequestOverrides {
    /// Validates stable nonblank route identities before persistence or restoration.
    /// 在持久化或恢复之前校验稳定非空路由身份。
    /// Returns a domain error without changing the captured request or revealing credentials.
    /// 返回领域错误，不改变捕获请求或暴露凭据。
    pub fn validate(&self) -> Result<(), DomainError> {
        if let Some(style) = &self.output_style {
            validate_names(&[&style.id])?;
            if style.text.trim() != style.text {
                return Err(DomainError::CollaborationStateViolation {
                    field: "child_execution_profile.output_style.text",
                });
            }
        }
        if let Some(access) = &self.knowledge_access {
            validate_names(&[&access.project_id])?;
            for library_id in &access.library_ids {
                validate_names(&[library_id])?;
            }
        }
        if let Some(selections) = &self.provider_tool_selections {
            for (capability, selection) in selections {
                validate_names(&[capability, &selection.connection_id, &selection.target_id])?;
            }
        }
        if let Some(capabilities) = &self.enabled_capabilities {
            for capability in capabilities {
                validate_names(&[capability])?;
            }
        }
        if let Some(selections) = &self.media_understanding_selections {
            for (capability, selection) in selections {
                validate_names(&[capability])?;
                match selection {
                    ChildMediaUnderstandingSelection::ProviderTool { selection } => {
                        validate_names(&[&selection.connection_id, &selection.target_id])?;
                    }
                    ChildMediaUnderstandingSelection::ModelReplacement { selection, .. } => {
                        validate_names(&[
                            &selection.provider_id,
                            &selection.model_id,
                            &selection.connection_id,
                        ])?;
                    }
                }
            }
        }
        if let Some(sets) = &self.extension_target_sets {
            for (capability, targets) in sets {
                validate_names(&[capability])?;
                for target in targets {
                    validate_names(&[&target.connection_id, &target.target_id])?;
                }
            }
        }
        Ok(())
    }
}

/// Checks `names` for canonical nonblank identity text; returns an explicit profile error.
/// 检查 `names` 是否为规范非空身份文本；返回明确配置记录错误。
fn validate_names(names: &[&str]) -> Result<(), DomainError> {
    if names
        .iter()
        .any(|name| name.is_empty() || name.trim() != *name)
    {
        Err(DomainError::CollaborationStateViolation {
            field: "child_execution_profile.request_overrides.identity",
        })
    } else {
        Ok(())
    }
}
