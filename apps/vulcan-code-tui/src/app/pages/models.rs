// Model list page mirroring the WebUI ModelListSettingsArea: provider groups, filters and the
// conversation, multimodal-understanding and plugin-use switches of each model.
// 镜像 WebUI ModelListSettingsArea 的模型清单页面：供应商分组、筛选，以及每个模型的对话使用、
// 多模态理解与插件调用开关。

use super::*;
use crate::app::action::LocalSetting;
use api::settings_actions as settings;
use std::collections::{BTreeSet, HashMap};

/// Display order of input and output modalities, as `MODALITY_ORDER` in the WebUI.
/// 输入与输出模态的展示顺序，对应 WebUI 的 `MODALITY_ORDER`。
const MODALITY_ORDER: [&str; 7] = [
    "text",
    "image",
    "video",
    "audio",
    "file",
    "embedding",
    "rerank",
];

/// Capabilities configured through native multi-selection, as `MODEL_EXTENSION_CAPABILITIES`.
/// 通过原生多选配置的能力，对应 `MODEL_EXTENSION_CAPABILITIES`。
const MODEL_EXTENSION_CAPABILITIES: [&str; 9] = [
    "image_generate",
    "image_edit",
    "video_generate",
    "video_edit",
    "video_extend",
    "speech_synthesize",
    "speech_transcribe",
    "music_generate",
    "music_cover",
];

/// One service-entry protocol route shared by the accounts that expose it.
/// 由暴露它的账号共享的一条服务入口协议路由。
struct Route<'a> {
    service_entry_id: String,
    service_entry_name: String,
    protocol_id: String,
    binding_source: String,
    binding_rule_id: Option<String>,
    manifest_revision: i64,
    /// Sorted, deduplicated protocol identifiers.
    /// 已排序去重的协议标识。
    available_protocols: Vec<String>,
    /// Support-catalog model of the first account aggregated into this route.
    /// 聚合到此路由的首个账号的支持目录模型。
    model: &'a Value,
    /// `(display name, admitted)` per account.
    /// 每个账号的 `(显示名称, 是否准入)`。
    accounts: Vec<(String, bool)>,
}

/// One model row of a provider group.
/// 供应商分组中的一个模型行。
struct Entry<'a> {
    model_id: String,
    display_name: String,
    catalog_models: Vec<&'a Value>,
    routes: Vec<Route<'a>>,
    extension_capabilities: Vec<String>,
    /// Connections that make this model available, deduplicated across sources.
    /// 使此模型可用的连接，跨来源去重。
    available_accounts: BTreeSet<String>,
}

/// One provider with its models, in first-seen order.
/// 一个供应商及其模型，按首次出现顺序排列。
struct Group<'a> {
    provider: &'a Value,
    models: Vec<Entry<'a>>,
}

impl Group<'_> {
    fn id(&self) -> String {
        s(self.provider, "/id")
    }
}

/// Mirrors `buildProviderModelGroups`: executable backend routes grouped per model.
/// 镜像 `buildProviderModelGroups`：按模型归组的可执行后端路由。
/// `ai` is the settings snapshot and `provider` one provider descriptor.
/// `ai` 为设置快照，`provider` 为单个供应商描述。
fn protocol_routes<'a>(ai: &'a Value, provider: &'a Value) -> HashMap<String, Vec<Route<'a>>> {
    let provider_id = s(provider, "/id");
    let mut models: HashMap<String, Vec<Route<'a>>> = HashMap::new();
    for record in a(ai, "/modelRouteRecords") {
        if s(record, "/providerId") != provider_id
            || !b(record, "/executable")
            || v(record, "/protocolId").as_str().is_none()
            || v(record, "/bindingSource").as_str().is_none()
        {
            continue;
        }
        let connection_id = s(record, "/connectionId");
        let Some(connection) = a(ai, "/connections")
            .iter()
            .find(|connection| s(connection, "/id") == connection_id)
        else {
            continue;
        };
        if s(connection, "/provider_id") != provider_id
            || s(connection, "/service_entry_id") != s(record, "/serviceEntryId")
        {
            continue;
        }
        // Catalog maps are keyed by connection id, which may contain `/`.
        // 目录映射以连接标识为键，标识可能包含 `/`。
        let effective = v(ai, "/catalogs").get(&connection_id);
        let support = v(ai, "/modelSupportCatalogs")
            .get(&connection_id)
            .or(effective);
        let entry = a(provider, "/service_entries")
            .iter()
            .find(|entry| s(entry, "/id") == s(record, "/serviceEntryId"));
        let (Some(effective), Some(support), Some(entry)) = (effective, support, entry) else {
            continue;
        };
        let model_id = s(record, "/modelId");
        let Some(model) = a(support, "/models")
            .iter()
            .find(|model| s(model, "/id") == model_id)
        else {
            continue;
        };
        let admitted = a(effective, "/models")
            .iter()
            .any(|model| s(model, "/id") == model_id);
        let protocols: Vec<String> = a(record, "/availableProtocols")
            .iter()
            .filter_map(|protocol| protocol.as_str().map(str::to_string))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let rule = v(record, "/bindingRuleId").as_str().map(str::to_string);
        let account = (s(connection, "/display_name"), admitted);
        let routes = models.entry(model_id).or_default();
        match routes.iter_mut().find(|route| {
            route.service_entry_id == s(record, "/serviceEntryId")
                && route.protocol_id == s(record, "/protocolId")
                && route.binding_source == s(record, "/bindingSource")
                && route.binding_rule_id == rule
                && route.manifest_revision == n(record, "/manifestRevision")
                && route.available_protocols == protocols
        }) {
            Some(route) => route.accounts.push(account),
            None => routes.push(Route {
                service_entry_id: s(entry, "/id"),
                service_entry_name: s(entry, "/display_name"),
                protocol_id: s(record, "/protocolId"),
                binding_source: s(record, "/bindingSource"),
                binding_rule_id: rule,
                manifest_revision: n(record, "/manifestRevision"),
                available_protocols: protocols,
                model,
                accounts: vec![account],
            }),
        }
    }
    models
}

/// Mirrors `buildProviderModelInventoryGroups`, then keeps models with an available account
/// (`filterProviderModelInventoryGroupsWithAvailableAccounts`) ordered by provider priority.
/// 镜像 `buildProviderModelInventoryGroups`，再保留有可用账号的模型
/// （`filterProviderModelInventoryGroupsWithAvailableAccounts`），按供应商优先级排序。
fn inventory<'a>(ai: &'a Value) -> Vec<Group<'a>> {
    let connections = a(ai, "/connections");
    let provider_of = |connection_id: &str| {
        connections
            .iter()
            .find(|connection| s(connection, "/id") == connection_id)
            .map(|connection| s(connection, "/provider_id"))
    };
    let mut groups: Vec<Group<'a>> = a(ai, "/providers")
        .iter()
        .map(|provider| {
            let provider_id = s(provider, "/id");
            let mut routes = protocol_routes(ai, provider);
            let mut models: Vec<Entry<'a>> = Vec::new();
            // Rows are created only from catalogs and signed templates, never from routes.
            // 行只由目录与签名模板创建，绝不由路由创建。
            let mut row = |models: &mut Vec<Entry<'a>>, model_id: String, name: String| match models
                .iter()
                .position(|entry| entry.model_id == model_id)
            {
                Some(index) => index,
                None => {
                    models.push(Entry {
                        routes: routes.remove(&model_id).unwrap_or_default(),
                        model_id,
                        display_name: name,
                        catalog_models: Vec::new(),
                        extension_capabilities: Vec::new(),
                        available_accounts: BTreeSet::new(),
                    });
                    models.len() - 1
                }
            };
            if let Some(catalogs) = v(ai, "/modelSupportCatalogs").as_object() {
                for (connection_id, catalog) in catalogs {
                    if provider_of(connection_id).as_deref() != Some(provider_id.as_str()) {
                        continue;
                    }
                    let effective = v(ai, "/catalogs")
                        .get(connection_id)
                        .map(|catalog| a(catalog, "/models"))
                        .unwrap_or_default();
                    for model in a(catalog, "/models") {
                        let index = row(&mut models, s(model, "/id"), s(model, "/display_name"));
                        models[index].catalog_models.push(model);
                        if effective
                            .iter()
                            .any(|candidate| s(candidate, "/id") == s(model, "/id"))
                        {
                            models[index]
                                .available_accounts
                                .insert(connection_id.clone());
                        }
                    }
                }
            }
            for template in a(ai, "/providerToolTemplateModels") {
                let connection_id = s(template, "/connectionId");
                if s(template, "/providerId") != provider_id
                    || provider_of(&connection_id).is_none()
                {
                    continue;
                }
                let model_id = s(template, "/modelId");
                let index = row(&mut models, model_id.clone(), s(template, "/displayName"));
                // Exact targets of this account and model.
                // 此账号与模型的精确目标。
                let targets: Vec<&Value> = a(ai, "/providerToolTargets")
                    .iter()
                    .filter(|target| {
                        s(target, "/connectionId") == connection_id
                            && s(target, "/target/provider_id") == provider_id
                            && s(target, "/target/upstream_target_id") == model_id
                    })
                    .collect();
                // Older templates without capabilities take them from their available targets.
                // 不含能力的旧模板从其可用目标取得能力。
                let capabilities: Vec<String> = match v(template, "/capabilities").as_array() {
                    Some(list) => list
                        .iter()
                        .filter_map(|capability| capability.as_str().map(str::to_string))
                        .collect(),
                    None => targets
                        .iter()
                        .map(|target| s(target, "/target/contract/capability"))
                        .collect(),
                };
                let entry = &mut models[index];
                for capability in &capabilities {
                    if !entry.extension_capabilities.contains(capability) {
                        entry.extension_capabilities.push(capability.clone());
                    }
                }
                if targets
                    .iter()
                    .any(|target| capabilities.contains(&s(target, "/target/contract/capability")))
                {
                    entry.available_accounts.insert(connection_id);
                }
            }
            models.retain(|entry| !entry.available_accounts.is_empty());
            Group { provider, models }
        })
        .filter(|group| !group.models.is_empty())
        .collect();
    // A stable sort keeps registry order between equal priorities.
    // 稳定排序使同优先级保持注册表顺序。
    let priority = |group: &Group<'_>| {
        v(ai, "/providerPriorities")
            .get(group.id())
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    groups.sort_by_key(|group| std::cmp::Reverse(priority(group)));
    groups
}

/// Number of models the list offers across all providers, for the page badge.
/// 清单在全部供应商中提供的模型数量，用于页面计数。
pub(super) fn model_count(app: &App) -> usize {
    inventory(app.slots.value(Slot::PublicAiSettings))
        .iter()
        .map(|group| group.models.len())
        .sum()
}

/// `providerModelModalities`: input and output modalities declared by one catalog model.
/// `providerModelModalities`：单个目录模型声明的输入与输出模态。
fn modalities(model: &Value) -> (BTreeSet<&'static str>, BTreeSet<&'static str>) {
    let operations = a(model, "/capabilities/operations");
    let has = |operation: &str| operations.iter().any(|value| value == operation);
    let mut input = BTreeSet::new();
    let mut output = BTreeSet::new();
    let known = |value: &Value| MODALITY_ORDER.into_iter().find(|kind| value == *kind);
    if !operations.is_empty() {
        input.insert("text");
    }
    input.extend(
        a(model, "/capabilities/input_resources")
            .iter()
            .filter_map(known),
    );
    if has("conversation_respond") {
        output.insert("text");
    }
    if has("embedding_create") || !v(model, "/capabilities/embedding").is_null() {
        output.insert("embedding");
    }
    if has("rerank_documents") || !v(model, "/capabilities/rerank").is_null() {
        output.insert("rerank");
    }
    output.extend(
        a(model, "/capabilities/output_resources")
            .iter()
            .filter_map(known),
    );
    (input, output)
}

/// `providerExtensionOutputModalities`: the output a signed extension operation produces.
/// `providerExtensionOutputModalities`：签名扩展操作产生的输出。
fn extension_output(capability: &str) -> Option<&'static str> {
    match capability {
        "image_generate" | "image_edit" => Some("image"),
        "video_generate" | "video_edit" | "video_extend" => Some("video"),
        "speech_synthesize" | "music_generate" | "music_cover" => Some("audio"),
        "speech_transcribe" => Some("text"),
        _ => None,
    }
}

/// Every output modality of a row: catalog models plus signed extensions.
/// 一行的全部输出模态：目录模型加签名扩展。
fn outputs(entry: &Entry<'_>) -> BTreeSet<&'static str> {
    let mut output: BTreeSet<_> = entry
        .catalog_models
        .iter()
        .flat_map(|model| modalities(model).1)
        .collect();
    output.extend(
        entry
            .extension_capabilities
            .iter()
            .filter_map(|capability| extension_output(capability)),
    );
    output
}

fn modality_label(modality: &str) -> String {
    match modality {
        "text" => tr!("settings.models.modality.text"),
        "image" => tr!("settings.models.modality.image"),
        "video" => tr!("settings.models.modality.video"),
        "audio" => tr!("settings.models.modality.audio"),
        "file" => tr!("settings.models.modality.file"),
        "embedding" => tr!("settings.models.modality.embedding"),
        "rerank" => tr!("settings.models.modality.rerank"),
        other => other.to_string(),
    }
}

/// Ordered, localized modality list joined with the locale's list delimiter.
/// 按顺序本地化并以语言列表分隔符连接的模态列表。
fn modality_list(kinds: &BTreeSet<&str>) -> String {
    MODALITY_ORDER
        .into_iter()
        .filter(|kind| kinds.contains(kind))
        .map(modality_label)
        .collect::<Vec<_>>()
        .join(&tr!("global.listDelimiter"))
}

/// `ProviderModelModalitySummary` as text: `Input …  · Output …`, empty when nothing is declared.
/// 文字形式的 `ProviderModelModalitySummary`：`输入 … · 输出 …`，未声明时为空。
fn modality_summary(entry: &Entry<'_>) -> String {
    let mut input: BTreeSet<&str> = entry
        .catalog_models
        .iter()
        .flat_map(|model| modalities(model).0)
        .collect();
    for capability in &entry.extension_capabilities {
        match capability.as_str() {
            "image_understanding" => input.insert("image"),
            "video_understanding" => input.insert("video"),
            "audio_understanding" => input.insert("audio"),
            _ => false,
        };
    }
    let output = outputs(entry);
    let mut parts = Vec::new();
    if !input.is_empty() {
        parts.push(format!(
            "{} {}",
            tr!("settings.models.inputModalities"),
            modality_list(&input)
        ));
    }
    if !output.is_empty() {
        parts.push(format!(
            "{} {}",
            tr!("settings.models.outputModalities"),
            modality_list(&output)
        ));
    }
    parts.join(" · ")
}

/// `localizedProviderName`.
fn provider_name(provider: &Value) -> String {
    match s(provider, "/id").as_str() {
        "tokenrhythm" => tr!("settings.providerName.tokenrhythm"),
        _ => s(provider, "/display_name"),
    }
}

/// `extensionCapabilityLabel`.
fn extension_label(capability: &str) -> String {
    match capability {
        "image_generate" => tr!("settings.providerTools.imageGenerate"),
        "image_edit" => tr!("settings.providerTools.imageEdit"),
        "video_generate" => tr!("settings.providerTools.videoGenerate"),
        "video_edit" => tr!("settings.providerTools.videoEdit"),
        "video_extend" => tr!("settings.providerTools.videoExtend"),
        "speech_synthesize" => tr!("settings.providerTools.speechSynthesize"),
        "speech_transcribe" => tr!("settings.providerTools.speechTranscribe"),
        "music_generate" => tr!("settings.providerTools.musicGenerate"),
        "music_cover" => tr!("settings.providerTools.musicCover"),
        other => other.to_string(),
    }
}

/// `protocolDisplayName`.
fn protocol_name(protocol: &str) -> String {
    match protocol {
        "anthropic-messages.v1" => "Anthropic",
        "anthropic-messages-video.v1" => "Anthropic + Video",
        "openai-chat-completions.v1" => "OpenAI Chat",
        "openai-codex-responses.v1" => "Codex Responses",
        "openai-responses.v1" => "OpenAI Responses",
        other => other,
    }
    .to_string()
}

/// `internalOperationLabel`.
fn operation_label(operation: &str) -> String {
    match operation {
        "conversation_respond" => tr!("settings.inference.llm"),
        "embedding_create" => tr!("settings.inference.embedding"),
        "rerank_documents" => tr!("settings.inference.rerank"),
        other => other.to_string(),
    }
}

/// `internalStateForModel`: the shared internal-use switch and its operations, if any route
/// offers one.
/// `internalStateForModel`：共享的内部使用开关及其操作，仅在有路由提供时存在。
fn internal_state(ai: &Value, provider_id: &str, model_id: &str) -> Option<(bool, Vec<String>)> {
    let mut operations: Vec<String> = Vec::new();
    let mut enabled = false;
    for candidate in a(ai, "/internalInferenceCandidates") {
        if s(candidate, "/route/provider_id") == provider_id
            && s(candidate, "/route/model_id") == model_id
        {
            let operation = s(candidate, "/route/operation");
            if !operations.contains(&operation) {
                operations.push(operation);
            }
            enabled |= b(candidate, "/enabled");
        }
    }
    (!operations.is_empty()).then_some((enabled, operations))
}

/// `extensionModelTargets`: signed, non-custom targets of one provider model and capability.
/// `extensionModelTargets`：某供应商模型与能力的签名非自定义目标。
fn extension_targets<'a>(
    ai: &'a Value,
    provider_id: &str,
    model_id: &str,
    capability: &str,
) -> Vec<&'a Value> {
    a(ai, "/providerToolTargets")
        .iter()
        .filter(|target| {
            s(target, "/target/provider_id") == provider_id
                && s(target, "/target/upstream_target_id") == model_id
                && s(target, "/target/contract/capability") == capability
                && s(target, "/manifestSourceLayer") != "custom"
        })
        .collect()
}

/// Whether a saved `{connection_id, target_id}` selection names one of `targets`.
/// 已保存的 `{connection_id, target_id}` 选择是否指向 `targets` 之一。
fn selects(selection: &Value, targets: &[&Value]) -> bool {
    targets.iter().any(|target| {
        s(target, "/connectionId") == s(selection, "/connection_id")
            && s(target, "/target/target_id") == s(selection, "/target_id")
    })
}

/// Saved extension selections of one capability.
/// 某能力已保存的扩展选择。
fn extension_set<'a>(ai: &'a Value, capability: &str) -> &'a [Value] {
    v(ai, "/providerExtensionTargetSets")
        .get(capability)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn has(list: &[Value], value: &str) -> bool {
    list.iter().any(|item| item == value)
}

/// Whether the provider model is a multimodal-understanding candidate.
/// 该供应商模型是否为多模态理解候选。
fn multimodal_admitted(ai: &Value, provider_id: &str, model_id: &str) -> bool {
    a(ai, "/multimodalUnderstandingCandidateModels")
        .iter()
        .any(|record| s(record, "/providerId") == provider_id && s(record, "/modelId") == model_id)
}

/// `access?.enabled ?? true` from modelAccessRecords.
/// 来自 modelAccessRecords 的 `access?.enabled ?? true`。
fn conversation_enabled(ai: &Value, provider_id: &str, model_id: &str) -> bool {
    a(ai, "/modelAccessRecords")
        .iter()
        .find(|record| s(record, "/providerId") == provider_id && s(record, "/modelId") == model_id)
        .is_none_or(|record| b(record, "/enabled"))
}

fn conversation_eligible(entry: &Entry<'_>) -> bool {
    entry.routes.iter().any(|route| {
        has(
            a(route.model, "/capabilities/operations"),
            "conversation_respond",
        )
    })
}

/// The usage and output filters of `visibleModelRows`, the search is applied by the caller.
/// `visibleModelRows` 的用途与输出筛选；搜索由调用方应用。
fn visible(app: &App, ai: &Value, provider_id: &str, entry: &Entry<'_>) -> bool {
    if app.model_output != "all" && !outputs(entry).contains(app.model_output.as_str()) {
        return false;
    }
    match app.model_usage.as_str() {
        "conversation" => {
            conversation_enabled(ai, provider_id, &entry.model_id) && conversation_eligible(entry)
        }
        "internal" => {
            internal_state(ai, provider_id, &entry.model_id).is_some_and(|(on, _)| on)
                || entry.extension_capabilities.iter().any(|capability| {
                    has(a(ai, "/providerToolEnabledCapabilities"), capability)
                        && extension_set(ai, capability).iter().any(|selection| {
                            a(ai, "/providerToolTargets").iter().any(|target| {
                                s(target, "/connectionId") == s(selection, "/connection_id")
                                    && s(target, "/target/target_id") == s(selection, "/target_id")
                                    && s(target, "/target/provider_id") == provider_id
                                    && s(target, "/target/upstream_target_id") == entry.model_id
                            })
                        })
                })
        }
        "multimodal" => multimodal_admitted(ai, provider_id, &entry.model_id),
        _ => true,
    }
}

/// `filterProviderModels` search: trimmed, case-insensitive name or id containment.
/// `filterProviderModels` 搜索：去除首尾空白后按名称或标识不区分大小写包含匹配。
fn matches_search(entry: &Entry<'_>, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || entry.display_name.to_lowercase().contains(&query)
        || entry.model_id.to_lowercase().contains(&query)
}

/// The switch cells of one model, in the WebUI column order: conversation use, multimodal
/// understanding, then internal use and each signed extension; plus the access button.
/// 单个模型的开关单元格，按 WebUI 列顺序：对话使用、多模态理解，然后为内部使用与各签名扩展；
/// 另加接入按钮。
fn cells(ai: &Value, provider: &Value, entry: &Entry<'_>) -> Vec<RowAction> {
    let provider_id = s(provider, "/id");
    let model_id = entry.model_id.as_str();
    let mut cells = Vec::new();
    let label = tr!("settings.models.conversationUse");
    if conversation_eligible(entry) {
        let on = conversation_enabled(ai, &provider_id, model_id);
        cells.push(switch(
            label,
            on,
            Some(call(settings::save_public_ai_model_enabled(
                &provider_id,
                model_id,
                !on,
            ))),
        ));
    } else {
        cells.push(absent(label));
    }
    // Eligibility is backend-resolved; a stale admission can still be switched off.
    // 资格由后端解析；失效的准入仍可关闭。
    let label = tr!("settings.modelExperience.understandingUse");
    let eligible = a(ai, "/mediaUnderstandingModelCandidates")
        .iter()
        .any(|candidate| {
            s(candidate, "/selection/provider_id") == provider_id
                && s(candidate, "/selection/model_id") == model_id
        });
    let admitted = multimodal_admitted(ai, &provider_id, model_id);
    if eligible || admitted {
        cells.push(switch(
            label,
            admitted,
            Some(call(
                settings::save_public_ai_multimodal_understanding_candidate(
                    &provider_id,
                    model_id,
                    !admitted,
                ),
            )),
        ));
    } else {
        cells.push(absent(label));
    }
    let internal = internal_state(ai, &provider_id, model_id);
    if let Some((on, _)) = internal {
        cells.push(switch(
            tr!("settings.models.internalUse"),
            on,
            Some(call(settings::save_public_ai_internal_model_enabled(
                &provider_id,
                model_id,
                !on,
            ))),
        ));
    }
    let extensions: Vec<&String> = entry
        .extension_capabilities
        .iter()
        .filter(|capability| MODEL_EXTENSION_CAPABILITIES.contains(&capability.as_str()))
        .collect();
    for capability in &extensions {
        let targets = extension_targets(ai, &provider_id, model_id, capability);
        let selections = extension_set(ai, capability);
        let selected = selections
            .iter()
            .any(|selection| selects(selection, &targets));
        // Only a target carrying the reviewed extension profile may be newly selected.
        // 只有带已审核扩展资料的目标才能被新选中。
        let available = targets
            .iter()
            .find(|target| !v(target, "/target/contract/extension_profile").is_null());
        let action = (available.is_some() || selected).then(|| {
            // The rest of the capability's complete target set is kept as is.
            // 该能力完整目标集合中的其余选择保持不变。
            let mut retained: Vec<Value> = selections
                .iter()
                .filter(|selection| !selects(selection, &targets))
                .cloned()
                .collect();
            if !selected && let Some(target) = available {
                retained.push(json!({"connection_id":v(target,"/connectionId"),
                    "target_id":v(target,"/target/target_id")}));
            }
            let save = settings::save_public_ai_provider_extension_target_set(capability, retained);
            // Checking a model also admits its capability when that is still off.
            // 勾选模型时，若其能力仍关闭，则一并启用该能力。
            if !selected && !has(a(ai, "/providerToolEnabledCapabilities"), capability) {
                call(
                    save.then(settings::save_public_ai_provider_tool_capability_enabled(
                        capability, true,
                    )),
                )
            } else {
                call(save)
            }
        });
        cells.push(switch(extension_label(capability), selected, action));
    }
    if internal.is_none() && extensions.is_empty() {
        cells.push(absent(tr!("settings.models.internalUse")));
    }
    cells.push(action(
        tr!("settings.modelExperience.accessButton"),
        access_sheet(entry, internal.map(|(_, operations)| operations)),
        Tone::Normal,
    ));
    cells
}

/// The access dialog: each protocol route with its binding and accounts, or the native
/// inventory summary when no protocol route exists.
/// 接入对话框：每条协议路由及其绑定与账号；没有协议路由时为原生目录摘要。
fn access_sheet(entry: &Entry<'_>, internal: Option<Vec<String>>) -> UiAction {
    let credentials = tr!(
        "settings.models.availableCredentials",
        count = entry.available_accounts.len()
    );
    let mut sheet = super::super::overlay::DetailSheet::new(entry.display_name.clone()).field(
        tr!("settings.modelExperience.modelColumn"),
        entry.model_id.clone(),
    );
    if let Some(operations) = internal {
        sheet = sheet.field(
            tr!("settings.models.internalUse"),
            operations
                .iter()
                .map(|operation| operation_label(operation))
                .collect::<Vec<_>>()
                .join(&tr!("global.listDelimiter")),
        );
    }
    if entry.routes.is_empty() {
        return UiAction::Detail(Box::new(
            sheet.section(
                tr!("settings.modelExperience.nativeInventory"),
                [modality_summary(entry), credentials]
                    .into_iter()
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ));
    }
    sheet = sheet.field(
        tr!("settings.modelExperience.accessButton"),
        tr!(
            "settings.modelExperience.routes",
            count = entry.routes.len()
        ),
    );
    for route in &entry.routes {
        let mut body = vec![
            format!(
                "{}: {} · {} · r{}",
                tr!("settings.modelExperience.bindingSource"),
                route.binding_source,
                route
                    .binding_rule_id
                    .clone()
                    .unwrap_or_else(|| tr!("settings.models.routeNoRule")),
                route.manifest_revision
            ),
            format!(
                "{}: {}",
                tr!("settings.models.routeAvailableProtocols"),
                if route.available_protocols.is_empty() {
                    "—".to_string()
                } else {
                    route.available_protocols.join(", ")
                }
            ),
            format!("{}:", tr!("settings.modelExperience.routeAccounts")),
        ];
        for (name, admitted) in &route.accounts {
            body.push(format!(
                "  {name} · {}",
                if *admitted {
                    tr!("settings.modelExperience.available")
                } else {
                    tr!("settings.modelExperience.accountNotAdmitted")
                }
            ));
        }
        sheet = sheet.section(
            format!(
                "{} · {}",
                route.service_entry_name,
                protocol_name(&route.protocol_id)
            ),
            body.join("\n"),
        );
    }
    UiAction::Detail(Box::new(sheet))
}

/// `codexBehaviorSummary`.
fn codex_behavior(record: &Value) -> String {
    [
        "wire_dialect",
        "apply_patch_tool_type",
        "shell_type",
        "multi_agent_version",
    ]
    .into_iter()
    .map(|key| s(record, &format!("/codexBehavior/{key}")))
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

/// Settings › Models, as the WebUI model list.
/// 设置 › 模型，对应 WebUI 模型清单。
pub(super) fn models(app: &App) -> Vec<Row> {
    let ai = app.slots.value(Slot::PublicAiSettings);
    if ai.is_null() {
        return Vec::new();
    }
    let groups = inventory(ai);
    let mut rows = Vec::new();
    let selected = groups
        .iter()
        .find(|group| group.id() == app.model_provider)
        .or(groups.first());
    rows.push(choice(
        tr!("settings.models.provider"),
        groups
            .iter()
            .map(|group| {
                (
                    group.id(),
                    format!("{}  {}", provider_name(group.provider), group.models.len()),
                )
            })
            .collect(),
        selected.map(Group::id).unwrap_or_default(),
        Edit::Local(LocalSetting::ModelProvider),
    ));
    // Admissions whose model left every group keep a way to be switched off.
    // 模型已离开全部分组的准入保留关闭入口。
    let stale: Vec<&Value> = a(ai, "/multimodalUnderstandingCandidateModels")
        .iter()
        .filter(|record| {
            !groups.iter().any(|group| {
                group.id() == s(record, "/providerId")
                    && group
                        .models
                        .iter()
                        .any(|entry| entry.model_id == s(record, "/modelId"))
            })
        })
        .collect();
    if !stale.is_empty() {
        rows.push(section(tr!(
            "settings.modelExperience.unavailableCandidates"
        )));
        for record in stale {
            let (provider_id, model_id) = (s(record, "/providerId"), s(record, "/modelId"));
            rows.push(
                item(
                    &format!("stale-candidate:{provider_id}/{model_id}"),
                    format!("{provider_id} · {model_id}"),
                    "",
                )
                .actions(vec![action(
                    tr!("settings.modelExperience.disableCandidate"),
                    call(settings::save_public_ai_multimodal_understanding_candidate(
                        &provider_id,
                        &model_id,
                        false,
                    )),
                    Tone::Normal,
                )]),
            );
        }
    }
    let Some(group) = selected else {
        rows.push(note(tr!("settings.models.emptyCatalog"), Tone::Muted));
        return rows;
    };
    let provider_id = group.id();
    let blocked: Vec<&Value> = a(ai, "/modelRouteRecords")
        .iter()
        .filter(|record| s(record, "/providerId") == provider_id && !b(record, "/executable"))
        .collect();
    if !blocked.is_empty() {
        rows.push(section(tr!("settings.models.blockedRouteDiagnostics")));
        for record in blocked {
            let mut detail = Vec::new();
            let behavior = codex_behavior(record);
            if !behavior.is_empty() {
                detail.push(format!(
                    "{}: {behavior}",
                    tr!("settings.models.codexBehavior")
                ));
            }
            if let Some(primitive) = v(record, "/requiredRuntimePrimitive").as_str() {
                detail.push(format!(
                    "{}: {primitive}",
                    tr!("settings.models.requiredRuntimePrimitive")
                ));
            }
            detail.push(format!(
                "{}: {}",
                tr!("settings.models.routeUnavailableReason"),
                v(record, "/unavailableReason")
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| tr!("settings.models.routeUnresolved"))
            ));
            rows.push(
                item(
                    &format!(
                        "blocked:{}/{}",
                        s(record, "/connectionId"),
                        s(record, "/modelId")
                    ),
                    s(record, "/modelId"),
                    detail.join("\n"),
                )
                .badge(
                    v(record, "/protocolId").as_str().unwrap_or("—"),
                    Tone::Muted,
                ),
            );
        }
    }
    rows.push(section(tr!("settings.models.liveModels")));
    rows.push(button(
        tr!("settings.models.searchLabel"),
        PageId::Models.search(app).expect("searchable model list"),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("tui.catalog.query"),
        app.model_query.clone(),
        Tone::Muted,
    ));
    rows.push(choice(
        tr!("settings.modelExperience.usageFilter"),
        vec![
            ("all".into(), tr!("settings.modelExperience.allUsages")),
            (
                "conversation".into(),
                tr!("settings.modelExperience.conversationEnabled"),
            ),
            (
                "internal".into(),
                tr!("settings.modelExperience.internalEnabled"),
            ),
            (
                "multimodal".into(),
                tr!("settings.modelExperience.multimodalEnabled"),
            ),
        ],
        app.model_usage.clone(),
        Edit::Local(LocalSetting::ModelUsage),
    ));
    rows.push(choice(
        tr!("settings.modelExperience.capabilityFilter"),
        std::iter::once((
            "all".into(),
            tr!("settings.modelExperience.allCapabilities"),
        ))
        .chain(
            MODALITY_ORDER
                .into_iter()
                .map(|kind| (kind.to_string(), modality_label(kind))),
        )
        .collect(),
        app.model_output.clone(),
        Edit::Local(LocalSetting::ModelOutput),
    ));
    let shown: Vec<&Entry<'_>> = group
        .models
        .iter()
        .filter(|entry| matches_search(entry, &app.model_query))
        .filter(|entry| visible(app, ai, &provider_id, entry))
        .collect();
    rows.push(info(
        tr!(
            "settings.models.catalogSummary",
            provider = provider_name(group.provider),
            count = group.models.len()
        ),
        tr!(
            "settings.models.filteredSummary",
            visible = shown.len(),
            total = group.models.len()
        ),
        Tone::Muted,
    ));
    if shown.is_empty() {
        rows.push(note(tr!("settings.models.emptyFilter"), Tone::Muted));
    }
    for entry in shown {
        let detail = [
            entry.model_id.clone(),
            modality_summary(entry),
            tr!(
                "settings.models.availableCredentials",
                count = entry.available_accounts.len()
            ),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        rows.push(
            item(
                &format!("model:{provider_id}/{}", entry.model_id),
                entry.display_name.clone(),
                detail,
            )
            .actions(cells(ai, group.provider, entry)),
        );
    }
    rows
}
