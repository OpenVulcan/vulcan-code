#[cfg(test)]
use super::dynamic_models::*;
use super::*;

/// Converts a successful remote model refresh into a persisted provider cache map.
/// 将一次成功的远端模型刷新结果转换为持久化 Provider 缓存映射。
pub fn persist_remote_models(models: &[ModelChoice]) -> BTreeMap<String, PersistedModelEntry> {
    models
        .iter()
        .map(|model| {
            (
                model.id.clone(),
                PersistedModelEntry {
                    name: Some(model.name.clone()),
                    status: model.status.clone(),
                    family: model.family.clone(),
                    release_date: model.release_date.clone(),
                    context_limit: model.context_limit,
                    input_limit: model.input_limit,
                    output_limit: model.output_limit,
                    reasoning: model.reasoning,
                    tool_call: model.tool_call,
                    temperature: model.temperature,
                    attachment: model.attachment,
                    interleaved: model.interleaved.clone(),
                    reasoning_profile: model.reasoning_profile.clone(),
                    ai_tools: model.ai_tools.clone(),
                    input_modalities: model.input_modalities.clone(),
                    output_modalities: model.output_modalities.clone(),
                    default_enabled: model.default_enabled,
                },
            )
        })
        .collect()
}

/// Returns the first-save default enabled model identifiers declared inside one catalog.
/// 返回单个目录内部声明的首次保存默认启用模型标识集合。
pub fn default_enabled_model_ids(models: &[ModelChoice]) -> Vec<String> {
    let default_ids = models
        .iter()
        .filter(|model| model.default_enabled)
        .map(|model| model.id.clone())
        .collect::<BTreeSet<_>>();

    if !default_ids.is_empty() {
        return default_ids.into_iter().collect();
    }

    models
        .iter()
        .map(|model| model.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
/// Merges a freshly refreshed remote model catalog into one persisted provider entry while preserving explicit enablement and user overrides.
/// 在保留显式启用结果与用户覆盖的前提下，将最新远端目录刷新结果合并回持久化供应商条目。
pub fn merge_refreshed_models(
    entry: &mut PersistedProviderEntry,
    refreshed_models: &[ModelChoice],
) {
    let previous_ids = entry.models.keys().cloned().collect::<BTreeSet<_>>();
    entry.models = persist_remote_models(refreshed_models);

    let valid_ids = entry.models.keys().cloned().collect::<BTreeSet<_>>();
    if let Some(enabled_models) = entry.enabled_models.as_mut() {
        let mut next_ids = enabled_models
            .iter()
            .filter(|model_id| valid_ids.contains(model_id.as_str()))
            .cloned()
            .collect::<BTreeSet<_>>();
        for model in refreshed_models
            .iter()
            .filter(|model| model.default_enabled && !previous_ids.contains(model.id.as_str()))
        {
            next_ids.insert(model.id.clone());
        }
        *enabled_models = next_ids.into_iter().collect();
    }
    entry
        .model_overrides
        .retain(|model_id, _| valid_ids.contains(model_id));
}

/// Returns a human-friendly provider summary used in status text and logs.
/// 返回用于状态文本与日志的人类可读供应商摘要。
pub fn provider_summary(provider_id: &str) -> String {
    supported_provider_by_id(provider_id)
        .map(|definition| definition.name.clone())
        .unwrap_or_else(|| provider_id.to_string())
}

/// Returns a human-friendly model summary used in status text and logs.
/// 返回用于状态文本与日志的人类可读模型摘要。
pub fn model_summary(
    provider_id: &str,
    model_id: &str,
    config: &ProviderConfigurationFile,
) -> String {
    effective_model_choices(provider_id, config)
        .into_iter()
        .find(|model| model.id == model_id)
        .map(|model| model.to_string())
        .unwrap_or_else(|| model_id.to_string())
}

/// Loads and sorts the split provider catalog entries from the resources directory.
/// 从资源目录中加载并排序分离的供应商目录条目。
pub(super) fn load_supported_providers() -> Vec<SupportedProviderDefinition> {
    let directory = provider_catalog_directory_path();
    let mut definitions = fs::read_dir(&directory)
        .unwrap_or_else(|error| {
            panic!(
                "provider catalog directory must exist at `{}`: {error}",
                directory.display()
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "provider catalog resource `{}` must stay readable: {error}",
                    path.display()
                )
            });
            let mut definition = serde_json::from_str::<SupportedProviderDefinition>(&content)
                .unwrap_or_else(|error| {
                    panic!(
                        "provider catalog resource `{}` must stay valid: {error}",
                        path.display()
                    )
                });
            apply_provider_reasoning_model_overrides(
                definition.id.as_str(),
                &mut definition.models,
            );

            if UNSUPPORTED_PROVIDER_IDS
                .iter()
                .any(|unsupported| *unsupported == definition.id)
            {
                None
            } else {
                Some(definition)
            }
        })
        .collect::<Vec<_>>();

    definitions.sort_by_key(provider_sort_key);
    definitions
}

/// Returns the directory path containing split provider catalog files.
/// 返回包含分离供应商目录文件的目录路径。
pub(super) fn provider_catalog_directory_path() -> PathBuf {
    runtime_assets::resolve_provider_catalog_directory()
}

/// Returns a sortable provider key that keeps common providers near the top of the picker.
/// 返回可排序的供应商键值，以便将常用供应商放到选择器顶部。
pub(super) fn provider_sort_key(provider: &SupportedProviderDefinition) -> (u8, String) {
    let priority = match provider.id.as_str() {
        "openai" => 0,
        "anthropic" => 1,
        "deepseek" => 2,
        "openrouter" => 3,
        "google" => 4,
        "azure" => 5,
        _ => 99,
    };

    (priority, provider.name.clone())
}

/// Returns the token name used inside one base URL template.
/// 返回单个基址模板内部使用的参数占位符名称。
pub(super) fn template_parameter_name(field: ProviderFieldKey) -> &'static str {
    match field {
        ProviderFieldKey::ApiKey => "apiKey",
        ProviderFieldKey::BaseUrl => "baseUrl",
        ProviderFieldKey::ResourceName => "resourceName",
        ProviderFieldKey::AccountId => "accountId",
    }
}

#[cfg(test)]
/// Builds the remote `/models` endpoint URL when the provider can try a standard refresh flow.
/// 当供应商可以尝试标准刷新流程时，构建远端 `/models` 端点地址。
pub(super) fn remote_models_url(
    definition: &SupportedProviderDefinition,
    options: &PersistedProviderOptions,
) -> Option<String> {
    if !definition.supports_remote_model_refresh() {
        return None;
    }

    let base = definition.resolved_base_url(options)?;
    let normalized = base
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions")
        .trim_end_matches("/responses")
        .trim_end_matches("/completions")
        .trim_end_matches("/messages")
        .to_string();

    let endpoint = format!("{}/models", normalized.trim_end_matches('/'));

    if definition.id == "openrouter" {
        return Some(format!("{endpoint}?output_modalities=text"));
    }

    Some(endpoint)
}

#[cfg(test)]
/// Applies provider-specific default filtering to one remotely refreshed model list.
/// 对单次远端刷新得到的模型列表应用供应商级默认过滤规则。
pub(super) fn filter_remote_model_choices(
    provider_id: &str,
    choices: Vec<ModelChoice>,
) -> Vec<ModelChoice> {
    if provider_id != "openrouter" {
        return choices;
    }

    choices
        .into_iter()
        .filter(openrouter_model_choice_is_conversation_model)
        .collect()
}

#[cfg(test)]
/// Returns whether one OpenRouter model entry should stay visible in the default conversation-model picker.
/// 返回某个 OpenRouter 模型条目是否应保留在默认对话模型选择器中。
pub(super) fn openrouter_model_choice_is_conversation_model(model: &ModelChoice) -> bool {
    if openrouter_model_matches_non_conversation_keywords(model) {
        return false;
    }

    if !openrouter_model_supports_text_input(model) {
        return false;
    }

    if !openrouter_model_supports_text_only_output(model) {
        return false;
    }

    true
}

#[cfg(test)]
/// Returns whether one OpenRouter model exposes text as an accepted input modality.
/// 返回某个 OpenRouter 模型是否将文本声明为可接受输入模态。
pub(super) fn openrouter_model_supports_text_input(model: &ModelChoice) -> bool {
    if model.input_modalities.is_empty() {
        return true;
    }

    model
        .input_modalities
        .iter()
        .any(|modality| modality.trim().eq_ignore_ascii_case("text"))
}

#[cfg(test)]
/// Returns whether one OpenRouter model only emits text outputs suitable for chat-style turns.
/// 返回某个 OpenRouter 模型是否仅输出适合聊天回合的文本结果。
pub(super) fn openrouter_model_supports_text_only_output(model: &ModelChoice) -> bool {
    if model.output_modalities.is_empty() {
        return true;
    }

    model
        .output_modalities
        .iter()
        .all(|modality| modality.trim().eq_ignore_ascii_case("text"))
        && model
            .output_modalities
            .iter()
            .any(|modality| modality.trim().eq_ignore_ascii_case("text"))
}

#[cfg(test)]
/// Returns whether one OpenRouter model id/name clearly points to non-conversation tasks that should stay hidden by default.
/// 返回某个 OpenRouter 模型标识或名称是否明确指向默认应隐藏的非对话任务。
pub(super) fn openrouter_model_matches_non_conversation_keywords(model: &ModelChoice) -> bool {
    let identifier = format!("{} {}", model.id, model.name).to_ascii_lowercase();

    [
        "rerank",
        "embedding",
        "embeddings",
        "vector",
        "ocr",
        "transcribe",
        "transcription",
        "speech",
        "tts",
        "asr",
        "moderation",
    ]
    .iter()
    .any(|keyword| identifier.contains(keyword))
}

#[cfg(test)]
pub(super) fn extract_models_from_array(array: &[serde_json::Value]) -> Vec<ModelChoice> {
    array
        .iter()
        .filter_map(|item| {
            if let Some(id) = item.as_str() {
                return Some(ModelChoice {
                    id: id.to_string(),
                    name: id.to_string(),
                    status: None,
                    family: None,
                    release_date: None,
                    context_limit: None,
                    input_limit: None,
                    output_limit: None,
                    reasoning: None,
                    tool_call: None,
                    temperature: None,
                    attachment: None,
                    interleaved: None,
                    reasoning_profile: None,
                    ai_tools: Vec::new(),
                    input_modalities: Vec::new(),
                    output_modalities: Vec::new(),
                    default_enabled: false,
                });
            }

            let object = item.as_object()?;
            let id = object.get("id")?.as_str()?.to_string();
            let name = object
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&id)
                .to_string();
            let (input_modalities, output_modalities) = dynamic_model_modalities(object);

            Some(ModelChoice {
                id,
                name,
                status: dynamic_model_string(object, &["status"]),
                family: dynamic_model_string(object, &["family"]),
                release_date: dynamic_model_string(
                    object,
                    &["releaseDate", "release_date", "releasedAt", "released_at"],
                ),
                context_limit: dynamic_model_usize(
                    object,
                    &[
                        "contextLimit",
                        "context_limit",
                        "contextWindow",
                        "context_window",
                    ],
                ),
                input_limit: dynamic_model_usize(object, &["inputLimit", "input_limit"]),
                output_limit: dynamic_model_usize(object, &["outputLimit", "output_limit"]),
                reasoning: dynamic_model_bool(object, &["reasoning"]),
                tool_call: dynamic_model_bool(object, &["toolCall", "tool_call"]),
                temperature: dynamic_model_bool(object, &["temperature"]),
                attachment: dynamic_model_bool(object, &["attachment"]),
                interleaved: dynamic_model_interleaved(object, &["interleaved"]),
                reasoning_profile: dynamic_model_reasoning_profile(
                    object,
                    &["reasoningProfile", "reasoning_profile"],
                ),
                ai_tools: dynamic_model_ai_tools(object, &["aiTools", "ai_tools"]),
                input_modalities,
                output_modalities,
                default_enabled: dynamic_model_bool(object, &["defaultEnabled", "default_enabled"])
                    .unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(test)]
/// Merges embedded model metadata back into dynamically discovered model entries by stable model id.
/// 依据稳定模型标识将内置模型元数据重新合并到动态发现的模型条目中。
pub(super) fn merge_embedded_model_metadata(
    discovered_models: Vec<ModelChoice>,
    embedded_models: &[ModelChoice],
) -> Vec<ModelChoice> {
    let embedded_lookup = embedded_models
        .iter()
        .map(|model| (model.id.as_str(), model))
        .collect::<BTreeMap<_, _>>();

    discovered_models
        .into_iter()
        .map(|model| {
            let Some(embedded) = embedded_lookup.get(model.id.as_str()) else {
                return model;
            };

            ModelChoice {
                id: model.id,
                name: if model.name.trim().is_empty() {
                    embedded.name.clone()
                } else {
                    model.name
                },
                status: model.status.or_else(|| embedded.status.clone()),
                family: model.family.or_else(|| embedded.family.clone()),
                release_date: model.release_date.or_else(|| embedded.release_date.clone()),
                context_limit: model.context_limit.or(embedded.context_limit),
                input_limit: model.input_limit.or(embedded.input_limit),
                output_limit: model.output_limit.or(embedded.output_limit),
                reasoning: model.reasoning.or(embedded.reasoning),
                tool_call: model.tool_call.or(embedded.tool_call),
                temperature: model.temperature.or(embedded.temperature),
                attachment: model.attachment.or(embedded.attachment),
                interleaved: model.interleaved.or_else(|| embedded.interleaved.clone()),
                reasoning_profile: model
                    .reasoning_profile
                    .or_else(|| embedded.reasoning_profile.clone()),
                ai_tools: if model.ai_tools.is_empty() {
                    embedded.ai_tools.clone()
                } else {
                    model.ai_tools
                },
                input_modalities: if model.input_modalities.is_empty() {
                    embedded.input_modalities.clone()
                } else {
                    model.input_modalities
                },
                output_modalities: if model.output_modalities.is_empty() {
                    embedded.output_modalities.clone()
                } else {
                    model.output_modalities
                },
                default_enabled: model.default_enabled || embedded.default_enabled,
            }
        })
        .collect()
}

/// Applies persisted user override metadata onto one complete catalog while keeping untouched models identical to their discovered entries.
/// 将持久化用户覆盖元数据应用到一份完整目录上，同时保持未修改模型与发现结果一致。
pub(super) fn apply_model_overrides_to_catalog(
    models: Vec<ModelChoice>,
    overrides: &BTreeMap<String, PersistedModelOverrideEntry>,
) -> Vec<ModelChoice> {
    models
        .into_iter()
        .map(
            |model| match overrides.get(&model.id).filter(|entry| !entry.is_empty()) {
                Some(entry) => apply_model_override(model, entry),
                None => model,
            },
        )
        .collect()
}

/// Applies one persisted user override entry onto a single model choice.
/// 将单个持久化用户覆盖条目应用到一条模型选项上。
pub(super) fn apply_model_override(
    mut model: ModelChoice,
    entry: &PersistedModelOverrideEntry,
) -> ModelChoice {
    if let Some(reasoning) = entry.reasoning {
        model.reasoning = Some(reasoning);
    }
    if let Some(attachment) = entry.attachment {
        model.attachment = Some(attachment);
    }
    if let Some(context_limit) = entry.context_limit {
        model.context_limit = Some(context_limit);
    }
    if let Some(input_limit) = entry.input_limit {
        model.input_limit = Some(input_limit);
    }
    if let Some(output_limit) = entry.output_limit {
        model.output_limit = Some(output_limit);
    }
    if let Some(reasoning_profile) = entry.reasoning_profile.clone() {
        model.reasoning_profile = Some(reasoning_profile);
    }
    if let Some(input_modalities) = entry.input_modalities.clone() {
        model.input_modalities = normalize_model_override_modalities(&input_modalities);
    }
    if let Some(output_modalities) = entry.output_modalities.clone() {
        model.output_modalities = normalize_model_override_modalities(&output_modalities);
    }

    model
}

/// Normalizes one modality string list into a stable lowercase deduplicated representation.
/// 将一组模态字符串规范化为稳定的小写去重表示。
pub(super) fn normalize_model_override_modalities(values: &[String]) -> Vec<String> {
    let mut ordered = BTreeSet::new();
    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            ordered.insert(normalized);
        }
    }
    ordered.into_iter().collect()
}

/// Resolves the enabled model identifier set by honoring explicit persisted preferences first and falling back to the full catalog only for legacy entries without any saved enablement state.
/// 先尊重显式持久化偏好，再仅对没有保存启用状态的旧版条目回退到完整目录，以解析启用模型标识集合。
pub(super) fn resolved_enabled_model_ids(
    entry: Option<&PersistedProviderEntry>,
    catalog: &[ModelChoice],
) -> BTreeSet<String> {
    let catalog_ids = catalog
        .iter()
        .map(|model| model.id.clone())
        .collect::<BTreeSet<_>>();

    if let Some(explicit_ids) = entry.and_then(|item| item.enabled_models.as_ref()) {
        return explicit_ids
            .iter()
            .filter(|model_id| catalog_ids.contains(model_id.as_str()))
            .cloned()
            .collect();
    }

    catalog_ids
}

/// Builds one normalized configuration snapshot that drops deprecated remote model caches before serialization.
/// 构建一个会在序列化前丢弃废弃远端模型缓存的规范化配置快照。
pub(super) fn normalize_provider_configuration_for_save(
    config: &ProviderConfigurationFile,
) -> ProviderConfigurationFile {
    let mut normalized = config.clone();
    for entry in normalized.provider.values_mut() {
        entry.models.clear();
    }
    normalized.luaskills.normalize();
    normalized.git_commit.normalize();
    normalized
}
