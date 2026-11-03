//! Credential-scoped model discovery, bounded pagination, and Manifest projection.
//! 凭据作用域模型发现、有界分页与 Manifest 投影。

use super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, read_bounded_model_discovery_body,
    send_metadata_get, send_metadata_get_with_authentication,
};
use super::{
    HttpProviderMetadataClient, MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION, ModelDiscoveryError,
    ModelDiscoveryFailureKind,
};
use crate::{
    AuthenticationProjection, ConnectionModel, ConnectionModelCatalog, CredentialConnection,
    CredentialExecutionSecret, ModelDiscoveryKind, PROVIDER_MODEL_CATALOG_MAX_MODELS,
    PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES, PROVIDER_MODEL_IDENTIFIER_MAX_BYTES, ProtocolRoute,
    ProviderDescriptor, ResolvedProviderManifest, ServiceEntry, build_anthropic_model_catalog,
    build_gemini_model_catalog, parse_anthropic_model_page, parse_gemini_model_page,
    parse_google_code_assist_model_catalog, parse_openai_codex_model_catalog,
    parse_openai_model_catalog, parse_openrouter_complete_model_catalog,
};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::ModelCapabilities;

/// Maximum pages accepted from one credential-scoped remote model catalog.
/// 单次凭据作用域远程模型目录允许接受的最大分页数。
const MODEL_DISCOVERY_MAX_PAGES: u32 = 1_024;

/// Codex client version declared to the account model catalog, which hides models whose
/// server-side minimum is newer. The bundled upstream `minimal_client_version` understates that
/// minimum (GPT-6.1 Sol lists 0.153.0 yet is hidden at 0.155.0 and listed at 0.160.0), so this tracks the released
/// official client instead.
/// 向账号模型目录声明的 Codex 客户端版本；目录会隐藏服务端最低版本更新的模型。上游内置的
/// `minimal_client_version` 低于实际要求（GPT-6.1 Sol 标注 0.153.0，0.155.0 仍隐藏，0.160.0 才列出），
/// 因此跟随已发布的官方客户端版本。
const OPENAI_CODEX_DISCOVERY_CLIENT_VERSION: &str = "0.163.0";

/// Resolves the shared custom catalog URL from its entry-owned base and optional path.
/// 从入口持有的基础地址及可选路径解析共享自定义目录 URL。
/// `entry` supplies the declared catalog; returns the exact URL without route selection.
/// `entry` 提供目录声明；返回精确 URL，不选择会话路由。
pub(super) fn custom_catalog_url(entry: &ServiceEntry) -> String {
    // path uses the selected catalog's standard endpoint only when no explicit path is declared.
    // path 仅在未声明显式路径时使用所选目录的标准端点。
    let path = entry
        .model_discovery_path
        .as_deref()
        .unwrap_or_else(|| default_discovery_path(entry.model_discovery));
    format!("{}{}", entry.base_url.trim_end_matches('/'), path)
}

/// Reports whether one signed Manifest authorizes a protocol variant for an exact model scope.
/// 报告一份签名 Manifest 是否为精确模型作用域授权一个协议变体。
///
/// # Parameters
/// 参数
/// `manifest` owns binding authority; `service_entry_id`, `credential_profile_id`, and
/// `model_id` identify the exact scope; `protocol_profile_id` is the candidate route.
/// `manifest` 持有绑定权威；`service_entry_id`、`credential_profile_id` 与 `model_id`
/// 标识精确作用域；`protocol_profile_id` 是候选路由。
///
/// # Returns
/// 返回值
/// `true` only when a matching rule or an explicitly allowed default lists the protocol.
/// 仅当匹配规则或显式允许的默认值列出该协议时返回 `true`。
pub(super) fn manifest_protocol_authorizes_model_variant(
    manifest: &ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> bool {
    if let Some(rule) =
        manifest.rule_for_model(service_entry_id, Some(credential_profile_id), model_id)
    {
        return rule
            .supported_protocols
            .iter()
            .any(|protocol| protocol == protocol_profile_id);
    }
    manifest
        .model_defaults_for_entry(service_entry_id)
        .is_ok_and(|defaults| {
            defaults.unknown_model_policy
                == crate::provider_manifest::UnknownModelPolicy::DefaultBindingAllowed
                && defaults
                    .supported_protocols
                    .iter()
                    .any(|protocol| protocol == protocol_profile_id)
        })
}

impl HttpProviderMetadataClient {
    /// Fetches the entry's single selected custom catalog; never probes conversation routes.
    /// 拉取入口选定的唯一自定义目录，绝不探测会话路由。
    ///
    /// `entry` owns transport settings; `connection`, `secret`, and `generation` scope the catalog.
    /// `entry` 持有传输设置；`connection`、`secret` 和 `generation` 限定目录归属。
    /// Returns the complete bounded catalog or the original failure.
    /// 返回完整的有界目录或原始失败。
    pub(super) async fn discover_custom_catalog(
        &self,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
        generation: u64,
    ) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
        // url comes only from the shared catalog declaration, independent of enabled protocols.
        // url 仅来自共享目录声明，独立于已启用协议。
        let url = custom_catalog_url(entry);
        match entry.model_discovery {
            ModelDiscoveryKind::OpenAi => {
                // body retains the exact selected response; parser errors never select another format.
                // body 保留精确选定响应；解析错误绝不触发其他格式。
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    entry.authentication,
                    &url,
                    secret,
                    "model discovery",
                )
                .await?;
                parse_openai_model_catalog(&connection.id, generation, &body)
                    .map_err(ModelDiscoveryError::invalid_response)
            }
            ModelDiscoveryKind::AnthropicCompatible => {
                // catalog_entry receives protocol-owned version headers without changing saved settings.
                // catalog_entry 接收协议持有的版本头，不改变已保存设置。
                let mut catalog_entry = entry.clone();
                for (name, value) in vulcan_llm_core::converted_protocol_headers(
                    vulcan_llm_core::UpstreamProtocol::AnthropicMessages,
                ) {
                    if !catalog_entry
                        .request_headers
                        .keys()
                        .any(|header| header.eq_ignore_ascii_case(&name))
                    {
                        catalog_entry.request_headers.insert(name, value);
                    }
                }
                discover_anthropic_compatible_models(
                    &self.http,
                    &catalog_entry,
                    connection,
                    entry.authentication,
                    secret,
                    generation,
                    &url,
                )
                .await
            }
            ModelDiscoveryKind::Gemini => {
                discover_gemini_models(&self.http, entry, connection, secret, generation, &url)
                    .await
            }
            ModelDiscoveryKind::Manual => Ok(ConnectionModelCatalog {
                connection_id: connection.id.clone(),
                models: Vec::new(),
                generation,
                page_count: 0,
                discovery_schema_revision: String::from("manual.models.v1"),
            }),
            _ => Err(ModelDiscoveryError::failed(
                "Custom provider catalog uses an unsupported discovery protocol.",
            )),
        }
    }

    /// Discovers the raw model catalog through one exact protocol route without enrichment.
    /// 通过一条精确协议路由发现原始模型目录，不做补全。
    pub(super) async fn discover_for_route(
        &self,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
        manifest: Option<&ResolvedProviderManifest>,
        generation: u64,
        protocol_route: &ProtocolRoute,
    ) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                ModelDiscoveryError::failed(format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                ))
            })?;
        let authentication = protocol_route
            .resolved_authentication(entry.authentication, credential_profile.authentication);
        let discovery_kind = protocol_route.resolved_model_discovery(entry.model_discovery);
        let catalog = match discovery_kind {
            ModelDiscoveryKind::OpenVulcan => {
                let url = protocol_route.model_discovery_url(entry, "/api/v1/vulcancode/models");
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    &url,
                    secret,
                    "OpenVulcan model discovery",
                )
                .await?;
                crate::openvulcan::parse_catalog(&connection.id, generation, &body)
                    .map_err(ModelDiscoveryError::invalid_response)?
            }
            ModelDiscoveryKind::Manual => ConnectionModelCatalog {
                connection_id: connection.id.clone(),
                models: Vec::new(),
                generation,
                page_count: 0,
                discovery_schema_revision: String::from("manual.models.v1"),
            },
            ModelDiscoveryKind::ProviderToolsOnly => {
                return Err(ModelDiscoveryError::new(
                    ModelDiscoveryFailureKind::Unsupported,
                    format!(
                        "Provider-tools service entry `{}` does not expose a conversation model catalog.",
                        entry.id
                    ),
                ));
            }
            ModelDiscoveryKind::OpenRouter => {
                let conversation_url = protocol_route.model_discovery_url(entry, "/models/user");
                let conversation_body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    conversation_url.as_str(),
                    secret,
                    "account-visible model discovery",
                )
                .await?;
                let embedding_url = protocol_route.model_discovery_url(entry, "/embeddings/models");
                let embedding_body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    embedding_url.as_str(),
                    secret,
                    "embedding model discovery",
                )
                .await?;
                let complete_url =
                    protocol_route.model_discovery_url(entry, "/models?output_modalities=all");
                let complete_body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    complete_url.as_str(),
                    secret,
                    "complete model discovery",
                )
                .await?;
                parse_openrouter_complete_model_catalog(
                    connection.id.as_str(),
                    generation,
                    conversation_body.as_str(),
                    embedding_body.as_str(),
                    complete_body.as_str(),
                )
                .map_err(ModelDiscoveryError::invalid_response)?
            }
            ModelDiscoveryKind::ManifestStatic => build_manifest_static_catalog(
                manifest,
                connection.id.as_str(),
                entry.id.as_str(),
                connection.credential_profile_id.as_str(),
                generation,
            )?,
            ModelDiscoveryKind::OpenAi => {
                let url = protocol_route.model_discovery_url(entry, "/models");
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    url.as_str(),
                    secret,
                    "model discovery",
                )
                .await?;
                parse_openai_model_catalog(connection.id.as_str(), generation, body.as_str())
                    .map_err(ModelDiscoveryError::invalid_response)?
            }
            ModelDiscoveryKind::OpenAiCodex => {
                let path =
                    format!("/models?client_version={OPENAI_CODEX_DISCOVERY_CLIENT_VERSION}");
                let url = protocol_route.model_discovery_url(entry, path.as_str());
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    url.as_str(),
                    secret,
                    "model discovery",
                )
                .await?;
                parse_openai_codex_model_catalog(connection.id.as_str(), generation, body.as_str())
                    .map_err(ModelDiscoveryError::invalid_response)?
            }
            ModelDiscoveryKind::Anthropic => {
                discover_anthropic_models(&self.http, entry, connection, secret, generation).await?
            }
            ModelDiscoveryKind::AnthropicCompatible => {
                // url preserves the reviewed protocol-route catalog path instead of rebuilding `/models`.
                // url 保留已审核的协议路由目录路径，而不是重新拼接 `/models`。
                let url = protocol_route.model_discovery_url(entry, "/models");
                discover_anthropic_compatible_models(
                    &self.http,
                    entry,
                    connection,
                    authentication,
                    secret,
                    generation,
                    url.as_str(),
                )
                .await?
            }
            ModelDiscoveryKind::Gemini => {
                // url preserves the existing system Gemini catalog location.
                // url 保留现有系统 Gemini 目录位置。
                let url = format!("{}/models", entry.base_url.trim_end_matches('/'));
                discover_gemini_models(&self.http, entry, connection, secret, generation, &url)
                    .await?
            }
            ModelDiscoveryKind::GoogleCodeAssist => {
                discover_google_code_assist_models(
                    &self.http, entry, connection, secret, generation,
                )
                .await?
            }
        };
        // Raw network discovery ends here; manifest-driven enrichment is applied per protocol
        // route by the caller so every route in one signature group receives its own
        // capability/evidence projection instead of copying the representative route's result
        // (plan §14.2).
        // 原始网络发现到此为止；清单驱动补全由调用方按每条协议路由分别应用，使同一签名组
        // 内每条路由获得各自的能力/证据投影，而不是复制代表路由的结果（方案 §14.2）。
        Ok(catalog)
    }

    /// Applies manifest-driven enrichment to one raw discovered catalog for one exact protocol.
    /// 对一份原始已发现目录按一个精确协议应用清单驱动补全。
    pub(super) fn enrich_catalog_for_protocol(
        &self,
        manifest: Option<&ResolvedProviderManifest>,
        provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        credential_profile_id: &str,
        protocol_profile_id: &str,
        catalog: &mut ConnectionModelCatalog,
    ) -> Result<(), ModelDiscoveryError> {
        // Custom providers have no system manifest. Their user-owned manual/discovery
        // capabilities are resolved later by the custom-provider overlay, so this boundary
        // must not apply any named-provider compatibility branch.
        // 自定义供应商没有系统 Manifest。其用户持有的手工/发现能力稍后由自定义供应商
        // 覆盖解析，因此此边界绝不能应用任何具名供应商兼容分支。
        let Some(manifest) = manifest else {
            return Ok(());
        };
        // The versioned native catalog owns exact per-account capabilities; generic model-name defaults cannot widen them.
        // 版本化原生目录持有准确的逐账户能力；通用模型名称默认值不能扩大其能力。
        if entry.model_discovery == ModelDiscoveryKind::OpenVulcan {
            return Ok(());
        }
        crate::discovery::enrich_compatible_model_catalog_with_manifest(
            provider.id.as_str(),
            entry.id.as_str(),
            Some(credential_profile_id),
            protocol_profile_id,
            catalog,
            manifest,
        );
        Ok(())
    }
}

/// Builds one conservative static catalog exclusively from the active manifest snapshot.
/// 完全从当前 Manifest 快照构建一份保守静态目录。
///
/// # Parameters
/// 参数
/// `manifest` is the immutable published snapshot, `connection_id` identifies the credential,
/// `service_entry_id` selects the exact inventory, `credential_profile_id` selects its declared
/// identity scope, and `generation` freezes catalog ownership.
/// `manifest` 是不可变已发布快照，`connection_id` 标识凭据，`service_entry_id` 选择精确
/// 清单，`credential_profile_id` 选择其声明身份作用域，`generation` 冻结目录归属代数。
///
/// # Returns
/// 返回值
/// A manifest-owned catalog whose models have no code-owned capability seeds. Missing manifest
/// authority or a missing service inventory fails closed; an explicit empty inventory succeeds.
/// 返回模型不含代码持有能力种子的 Manifest 权威目录。缺少 Manifest 权威或服务清单时
/// 封闭失败；显式空清单则成功。
fn build_manifest_static_catalog(
    manifest: Option<&ResolvedProviderManifest>,
    connection_id: &str,
    service_entry_id: &str,
    credential_profile_id: &str,
    generation: u64,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    // active_manifest is the sole execution authority for this non-network catalog.
    // active_manifest 是此非网络目录的唯一执行权威。
    let active_manifest = manifest.ok_or_else(|| {
        ModelDiscoveryError::failed(format!(
            "Manifest-static discovery for service entry `{service_entry_id}` requires an active provider manifest."
        ))
    })?;
    // declared_models is the exact service-scoped inventory frozen into this snapshot.
    // declared_models 是冻结到此快照中的精确服务作用域清单。
    let declared_models = active_manifest
        .catalog_models
        .get(service_entry_id)
        .ok_or_else(|| {
            ModelDiscoveryError::failed(format!(
                "Manifest-static discovery has no model inventory for service entry `{service_entry_id}`."
            ))
        })?;
    // models intentionally start without capabilities; the exact protocol rule enriches them later.
    // models 刻意以无能力状态开始；随后由精确协议规则进行补全。
    let models = declared_models
        .iter()
        .filter(|declared| {
            declared.credential_profile_ids.is_empty()
                || declared
                    .credential_profile_ids
                    .iter()
                    .any(|profile_id| profile_id == credential_profile_id)
        })
        .map(|declared| ConnectionModel {
            id: declared.model_id.clone(),
            display_name: declared.display_name.clone(),
            capabilities: ModelCapabilities::default(),
            capability_evidence: crate::ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from(MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION),
    })
}

/// Returns the default path for `kind`, used by custom catalog requests and discovery signatures.
/// 返回 `kind` 的默认路径，用于自定义目录请求及发现签名。
/// The base URL owns its version and relay prefix; identical request URLs must share a signature.
/// 基础地址持有版本及中转前缀；相同请求 URL 必须共享签名。
pub(super) fn default_discovery_path(kind: ModelDiscoveryKind) -> &'static str {
    match kind {
        ModelDiscoveryKind::OpenVulcan => "/api/v1/vulcancode/models",
        ModelDiscoveryKind::Manual
        | ModelDiscoveryKind::ProviderToolsOnly
        | ModelDiscoveryKind::ManifestStatic => "",
        ModelDiscoveryKind::Anthropic => "/v1/models",
        ModelDiscoveryKind::OpenRouter => "/models/user",
        ModelDiscoveryKind::OpenAi
        | ModelDiscoveryKind::OpenAiCodex
        | ModelDiscoveryKind::AnthropicCompatible
        | ModelDiscoveryKind::Gemini => "/models",
        ModelDiscoveryKind::GoogleCodeAssist => "/v1/codeassist:fetchAvailableModels",
    }
}

/// Fetches the exact model map visible to one Google Code Assist credential.
/// 获取一个 Google Code Assist 凭据可见的精确模型映射。
///
/// # Parameters
/// 参数
/// `entry`, `connection`, and `secret` form the immutable credential-scoped route.
/// `entry`、`connection` 与 `secret` 组成不可变凭据作用域路由。
///
/// # Returns
/// 返回值
/// A process-memory-only model catalog.
/// 返回仅存在于进程内存的模型目录。
async fn discover_google_code_assist_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let project_id = secret.project_id().ok_or_else(|| {
        ModelDiscoveryError::failed(
            "Google Code Assist model discovery requires a validated project id.",
        )
    })?;
    let url = format!(
        "{}/v1internal:fetchAvailableModels",
        entry.base_url.trim_end_matches('/')
    );
    let mut request = http
        .post(url.as_str())
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .header("user-agent", crate::execution::vulcan_code_user_agent())
        .json(&serde_json::json!({ "project": project_id }));
    if let Some(quota_project_id) = secret.quota_project_id() {
        request = request.header("x-goog-user-project", quota_project_id);
    }
    let response = request.send().await.map_err(|error| {
        ModelDiscoveryError::new(
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
            format!("Google Code Assist model discovery failed: {error}"),
        )
    })?;
    let status = response.status();
    let body_result = read_bounded_model_discovery_body(
        response,
        "Google Code Assist model discovery",
        url.as_str(),
    )
    .await;
    if !status.is_success() {
        let diagnostic = body_result
            .as_ref()
            .map(|body| controlled_error_summary(body.as_str(), secret.expose_value()))
            .unwrap_or_else(|error| error.message.clone());
        return Err(ModelDiscoveryError::from_http_status(
            status.as_u16(),
            format!(
                "Google Code Assist model discovery returned HTTP {}: {}",
                status.as_u16(),
                diagnostic
            ),
        ));
    }
    let body = body_result?;
    parse_google_code_assist_model_catalog(connection.id.as_str(), generation, body.as_str())
        .map_err(ModelDiscoveryError::invalid_response)
}

/// Validates aggregate catalog identity and size limits after protocol parsing completes.
/// 在协议解析完成后校验目录聚合身份与大小边界。
///
/// # Parameters
/// 参数
/// `catalog` is the complete parsed result and `connection_id` is the expected credential owner.
/// `catalog` 是完整解析结果，`connection_id` 是预期凭据所有者。
///
/// # Returns
/// 返回值
/// The unchanged catalog when every bound holds, or a typed invalid-response failure.
/// 全部边界成立时返回原目录，否则返回强类型无效响应失败。
pub(super) fn validate_discovered_catalog_bounds(
    catalog: ConnectionModelCatalog,
    connection_id: &str,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    if catalog.connection_id != connection_id {
        return Err(ModelDiscoveryError::invalid_response(
            "Model discovery returned a catalog for a different credential connection.",
        ));
    }
    if catalog.page_count > MODEL_DISCOVERY_MAX_PAGES {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "Model discovery exceeded the {MODEL_DISCOVERY_MAX_PAGES}-page safety boundary."
        )));
    }
    if catalog.models.len() > PROVIDER_MODEL_CATALOG_MAX_MODELS {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "Model discovery exceeded the {PROVIDER_MODEL_CATALOG_MAX_MODELS}-model safety boundary."
        )));
    }
    // model_ids rejects duplicates in non-paginated parsers as well as malformed identities.
    // model_ids 同时拒绝非分页解析器中的重复项与格式异常标识。
    let mut model_ids = BTreeSet::new();
    for (index, model) in catalog.models.iter().enumerate() {
        if model.id.trim().is_empty()
            || model.id.len() > PROVIDER_MODEL_IDENTIFIER_MAX_BYTES
            || model.id.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "Model discovery returned an invalid model identifier at index {index}."
            )));
        }
        if !model_ids.insert(model.id.as_str()) {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "Model discovery returned a duplicate model identifier at index {index}."
            )));
        }
        if model.display_name.len() > PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES
            || model.display_name.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "Model discovery returned an invalid display name at index {index}."
            )));
        }
    }
    Ok(catalog)
}

/// Appends one discovery page while rejecting duplicate model identities across pages.
/// 追加一页发现结果，同时拒绝跨页重复的模型标识。
///
/// # Parameters
/// 参数
/// `target` owns accepted models, `seen_ids` tracks exact identities, `page` is the new page, and
/// `operation` scopes the controlled diagnostic.
/// `target` 持有已接受模型，`seen_ids` 跟踪精确标识，`page` 是新分页，`operation` 限定受控诊断。
///
/// # Returns
/// 返回值
/// Success after an atomic page append, or a duplicate-model contract error without partial append.
/// 原子追加分页后返回成功；发现重复模型时不进行部分追加并返回契约错误。
fn append_unique_discovered_models(
    target: &mut Vec<ConnectionModel>,
    seen_ids: &mut BTreeSet<String>,
    page: Vec<ConnectionModel>,
    operation: &str,
) -> Result<(), ModelDiscoveryError> {
    if target
        .len()
        .checked_add(page.len())
        .is_none_or(|length| length > PROVIDER_MODEL_CATALOG_MAX_MODELS)
    {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "{operation} exceeded the {PROVIDER_MODEL_CATALOG_MAX_MODELS}-model safety boundary."
        )));
    }
    let mut page_ids = BTreeSet::new();
    for (index, model) in page.iter().enumerate() {
        if model.id.trim().is_empty()
            || model.id.len() > PROVIDER_MODEL_IDENTIFIER_MAX_BYTES
            || model.id.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "{operation} returned an invalid model identifier at page index {index}."
            )));
        }
        if model.display_name.len() > PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES
            || model.display_name.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "{operation} returned an invalid display name at page index {index}."
            )));
        }
        if seen_ids.contains(model.id.as_str()) || !page_ids.insert(model.id.clone()) {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "{operation} returned duplicate model `{}` across pages.",
                model.id
            )));
        }
    }
    seen_ids.extend(page_ids);
    target.extend(page);
    Ok(())
}

/// Registers one next-page cursor and rejects every repeated value in the current traversal.
/// 登记一个下一页游标，并拒绝当前遍历中出现过的任意重复值。
///
/// # Parameters
/// 参数
/// `seen` owns prior cursors, `next` is the provider-returned cursor, and `operation` scopes errors.
/// `seen` 持有历史游标，`next` 是供应商返回的游标，`operation` 限定错误范围。
///
/// # Returns
/// 返回值
/// The unchanged cursor after first registration, or a deterministic loop diagnostic.
/// 首次登记时返回原游标；重复时返回确定性的循环诊断。
fn register_pagination_cursor(
    seen: &mut BTreeSet<String>,
    next: String,
    operation: &str,
) -> Result<String, ModelDiscoveryError> {
    if !seen.insert(next.clone()) {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "{operation} repeated pagination cursor `{next}`."
        )));
    }
    Ok(next)
}

/// Rejects an upstream catalog that continues beyond the explicit traversal boundary.
/// 拒绝超过明确遍历边界后仍继续的上游目录。
///
/// # Parameters
/// 参数
/// `page_count` is the number already consumed, `has_more` is authoritative, and `operation`
/// scopes the controlled diagnostic.
/// `page_count` 是已经消费的分页数，`has_more` 是权威继续标记，`operation` 限定受控诊断。
///
/// # Returns
/// 返回值
/// Success when traversal may stop or continue safely, otherwise an invalid-response failure.
/// 遍历可以安全停止或继续时返回成功，否则返回无效响应失败。
fn validate_pagination_boundary(
    page_count: u32,
    has_more: bool,
    operation: &str,
) -> Result<(), ModelDiscoveryError> {
    if has_more && page_count >= MODEL_DISCOVERY_MAX_PAGES {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "{operation} exceeded the {MODEL_DISCOVERY_MAX_PAGES}-page safety boundary."
        )));
    }
    Ok(())
}

/// Fetches every documented Anthropic model page.
/// 获取每一页文档化 Anthropic 模型。
async fn discover_anthropic_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let mut models = Vec::<ConnectionModel>::new();
    let mut model_ids = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    let mut after_id = None::<String>;
    let mut page_count = 0_u32;
    loop {
        let mut url = reqwest::Url::parse(
            format!("{}/v1/models", entry.base_url.trim_end_matches('/')).as_str(),
        )
        .map_err(|_| ModelDiscoveryError::failed("Anthropic model discovery URL is invalid."))?;
        url.query_pairs_mut().append_pair("limit", "1000");
        if let Some(cursor) = after_id.as_deref() {
            url.query_pairs_mut().append_pair("after_id", cursor);
        }
        let body = send_metadata_get(
            http,
            entry,
            url.as_str(),
            secret,
            "Anthropic model discovery",
        )
        .await?;
        let page = parse_anthropic_model_page(body.as_str(), true)
            .map_err(ModelDiscoveryError::invalid_response)?;
        page_count = page_count.saturating_add(1);
        append_unique_discovered_models(
            &mut models,
            &mut model_ids,
            page.models,
            "Anthropic model discovery",
        )?;
        validate_pagination_boundary(page_count, page.has_more, "Anthropic model discovery")?;
        if !page.has_more {
            break;
        }
        let next = page.last_id.ok_or_else(|| {
            ModelDiscoveryError::invalid_response(
                "Anthropic model discovery reported more pages without `last_id`.",
            )
        })?;
        after_id = Some(register_pagination_cursor(
            &mut seen_cursors,
            next,
            "Anthropic model discovery",
        )?);
    }
    Ok(build_anthropic_model_catalog(
        connection.id.as_str(),
        generation,
        models,
        page_count,
        "anthropic.models.v1",
    ))
}

/// Fetches every Anthropic-compatible model page relative to a versioned base URL.
/// 相对于带版本基础 URL 获取每一页 Anthropic 兼容模型。
///
/// # Parameters
/// 参数
/// `http`, `entry`, `connection`, `authentication`, and `secret` define the exact credential
/// request; `generation` identifies the in-memory revision and `discovery_url` is the reviewed
/// first-page URL.
/// `http`、`entry`、`connection`、`authentication` 与 `secret` 定义精确凭据请求；
/// `generation` 标识内存修订，`discovery_url` 是已审核的第一页地址。
///
/// # Returns
/// 返回值
/// A complete credential-scoped catalog or one typed discovery failure.
/// 返回完整的凭据作用域目录或一个强类型发现失败。
async fn discover_anthropic_compatible_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    authentication: AuthenticationProjection,
    secret: &CredentialExecutionSecret,
    generation: u64,
    discovery_url: &str,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    // models accumulates the exact provider order across bounded pages.
    // models 按有界分页累积供应商返回的精确顺序。
    let mut models = Vec::<ConnectionModel>::new();
    // model_ids rejects duplicate identities across pages.
    // model_ids 拒绝跨分页重复的模型身份。
    let mut model_ids = BTreeSet::new();
    // seen_cursors prevents an upstream cursor cycle.
    // seen_cursors 防止上游游标形成循环。
    let mut seen_cursors = BTreeSet::new();
    // after_id owns the next provider-issued pagination cursor.
    // after_id 持有下一个供应商签发的分页游标。
    let mut after_id = None::<String>;
    // page_count enforces and records the bounded traversal size.
    // page_count 强制并记录有界遍历页数。
    let mut page_count = 0_u32;
    loop {
        // url starts from the manifest-reviewed route authority on every page.
        // url 在每一页都从 Manifest 已审核的路由权限重新构建。
        let mut url = reqwest::Url::parse(discovery_url).map_err(|_| {
            ModelDiscoveryError::failed("Anthropic-compatible model discovery URL is invalid.")
        })?;
        url.query_pairs_mut().append_pair("limit", "1000");
        if let Some(cursor) = after_id.as_deref() {
            url.query_pairs_mut().append_pair("after_id", cursor);
        }
        // body is fetched with the exact service, route, and credential-profile projection.
        // body 使用精确服务、路由与凭据配置档投影获取。
        let body = send_metadata_get_with_authentication(
            http,
            entry,
            authentication,
            url.as_str(),
            secret,
            "Anthropic-compatible model discovery",
        )
        .await?;
        // page is the provider-neutral Anthropic-compatible envelope.
        // page 是供应商无关的 Anthropic 兼容响应封装。
        let page = parse_anthropic_model_page(body.as_str(), false)
            .map_err(ModelDiscoveryError::invalid_response)?;
        page_count = page_count.saturating_add(1);
        append_unique_discovered_models(
            &mut models,
            &mut model_ids,
            page.models,
            "Anthropic-compatible model discovery",
        )?;
        validate_pagination_boundary(
            page_count,
            page.has_more,
            "Anthropic-compatible model discovery",
        )?;
        if !page.has_more {
            break;
        }
        // next is mandatory whenever the provider reports another page.
        // next 在供应商报告仍有下一页时必须存在。
        let next = page.last_id.ok_or_else(|| {
            ModelDiscoveryError::invalid_response(
                "Anthropic-compatible model discovery reported more pages without `last_id`.",
            )
        })?;
        after_id = Some(register_pagination_cursor(
            &mut seen_cursors,
            next,
            "Anthropic-compatible model discovery",
        )?);
    }
    Ok(build_anthropic_model_catalog(
        connection.id.as_str(),
        generation,
        models,
        page_count,
        "anthropic-compatible.models.v1",
    ))
}

/// Fetches every documented Gemini model page.
/// 获取每一页文档化 Gemini 模型。
/// `http` sends requests using `entry` headers and `secret`; `discovery_url` fixes the catalog endpoint.
/// `http` 使用 `entry` 请求头与 `secret` 发送请求；`discovery_url` 固定目录端点。
/// `connection` and `generation` identify the returned complete catalog; pagination failures remain explicit.
/// `connection` 与 `generation` 标识返回的完整目录；分页失败会明确返回。
async fn discover_gemini_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
    discovery_url: &str,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let mut models = Vec::<ConnectionModel>::new();
    let mut model_ids = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    let mut page_token = None::<String>;
    let mut page_count = 0_u32;
    loop {
        let mut url = reqwest::Url::parse(discovery_url)
            .map_err(|_| ModelDiscoveryError::failed("Gemini model discovery URL is invalid."))?;
        url.query_pairs_mut().append_pair("pageSize", "1000");
        if let Some(token) = page_token.as_deref() {
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        let body =
            send_metadata_get(http, entry, url.as_str(), secret, "Gemini model discovery").await?;
        let page = parse_gemini_model_page(body.as_str())
            .map_err(ModelDiscoveryError::invalid_response)?;
        page_count = page_count.saturating_add(1);
        append_unique_discovered_models(
            &mut models,
            &mut model_ids,
            page.models,
            "Gemini model discovery",
        )?;
        validate_pagination_boundary(
            page_count,
            page.next_page_token
                .as_ref()
                .is_some_and(|value| !value.is_empty()),
            "Gemini model discovery",
        )?;
        let Some(next) = page.next_page_token.filter(|value| !value.is_empty()) else {
            break;
        };
        page_token = Some(register_pagination_cursor(
            &mut seen_cursors,
            next,
            "Gemini model discovery",
        )?);
    }
    Ok(build_gemini_model_catalog(
        connection.id.as_str(),
        generation,
        models,
        page_count,
    ))
}

#[cfg(test)]
mod tests;
