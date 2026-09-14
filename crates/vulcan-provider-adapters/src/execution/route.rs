//! Frozen execution route identity, endpoint resolution, and exact protocol classification.
//! 冻结执行路由身份、端点解析与精确协议归类。

use super::{ProviderExecutionFailure, pre_dispatch_failure};
use crate::{
    CredentialConnection, CustomModelRequestProjection, ProtocolRoute, ProviderDescriptor,
    RequestProjectorKind, SecondaryRouteSelection, ServiceEntry,
};
use std::collections::BTreeSet;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};
use vulcan_llm_core::UpstreamProtocol;

/// Immutable route selected before an execution starts.
/// 在执行开始前选择的不可变路由。
#[derive(Debug, Clone)]
pub struct ProviderExecutionRoute {
    /// Provider descriptor snapshot.
    /// 供应商描述快照。
    pub provider: ProviderDescriptor,
    /// Service entry snapshot.
    /// 服务入口快照。
    pub entry: ServiceEntry,
    /// Exact credential connection snapshot without secret material.
    /// 不含秘钥材料的精确凭据连接快照。
    pub connection: CredentialConnection,
    /// Exact verified protocol route.
    /// 精确的已验证协议路由。
    pub protocol_route: ProtocolRoute,
    /// Optional provider-declared secondary route.
    /// 可选供应商声明二级路由。
    pub secondary_route: SecondaryRouteSelection,
    /// Optional user-owned safe projection for one exact custom model.
    /// 可选的单个精确自定义模型用户自有安全投影。
    pub custom_model_projection: Option<CustomModelRequestProjection>,
    /// Provider-facing tool-schema projection frozen for this exact model route.
    /// 为当前精确模型路由冻结的 Provider 可见工具 Schema 投影。
    pub tool_schema_projection: crate::provider_manifest::ToolSchemaProjectionMode,
    /// Optional closed Codex behavior frozen from the model protocol binding.
    /// 从模型协议绑定冻结的可选封闭 Codex 行为。
    pub codex_behavior: Option<crate::provider_manifest::CodexModelBehavior>,
    /// Ordered request-projection profile chain effective for this exact model route; route
    /// defaults form the base and an explicitly present model-rule list replaces them, including
    /// an explicit empty list that clears the chain (plan §R8.4/§R8.5).
    /// 当前精确模型路由生效的有序请求投影 Profile 链；路由默认引用构成基座，模型规则中
    /// 显式存在的列表会替换基座，其中显式空列表会清空该链（方案 §R8.4/§R8.5）。
    pub request_projection_profiles: Vec<crate::provider_manifest::RequestProjectionProfile>,
    /// Ordered response-projection profile chain effective for this exact model route.
    /// 当前精确模型路由生效的有序响应投影 Profile 链。
    pub response_projection_profiles: Vec<crate::provider_manifest::ResponseProjectionProfile>,
    /// Upload channels frozen from the same manifest generation as this route.
    /// 与当前路由来自同一 Manifest 代际的冻结上传通道。
    pub upload_channels: Vec<crate::UploadChannelDeclaration>,
    /// Manifest revision that owns the selected model-protocol binding.
    /// 持有所选模型协议绑定的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Runtime manifest generation frozen with this route.
    /// 与当前路由一同冻结的运行时 Manifest 代际。
    pub manifest_generation: u64,
    /// Content-addressed identity of the complete effective execution route.
    /// 完整生效执行路由的内容寻址身份。
    pub route_revision: String,
}

impl ProviderExecutionRoute {
    /// Resolves an immutable execution route from registered snapshots.
    /// 从已注册快照解析不可变执行路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider`, `entry`, and `connection` are exact configuration snapshots;
    /// `secondary_route` is the user-selected provider-internal route.
    /// `provider`、`entry` 与 `connection` 是精确配置快照；`secondary_route` 是用户选择的供应商内部路由。
    ///
    /// # Returns
    /// 返回值
    /// A validated route or a controlled pre-dispatch failure.
    /// 返回已校验路由或受控的分派前失败。
    pub fn resolve(
        provider: ProviderDescriptor,
        entry: ServiceEntry,
        connection: CredentialConnection,
        secondary_route: SecondaryRouteSelection,
        effective_protocol_id: &str,
    ) -> Result<Self, ProviderExecutionFailure> {
        if provider.id != connection.provider_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not belong to provider `{}`.",
                connection.id, provider.id
            )));
        }
        if entry.id != connection.service_entry_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not use service entry `{}`.",
                connection.id, entry.id
            )));
        }
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                ))
            })?;
        if credential_profile.credential_mode != connection.credential_mode {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` credential mode does not match profile `{}`.",
                connection.id, credential_profile.id
            )));
        }
        let mut protocol_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == effective_protocol_id)
            .cloned()
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Protocol `{}` is not registered for service entry `{}`.",
                    effective_protocol_id, entry.id
                ))
            })?;
        // Manifest-declared extraction profiles are authoritative; the protocol baseline only
        // applies when the route declares no usage profile (plan §15.4).
        // Manifest 声明的提取 Profile 是权威；仅当路由未声明 usage Profile 时应用协议基线（方案 §15.4）。
        if protocol_route.usage_extraction_profiles.is_empty()
            && let Some(protocol) =
                upstream_protocol_for_profile_id(protocol_route.protocol_profile_id.as_str())
        {
            protocol_route.usage_extraction_profiles =
                vulcan_llm_core::standard_usage_profiles(protocol);
        }
        if !protocol_route.verified {
            return Err(pre_dispatch_failure(format!(
                "Protocol `{}` is not verified by service entry `{}`.",
                effective_protocol_id, entry.id
            )));
        }
        if !matches!(secondary_route, SecondaryRouteSelection::Automatic)
            && entry.request_projector != RequestProjectorKind::RoutedCompatible
        {
            return Err(pre_dispatch_failure(format!(
                "Provider `{}` does not register secondary routing.",
                provider.id
            )));
        }
        // Route default profiles are cloned before the route moves into the snapshot so the
        // model-level override chain can replace them during selection (plan §R8.4).
        // 在路由移入快照前克隆路由默认 Profile，使选择期可用模型级覆盖链替换它们（方案 §R8.4）。
        let request_projection_profiles = protocol_route.request_projection_profiles.clone();
        let response_projection_profiles = protocol_route.response_projection_profiles.clone();
        Ok(Self {
            provider,
            entry,
            connection,
            protocol_route,
            secondary_route,
            custom_model_projection: None,
            tool_schema_projection: crate::provider_manifest::ToolSchemaProjectionMode::Lightweight,
            codex_behavior: None,
            request_projection_profiles,
            response_projection_profiles,
            upload_channels: Vec::new(),
            manifest_revision: 0,
            manifest_generation: 0,
            route_revision: String::new(),
        })
    }

    /// Returns the closed reasoning-replay contract this exact protocol route declared.
    /// 返回当前精确协议路由声明的封闭推理回传契约。
    ///
    /// # Returns
    /// 返回值
    /// The frozen contract compiled from the owning manifest route, or `None` when the route
    /// declared no contract and must keep its previous request bytes.
    /// 从所属 Manifest 路由编译得到的冻结契约；路由未声明契约时必须返回 `None` 并保持既有请求字节。
    pub fn reasoning_replay_contract(
        &self,
    ) -> Option<crate::provider_manifest::ReasoningReplayContract> {
        self.protocol_route.reasoning_replay_contract
    }

    /// Returns the exact request URL for this immutable route.
    /// 返回该不可变路由的精确请求 URL。
    ///
    /// # Returns
    /// 返回值
    /// The service base URL joined with the registered endpoint path.
    /// 返回服务基础 URL 与已注册端点路径的拼接结果。
    pub fn request_url(&self, encoded_path: &str) -> String {
        format!(
            "{}{}",
            crate::connection_endpoint::resolved_connection_protocol_base_url(
                &self.connection,
                &self.entry,
                &self.protocol_route,
            )
            .trim_end_matches('/'),
            ensure_leading_slash(encoded_path)
        )
    }

    /// Returns model parameters that a frozen manifest guard conflicts with one request field.
    /// 返回冻结 Manifest 守卫中与一个请求字段冲突的模型参数。
    ///
    /// # Parameters
    /// 参数
    /// `request_field` is the registered field name without the leading `@` marker.
    /// `request_field` 是不带前导 `@` 标记的已注册字段名。
    ///
    /// # Returns
    /// 返回值
    /// Stable parameter identifiers from all-present guards that include the requested field.
    /// 返回包含目标请求字段的全部存在守卫中声明的稳定参数标识。
    pub fn model_parameters_conflicting_with_request_field(
        &self,
        request_field: &str,
    ) -> BTreeSet<String> {
        let expected_request_field = format!("@{request_field}");
        let mut conflicts = BTreeSet::new();
        for profile in &self.request_projection_profiles {
            for step in &profile.steps {
                let crate::provider_manifest::MappingStep::RejectIfAllPresent {
                    conditions, ..
                } = step
                else {
                    continue;
                };
                let guards_request_field = conditions.iter().any(|condition| {
                    condition.source == crate::provider_manifest::MappingStepSource::RequestField
                        && condition.source_path == expected_request_field
                });
                if !guards_request_field {
                    continue;
                }
                for condition in conditions {
                    if condition.source != crate::provider_manifest::MappingStepSource::RequestField
                    {
                        continue;
                    }
                    if let Some(parameter_id) = condition
                        .source_path
                        .strip_prefix("@model_parameters.")
                        .filter(|parameter_id| !parameter_id.is_empty())
                    {
                        conflicts.insert(parameter_id.to_string());
                    }
                }
            }
        }
        conflicts
    }
}

/// Resolves the provider-side protocol handled by the Responses conversion core.
/// 解析 Responses 转换核心处理的供应商侧协议。
pub(super) fn upstream_protocol_for_route(
    route: &ProviderExecutionRoute,
) -> Option<UpstreamProtocol> {
    upstream_protocol_for_profile_id(route.protocol_route.protocol_profile_id.as_str())
}

/// Reports whether one frozen execution route uses the native Codex Responses wire contract.
/// 判断冻结的执行路由是否使用原生 Codex Responses 线路契约。
///
/// # Parameters
/// 参数
/// `route` is the immutable provider route selected before execution.
/// `route` 是执行前选定的不可变供应商路由。
///
/// # Returns
/// 返回值
/// `true` only for the native Codex Responses protocol profile.
/// 仅原生 Codex Responses 协议 Profile 返回 `true`。
pub(super) fn route_uses_native_codex_responses(route: &ProviderExecutionRoute) -> bool {
    route.protocol_route.protocol_profile_id == OPENAI_CODEX_RESPONSES_PROFILE_ID
}

/// Maps one exact registered protocol profile to the conversion-core wire protocol.
/// 将一个精确注册协议 Profile 映射到转换核心 Wire 协议。
pub(crate) fn upstream_protocol_for_profile_id(profile_id: &str) -> Option<UpstreamProtocol> {
    match profile_id {
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            Some(UpstreamProtocol::OpenAiResponses)
        }
        OPENAI_CHAT_PROFILE_ID => Some(UpstreamProtocol::OpenAiChatCompletions),
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => Some(UpstreamProtocol::AnthropicMessages),
        GEMINI_GENERATE_CONTENT_PROFILE_ID => Some(UpstreamProtocol::GeminiGenerateContent),
        GEMINI_INTERACTIONS_PROFILE_ID => Some(UpstreamProtocol::GeminiInteractions),
        GOOGLE_CODE_ASSIST_PROFILE_ID => Some(UpstreamProtocol::GoogleCodeAssist),
        _ => None,
    }
}

/// Ensures one endpoint path begins with a slash.
/// 确保一个端点路径以斜杠开头。
fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

#[cfg(test)]
mod tests;
