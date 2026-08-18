//! Bundled manifest single-authority regression tests.
//! 内置清单单一权威回归测试。

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use vulcan_provider_adapters::ManifestOwnership;

/// Returns the authoritative bundled manifest directory below the runtime-assets crate.
/// 返回 runtime-assets crate 下的权威内置清单目录。
///
/// # Returns
/// 返回值
/// The repository path that owns every bundled provider document.
/// 返回持有全部内置供应商文档的仓库路径。
fn bundled_manifests_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vulcan-runtime-assets")
        .join("resources")
        .join("provider-manifests")
        .join("manifests")
}

/// Reads every authoritative bundled manifest keyed by its file-stem provider identity.
/// 读取全部权威内置清单，并按文件名中的供应商标识索引。
///
/// # Returns
/// 返回值
/// Exact UTF-8 manifest documents in deterministic provider-id order.
/// 返回按供应商标识确定性排序的精确 UTF-8 清单文档。
fn load_bundled_manifests() -> BTreeMap<String, String> {
    let directory = bundled_manifests_directory();
    let mut manifests = BTreeMap::new();
    let entries = std::fs::read_dir(&directory).unwrap_or_else(|_| {
        panic!(
            "missing bundled manifest directory at {}",
            directory.display()
        )
    });
    for entry in entries {
        let entry = entry.expect("read bundled manifest entry");
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let provider_id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("bundled manifest file must have a provider-id stem")
            .to_string();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read bundled manifest {}", path.display()));
        manifests.insert(provider_id, text);
    }
    manifests
}

/// Every reviewed OpenRouter model that remains default-enabled after migration.
/// 迁移后仍默认启用的全部已审核 OpenRouter 模型。
const OPENROUTER_FEATURED_MODELS: &[&str] = &[
    "openai/gpt-5.6-sol",
    "openai/gpt-5.6-terra",
    "anthropic/claude-sonnet-5",
    "anthropic/claude-opus-4.8",
    "google/gemini-3.1-pro-preview",
    "google/gemini-3.5-flash",
    "deepseek/deepseek-v4-pro",
    "deepseek/deepseek-v4-flash",
    "qwen/qwen3.6-plus",
    "z-ai/glm-5.2",
    "minimax/minimax-m3",
    "openai/text-embedding-3-small",
    "cohere/rerank-v3.5",
    "openai/gpt-image-1",
    "google/veo-3.1",
    "minimax/speech-2.8-turbo",
    "openai/whisper-large-v3-turbo",
];

/// Every source-derived OpenCode Go model with a complete models.dev contract.
/// 具备完整 models.dev 契约的全部源码派生 OpenCode Go 模型。
const OPENCODE_GO_MODELS: &[&str] = &[
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "glm-5",
    "glm-5.1",
    "glm-5.2",
    "glm-5.3",
    "gpt-5.6-luna",
    "grok-4.5",
    "hy3",
    "kimi-k2.5",
    "kimi-k2.6",
    "kimi-k2.7-code",
    "kimi-k3",
    "mimo-v2-omni",
    "mimo-v2-pro",
    "mimo-v2.5",
    "mimo-v2.5-pro",
    "minimax-m2.5",
    "minimax-m2.7",
    "minimax-m3",
    "qwen3.5-plus",
    "qwen3.6-plus",
    "qwen3.7-max",
    "qwen3.7-plus",
    "qwen3.8-max",
];

/// Every CommandCode model declared by CLI 1.26.0 and currently returned by the Provider API.
/// CommandCode CLI 1.26.0 声明且当前 Provider API 返回的全部模型。
const COMMANDCODE_MODELS: &[&str] = &[
    "claude-sonnet-5",
    "claude-sonnet-4-6",
    "claude-fable-5",
    "claude-opus-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-haiku-4-5-20251001",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.3-codex",
    "gpt-5.4-mini",
    "deepseek/deepseek-v4-pro",
    "deepseek/deepseek-v4-flash",
    "moonshotai/Kimi-K3",
    "moonshotai/Kimi-K2.7-Code",
    "moonshotai/Kimi-K2.7-Code-Highspeed",
    "moonshotai/Kimi-K2.6",
    "moonshotai/Kimi-K2.5",
    "zai-org/GLM-5.3",
    "zai-org/GLM-5.2",
    "zai-org/GLM-5.2-Fast",
    "zai-org/GLM-5.1",
    "zai-org/GLM-5",
    "MiniMaxAI/MiniMax-M3",
    "MiniMaxAI/MiniMax-M2.7",
    "MiniMaxAI/MiniMax-M2.5",
    "xiaomi/mimo-v2.5-pro",
    "xiaomi/mimo-v2.5",
    "Qwen/Qwen3.8-Max",
    "Qwen/Qwen3.7-Max",
    "Qwen/Qwen3.7-Plus",
    "Qwen/Qwen3.7-Flash",
    "Qwen/Qwen3.6-Max-Preview",
    "Qwen/Qwen3.6-Plus",
    "stepfun/Step-3.7-Flash",
    "stepfun/Step-3.5-Flash",
    "tencent/hy3-paid",
    "google/gemini-3.7-flash",
    "google/gemini-3.6-flash",
    "google/gemini-3.5-flash",
    "google/gemini-3.5-flash-lite",
    "google/gemini-3.1-flash-lite",
    "sakana/fugu-ultra",
    "nvidia/nemotron-3-ultra-550b-a55b",
    "thinkingmachines/inkling",
    "thinkingmachines/inkling-small",
    "poolside/laguna-s-2.1-free",
    "meta/muse-spark-1.1",
    "meta/muse-spark-1.2",
    "meta/muse-spark-1.2-contributor",
    "xai/grok-4.5",
    "xai/grok-4.6",
];

/// Verifies the packaged integrity index binds the complete bundled manifest generation.
/// 验证打包完整性 Index 绑定完整的内置 Manifest 代际。
#[test]
fn bundled_integrity_index_binds_the_complete_manifest_generation() {
    // manifests can be returned only after index file-set, bytes, digest, identity, revision,
    // and engine-contract pins all match the source resource generation.
    // manifests 仅在 Index 的文件集合、字节、摘要、身份、修订和引擎契约固定值全部匹配
    // 源资源代际后才会返回。
    let provider_count = vulcan_provider_adapters::verify_source_bundled_manifest_integrity_index()
        .expect("source bundled integrity generation should verify");
    assert_eq!(provider_count, 16);
}

/// Verifies model-access defaults are fully owned by every bundled manifest.
/// 验证模型访问默认值完全由每份内置清单持有。
#[test]
fn bundled_manifest_model_defaults_are_self_contained() {
    for (provider_id, text) in load_bundled_manifests() {
        let compiled =
            vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
                .unwrap_or_else(|error| {
                    panic!("bundled manifest for `{provider_id}` must compile: {error}")
                });
        let service_entry_id = compiled
            .descriptor
            .service_entries
            .first()
            .expect("bundled manifest must expose one service entry")
            .id
            .as_str();

        if provider_id == "openrouter" {
            for curated in OPENROUTER_FEATURED_MODELS {
                assert!(
                    vulcan_provider_adapters::manifest_default_enabled(
                        &compiled,
                        service_entry_id,
                        None,
                        curated,
                    ),
                    "OpenRouter manifest must default-enable reviewed model `{curated}`"
                );
            }
            assert!(
                !vulcan_provider_adapters::manifest_default_enabled(
                    &compiled,
                    service_entry_id,
                    None,
                    "vendor/unreviewed-model"
                ),
                "OpenRouter manifest must keep unreviewed models disabled by default"
            );
        } else {
            assert!(
                vulcan_provider_adapters::manifest_default_enabled(
                    &compiled,
                    service_entry_id,
                    None,
                    "future/provider-model"
                ),
                "provider `{provider_id}` manifest must explicitly own its future-model default"
            );
        }
    }
}

/// Verifies DeepSeek exposes only the two current official identities with exact protocol scopes.
/// 验证 DeepSeek 仅公开当前两款官方模型身份及其精确协议作用域。
#[test]
fn deepseek_manifest_declares_only_current_official_models_and_protocols() {
    // path identifies the single authoritative DeepSeek bundled manifest.
    // path 标识唯一权威的 DeepSeek 内置清单。
    let path = bundled_manifests_directory().join("deepseek.json");
    // text preserves the exact packaged document consumed by the production loader.
    // text 保留生产加载器消费的精确打包文档。
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("read bundled manifest {}", path.display()));
    // compiled is the same resolved authority used by discovery and execution.
    // compiled 是发现与执行共同使用的同一份解析权威。
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("bundled DeepSeek manifest must compile");
    assert_eq!(
        compiled
            .model_source_policy("official-api")
            .expect("DeepSeek official API must declare a model source policy"),
        vulcan_provider_adapters::ModelSourcePolicy::ManifestOnly
    );
    // declaredModels freezes the exact hot-updatable official model inventory.
    // declaredModels 冻结可热更新的精确官方模型清单。
    let declared_models = compiled
        .catalog_models
        .get("official-api")
        .expect("DeepSeek official API must declare its model inventory");
    // declaredModelIds normalizes ordering for exact identity comparison.
    // declaredModelIds 规范化顺序以便精确比较模型身份。
    let declared_model_ids = declared_models
        .iter()
        .map(|model| model.model_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        declared_model_ids,
        BTreeSet::from(["deepseek-v4-flash", "deepseek-v4-pro"])
    );
    assert_eq!(
        compiled.catalog_membership_policy,
        vulcan_provider_adapters::CatalogMembershipPolicy::DeclaredOnly
    );
    assert_eq!(
        compiled.unknown_model_policy,
        vulcan_provider_adapters::UnknownModelPolicy::DeclaredRuleRequired
    );
    assert!(
        compiled
            .rule_for_model("official-api", None, "deepseek-chat")
            .is_none(),
        "retired DeepSeek Chat must not remain declared"
    );
    assert!(
        compiled
            .rule_for_model("official-api", None, "deepseek-reasoner")
            .is_none(),
        "retired DeepSeek Reasoner must not remain declared"
    );
    assert!(
        compiled
            .rule_for_model("official-api", None, "deepseek-v4-flash-preview")
            .is_none(),
        "the official Flash identity must use exact matching"
    );
    // flash owns Responses, Anthropic, and Chat with Responses selected.
    // flash 持有 Responses、Anthropic 与 Chat，并选定 Responses。
    let flash = compiled
        .rule_for_model("official-api", None, "deepseek-v4-flash")
        .expect("DeepSeek V4 Flash rule must exist");
    // flashCapabilities is the required model-level capability declaration for Flash.
    // flashCapabilities 是 Flash 必须提供的模型级能力声明。
    let flash_capabilities = flash
        .capabilities
        .as_ref()
        .expect("DeepSeek V4 Flash capabilities must exist");
    assert_eq!(
        flash
            .supported_protocols
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "anthropic-messages.v1",
            "openai-chat-completions.v1",
            "openai-responses.v1",
        ])
    );
    assert_eq!(
        flash.selected_protocol.as_deref(),
        Some("openai-responses.v1")
    );
    assert_eq!(
        flash_capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::None,
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::Low,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
    );
    assert_eq!(
        flash
            .protocol_capabilities
            .get("anthropic-messages.v1")
            .expect("Flash Anthropic capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );
    assert_eq!(
        flash
            .protocol_capabilities
            .get("openai-responses.v1")
            .expect("Flash Responses capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );
    assert!(
        flash
            .protocol_capabilities
            .get("openai-chat-completions.v1")
            .expect("Flash Chat capabilities must exist")
            .provider_native_tools
            .is_empty(),
        "Flash Chat must not declare provider-native web search"
    );
    // pro owns the same protocol set and selected Responses route as flash.
    // pro 持有与 flash 相同的协议集合，并同样选定 Responses 路由。
    let pro = compiled
        .rule_for_model("official-api", None, "deepseek-v4-pro")
        .expect("DeepSeek V4 Pro rule must exist");
    // proCapabilities is the required model-level capability declaration for Pro.
    // proCapabilities 是 Pro 必须提供的模型级能力声明。
    let pro_capabilities = pro
        .capabilities
        .as_ref()
        .expect("DeepSeek V4 Pro capabilities must exist");
    assert_eq!(
        pro.supported_protocols
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "anthropic-messages.v1",
            "openai-chat-completions.v1",
            "openai-responses.v1",
        ])
    );
    assert_eq!(
        pro.selected_protocol.as_deref(),
        Some("openai-responses.v1")
    );
    assert_eq!(
        pro_capabilities.reasoning_efforts,
        flash_capabilities.reasoning_efforts
    );
    assert_eq!(
        pro.protocol_capabilities
            .get("anthropic-messages.v1")
            .expect("Pro Anthropic capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );
    assert_eq!(
        pro.protocol_capabilities
            .get("openai-responses.v1")
            .expect("Pro Responses capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );
    assert!(
        pro.protocol_capabilities
            .get("openai-chat-completions.v1")
            .expect("Pro Chat capabilities must exist")
            .provider_native_tools
            .is_empty(),
        "Pro Chat must not declare provider-native web search"
    );
}

/// Verifies OpenCode Go freezes the source-derived inventory, endpoints, protocols, and model controls.
/// 验证 OpenCode Go 固化源码派生的模型清单、端点、协议与模型控制项。
///
/// # Returns
/// 返回值
/// This assertion-only regression test returns no value.
/// 此纯断言回归测试不返回值。
#[test]
fn opencode_go_manifest_matches_source_derived_model_contracts() {
    // path identifies the single authoritative OpenCode Go bundled manifest.
    // path 标识唯一权威的 OpenCode Go 内置清单。
    let path = bundled_manifests_directory().join("opencode-go.json");
    // text preserves the exact packaged document consumed by the production loader.
    // text 保留生产加载器消费的精确打包文档。
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("read bundled manifest {}", path.display()));
    // document exposes route paths and authentication exactly as declared on the wire.
    // document 以声明的 Wire 形态公开精确路由路径与认证方式。
    let document: Value = serde_json::from_str(&text).expect("parse OpenCode Go manifest JSON");
    assert_eq!(
        document.pointer("/services/0/credential_profiles/0/default_usage_scopes"),
        Some(&serde_json::json!(["interactive_agent"])),
        "OpenCode Go must keep conversation-only as its initial default, not an internal-use ceiling"
    );
    // routes is the only OpenCode Go conversation service route collection.
    // routes 是唯一的 OpenCode Go 会话服务路由集合。
    let routes = document
        .pointer("/services/0/protocol_routes")
        .and_then(Value::as_array)
        .expect("OpenCode Go protocol routes must exist");
    // route finds one exact protocol route without accepting aliases or fallback candidates.
    // route 精确查找一条协议路由，不接受别名或回退候选。
    let route = |protocol_id: &str| {
        routes
            .iter()
            .find(|candidate| candidate["protocol_profile_id"] == protocol_id)
            .unwrap_or_else(|| panic!("missing OpenCode Go route `{protocol_id}`"))
    };
    assert_eq!(
        route("openai-chat-completions.v1")["endpoint_path"],
        "/chat/completions"
    );
    assert_eq!(route("anthropic-messages.v1")["endpoint_path"], "/messages");
    assert_eq!(
        route("anthropic-messages.v1")["authentication"],
        "x_api_key"
    );
    assert_eq!(route("openai-responses.v1")["endpoint_path"], "/responses");

    // compiled is the same resolved authority used by discovery and execution.
    // compiled 是发现与执行共同使用的同一份解析权威。
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("bundled OpenCode Go manifest must compile");
    assert_eq!(
        compiled
            .model_source_policy("go-api")
            .expect("OpenCode Go must declare a model source policy"),
        vulcan_provider_adapters::ModelSourcePolicy::ManifestOnly
    );
    // declaredModelIds normalizes ordering for exact current-inventory comparison.
    // declaredModelIds 规范化顺序以便精确比较当前模型清单。
    let declared_model_ids = compiled
        .catalog_models
        .get("go-api")
        .expect("OpenCode Go must declare its complete inventory")
        .iter()
        .map(|model| model.model_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        declared_model_ids,
        OPENCODE_GO_MODELS.iter().copied().collect::<BTreeSet<_>>()
    );
    assert!(
        compiled
            .rule_for_model("go-api", None, "hy3-preview")
            .is_none(),
        "an endpoint-only preview without a complete source contract must stay undeclared"
    );

    // flash is the user-verified three-protocol DeepSeek Flash route.
    // flash 是用户已验证的 DeepSeek Flash 三协议路由。
    let flash = compiled
        .rule_for_model("go-api", None, "deepseek-v4-flash")
        .expect("OpenCode Go DeepSeek V4 Flash rule must exist");
    assert_eq!(
        flash
            .supported_protocols
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "anthropic-messages.v1",
            "openai-chat-completions.v1",
            "openai-responses.v1",
        ])
    );
    assert_eq!(
        flash.selected_protocol.as_deref(),
        Some("openai-responses.v1")
    );
    assert_eq!(
        flash
            .protocol_capabilities
            .get("anthropic-messages.v1")
            .expect("Flash Anthropic capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );
    assert_eq!(
        flash
            .protocol_capabilities
            .get("openai-responses.v1")
            .expect("Flash Responses capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );

    // Pro supports Responses, Anthropic, and Chat, and binds to the highest-priority Responses route.
    // Pro 支持 Responses、Anthropic 与 Chat，并绑定到优先级最高的 Responses 路由。
    let pro = compiled
        .rule_for_model("go-api", None, "deepseek-v4-pro")
        .expect("OpenCode Go DeepSeek V4 Pro rule must exist");
    assert_eq!(
        pro.supported_protocols
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "anthropic-messages.v1",
            "openai-chat-completions.v1",
            "openai-responses.v1"
        ])
    );
    assert_eq!(
        pro.selected_protocol.as_deref(),
        Some("openai-responses.v1")
    );
    assert_eq!(
        pro.protocol_capabilities
            .get("openai-responses.v1")
            .expect("Pro Responses capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );

    // glm is the OpenAI-compatible source mapping with explicit high/max effort choices.
    // glm 是带显式 high/max 思维档位的 OpenAI 兼容源码映射。
    let glm = compiled
        .rule_for_model("go-api", None, "glm-5.2")
        .expect("OpenCode Go GLM-5.2 rule must exist");
    assert_eq!(
        glm.selected_protocol.as_deref(),
        Some("openai-chat-completions.v1")
    );
    assert_eq!(
        glm.capabilities
            .as_ref()
            .expect("GLM-5.2 capabilities must exist")
            .reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
    );

    // glm53 must remain execution-equivalent to the user-confirmed GLM-5.2 contract.
    // glm53 必须与用户确认的 GLM-5.2 契约保持执行等价。
    let glm53 = compiled
        .rule_for_model("go-api", None, "glm-5.3")
        .expect("OpenCode Go GLM-5.3 rule must exist");
    assert_eq!(&glm53.supported_protocols, &glm.supported_protocols);
    assert_eq!(&glm53.selected_protocol, &glm.selected_protocol);
    // glm53_capabilities normalizes only provenance before comparing every executable field.
    // glm53_capabilities 仅规范化证据来源，再比较全部可执行字段。
    let mut glm53_capabilities = glm53
        .capabilities
        .clone()
        .expect("GLM-5.3 capabilities must exist");
    let glm_capabilities = glm
        .capabilities
        .as_ref()
        .expect("GLM-5.2 capabilities must exist");
    glm53_capabilities.evidence_revision = glm_capabilities.evidence_revision.clone();
    assert_eq!(&glm53_capabilities, glm_capabilities);
    assert_eq!(&glm53.model_parameters, &glm.model_parameters);
    assert_eq!(&glm53.protocol_capabilities, &glm.protocol_capabilities);

    // qwen is the Anthropic source mapping with budget-derived high/max choices.
    // qwen 是带预算派生 high/max 档位的 Anthropic 源码映射。
    let qwen = compiled
        .rule_for_model("go-api", None, "qwen3.8-max")
        .expect("OpenCode Go Qwen3.8 Max rule must exist");
    assert_eq!(
        qwen.selected_protocol.as_deref(),
        Some("anthropic-messages.v1")
    );
    assert_eq!(
        qwen.capabilities
            .as_ref()
            .expect("Qwen3.8 Max capabilities must exist")
            .reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
    );

    // luna is the Responses source mapping with the complete OpenCode effort set and file inputs.
    // luna 是带完整 OpenCode 思维档位及文件输入的 Responses 源码映射。
    let luna = compiled
        .rule_for_model("go-api", None, "gpt-5.6-luna")
        .expect("OpenCode Go GPT-5.6 Luna rule must exist");
    // lunaCapabilities freezes exact modality and token-limit evidence from models.dev.
    // lunaCapabilities 固化 models.dev 中精确的模态与 Token 上限证据。
    let luna_capabilities = luna
        .capabilities
        .as_ref()
        .expect("GPT-5.6 Luna capabilities must exist");
    assert_eq!(
        luna.selected_protocol.as_deref(),
        Some("openai-responses.v1")
    );
    assert_eq!(
        luna_capabilities.input_resources,
        vec![
            vulcan_ai_contract::ResourceKind::Image,
            vulcan_ai_contract::ResourceKind::File,
        ]
    );
    assert_eq!(luna_capabilities.context_limit, Some(1_050_000));
    assert_eq!(luna_capabilities.output_limit, Some(128_000));
    assert_eq!(
        luna_capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::None,
            vulcan_ai_contract::ReasoningEffort::Low,
            vulcan_ai_contract::ReasoningEffort::Medium,
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::XHigh,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
    );

    // grokTemperature proves Responses-only temperature support remains model-scoped.
    // grokTemperature 证明仅 Responses 的温度支持仍严格限定在模型作用域内。
    let grok_temperature = compiled
        .rule_for_model("go-api", None, "grok-4.5")
        .expect("OpenCode Go Grok 4.5 rule must exist")
        .model_parameters
        .iter()
        .find(|parameter| parameter.id == "sampling.temperature")
        .expect("Grok 4.5 must expose Responses temperature");
    assert_eq!(
        grok_temperature.protocol_ids,
        vec![String::from("openai-responses.v1")]
    );
}

/// Verifies CommandCode freezes exact online identities, multimodality, routing, and usage.
/// 验证 CommandCode 固化精确在线身份、多模态、路由与用量契约。
#[test]
fn commandcode_manifest_freezes_the_verified_provider_contract() {
    // path identifies the single authoritative CommandCode bundled manifest.
    // path 标识唯一权威的 CommandCode 内置清单。
    let path = bundled_manifests_directory().join("commandcode.json");
    // text preserves the exact packaged document consumed by production.
    // text 保留生产环境消费的精确打包文档。
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("read bundled manifest {}", path.display()));
    // document exposes exact wire paths and matcher kinds.
    // document 公开精确 Wire 路径与匹配器种类。
    let document: Value = serde_json::from_str(&text).expect("parse CommandCode manifest JSON");
    assert_eq!(
        document.pointer("/services/0/base_url"),
        Some(&Value::String(String::from(
            "https://api.commandcode.ai/provider/v1"
        )))
    );
    assert_eq!(
        document.pointer("/services/0/request_projector"),
        Some(&Value::String(String::from("direct")))
    );
    assert_eq!(
        document.pointer("/services/0/credential_profiles/0/key_prefix"),
        None,
        "CommandCode 1.26.0 validates API keys remotely instead of enforcing a local prefix"
    );
    assert_eq!(
        document.pointer("/services/0/credential_profiles/1/key_prefix"),
        None,
        "browser-delivered CommandCode keys must not be rejected by an obsolete prefix rule"
    );
    assert_eq!(
        document.pointer("/services/0/credential_profiles/0/account_usage_query"),
        Some(&Value::String(String::from("command_code_usage")))
    );
    let routes = document
        .pointer("/services/0/protocol_routes")
        .and_then(Value::as_array)
        .expect("CommandCode protocol routes must exist");
    assert_eq!(
        routes.len(),
        2,
        "CommandCode must expose its two model-family endpoints"
    );
    // routeByProtocol freezes each wire endpoint instead of relying on array order.
    // routeByProtocol 按协议冻结每个 Wire 端点，而不是依赖数组顺序。
    let route_by_protocol = |protocol_id: &str| {
        routes
            .iter()
            .find(|route| route["protocol_profile_id"] == protocol_id)
            .unwrap_or_else(|| panic!("missing CommandCode route `{protocol_id}`"))
    };
    let anthropic_route = route_by_protocol("anthropic-messages.v1");
    assert_eq!(anthropic_route["endpoint_path"], "/messages");
    assert_eq!(
        anthropic_route["request_headers"]["anthropic-version"],
        "2023-06-01"
    );
    let chat_route = route_by_protocol("openai-chat-completions.v1");
    assert_eq!(chat_route["endpoint_path"], "/chat/completions");
    // transientProfiles prove Chat-only gateway fields are mapped by manifest configuration.
    // transientProfiles 证明仅 Chat 网关字段通过 Manifest 配置完成映射。
    let transient_profiles = document["response_profiles"]
        .as_array()
        .expect("CommandCode response profiles must exist")
        .iter()
        .map(|profile| {
            (
                profile["profile_id"].as_str().expect("response profile id"),
                profile["payload"]["transient_reasoning_status"]
                    .as_str()
                    .expect("transient reasoning status mode"),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        transient_profiles,
        BTreeSet::from([
            ("commandcode.chat.reasoning-summary.v1", "summary"),
            ("commandcode.chat.encrypted-reasoning.v1", "encrypted"),
        ])
    );
    // mappedModels freezes the verified model-to-response-profile assignment without runtime model checks.
    // mappedModels 冻结已验证的模型到响应 Profile 分配，且无需运行时模型判断。
    let mapped_models = document["model_rules"]
        .as_array()
        .expect("CommandCode model rules")
        .iter()
        .filter_map(|rule| {
            let refs = rule["response_profile_refs"].as_array()?;
            if refs.is_empty() {
                return None;
            }
            Some((
                rule["matcher"]["value"].as_str().expect("mapped model id"),
                refs[0].as_str().expect("mapped response profile id"),
            ))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        mapped_models,
        BTreeSet::from([
            ("gpt-5.6-luna", "commandcode.chat.reasoning-summary.v1"),
            (
                "meta/muse-spark-1.2",
                "commandcode.chat.encrypted-reasoning.v1"
            ),
            (
                "meta/muse-spark-1.2-contributor",
                "commandcode.chat.encrypted-reasoning.v1"
            ),
            ("xai/grok-4.5", "commandcode.chat.reasoning-summary.v1"),
            ("xai/grok-4.6", "commandcode.chat.reasoning-summary.v1"),
        ])
    );
    assert!(
        document["model_rules"]
            .as_array()
            .expect("CommandCode model rules")
            .iter()
            .all(|rule| rule["matcher"]["kind"] == "exact"),
        "CommandCode must not admit future models through prefix matching"
    );

    // compiled is the production authority used by discovery and execution.
    // compiled 是发现与执行使用的生产权威。
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("bundled CommandCode manifest must compile");
    assert!(
        compiled.descriptor.service_entries[0]
            .credential_profiles
            .iter()
            .all(|profile| profile.key_prefix.is_none()),
        "compiled CommandCode credential profiles must defer key validity to the account API"
    );
    assert_eq!(
        compiled
            .model_source_policy("provider-api")
            .expect("CommandCode source policy"),
        vulcan_provider_adapters::ModelSourcePolicy::DiscoveryOnly
    );
    assert_eq!(
        compiled.catalog_membership_policy,
        vulcan_provider_adapters::CatalogMembershipPolicy::DeclaredOnly
    );
    assert_eq!(
        compiled.unknown_model_policy,
        vulcan_provider_adapters::UnknownModelPolicy::DeclaredRuleRequired
    );
    let declared_model_ids = compiled
        .rule_instances
        .iter()
        .map(|rule| rule.matcher.value.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        declared_model_ids,
        COMMANDCODE_MODELS.iter().copied().collect::<BTreeSet<_>>()
    );
    assert!(
        compiled
            .rule_for_model("provider-api", None, "MiniMaxAI/MiniMax-M3-Free")
            .is_none(),
        "a CLI-only model absent from the Provider API must remain unavailable"
    );

    // textOnlyModels is the exact current-online subset of the CLI 1.26.0 text-only registry.
    // textOnlyModels 是 CLI 1.26.0 纯文本注册表中当前在线的精确子集。
    let text_only_models = BTreeSet::from([
        "deepseek/deepseek-v4-pro",
        "deepseek/deepseek-v4-flash",
        "zai-org/GLM-5.3",
        "zai-org/GLM-5.2",
        "zai-org/GLM-5.2-Fast",
        "zai-org/GLM-5.1",
        "zai-org/GLM-5",
        "MiniMaxAI/MiniMax-M2.7",
        "MiniMaxAI/MiniMax-M2.5",
        "xiaomi/mimo-v2.5-pro",
        "Qwen/Qwen3.6-Max-Preview",
        "Qwen/Qwen3.7-Max",
        "stepfun/Step-3.5-Flash",
        "tencent/hy3-paid",
        "nvidia/nemotron-3-ultra-550b-a55b",
        "poolside/laguna-s-2.1-free",
        "xai/grok-4.6",
    ]);
    for model_id in COMMANDCODE_MODELS {
        let rule = compiled
            .rule_for_model("provider-api", None, model_id)
            .unwrap_or_else(|| panic!("missing CommandCode model rule `{model_id}`"));
        let capabilities = rule
            .capabilities
            .as_ref()
            .unwrap_or_else(|| panic!("missing CommandCode capabilities `{model_id}`"));
        // expectedReasoningEfforts freezes explicit CLI hints while defaulting unlisted thinking models to the gateway's five accepted levels.
        // expectedReasoningEfforts 冻结 CLI 明确提示；未列出的思考模型则采用网关接受的完整五档。
        let expected_reasoning_efforts = match *model_id {
            "gpt-5.5" | "gpt-5.4" | "gpt-5.3-codex" => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::XHigh,
            ],
            "gpt-5.4-mini"
            | "google/gemini-3.7-flash"
            | "google/gemini-3.6-flash"
            | "google/gemini-3.5-flash"
            | "google/gemini-3.5-flash-lite"
            | "google/gemini-3.1-flash-lite"
            | "xai/grok-4.5" => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
            ],
            "deepseek/deepseek-v4-pro" | "deepseek/deepseek-v4-flash" | "zai-org/GLM-5.2" => vec![
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
            "zai-org/GLM-5.3" => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
            "Qwen/Qwen3.8-Max" => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::XHigh,
            ],
            "sakana/fugu-ultra" => vec![
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::XHigh,
            ],
            "xai/grok-4.6" => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::XHigh,
            ],
            "moonshotai/Kimi-K2.5" | "nvidia/nemotron-3-ultra-550b-a55b" => Vec::new(),
            _ => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::XHigh,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
        };
        if expected_reasoning_efforts.is_empty() {
            assert_eq!(
                capabilities.reasoning,
                vulcan_ai_contract::CapabilitySupport::Unsupported,
                "only the two verified no-mode models may disable reasoning for `{model_id}`"
            );
            assert_eq!(
                capabilities.reasoning_control,
                vulcan_ai_contract::ReasoningControlMode::FixedOff,
                "no-mode model reasoning control drifted for `{model_id}`"
            );
        } else {
            assert_eq!(
                capabilities.reasoning,
                vulcan_ai_contract::CapabilitySupport::Native,
                "CommandCode reasoning support drifted for `{model_id}`"
            );
            assert_eq!(
                capabilities.reasoning_control,
                vulcan_ai_contract::ReasoningControlMode::EffortSelection,
                "CommandCode reasoning control drifted for `{model_id}`"
            );
        }
        assert_eq!(
            capabilities.reasoning_efforts, expected_reasoning_efforts,
            "CommandCode reasoning effort matrix drifted for `{model_id}`"
        );
        assert!(
            !capabilities
                .reasoning_efforts
                .contains(&vulcan_ai_contract::ReasoningEffort::None)
                && !capabilities
                    .reasoning_efforts
                    .contains(&vulcan_ai_contract::ReasoningEffort::Minimal),
            "CommandCode must never expose rejected none/minimal efforts for `{model_id}`"
        );
        let supports_image = capabilities
            .input_resources
            .contains(&vulcan_ai_contract::ResourceKind::Image);
        assert_eq!(
            supports_image,
            !text_only_models.contains(model_id),
            "CommandCode image-input declaration drifted for `{model_id}`"
        );
        // expectedProtocol enforces CommandCode's provider-side model-family routing rule.
        // expectedProtocol 强制执行 CommandCode 服务端的模型系列路由规则。
        let expected_protocol = if model_id.starts_with("claude-") {
            "anthropic-messages.v1"
        } else {
            "openai-chat-completions.v1"
        };
        assert_eq!(
            rule.selected_protocol.as_deref(),
            Some(expected_protocol),
            "CommandCode protocol binding drifted for `{model_id}`"
        );
        assert_eq!(
            rule.supported_protocols,
            vec![String::from(expected_protocol)],
            "CommandCode must never expose a cross-family fallback for `{model_id}`"
        );
    }
    let deepseek = compiled
        .rule_for_model("provider-api", None, "deepseek/deepseek-v4-flash")
        .expect("CommandCode DeepSeek V4 Flash rule");
    let deepseek_capabilities = deepseek
        .capabilities
        .as_ref()
        .expect("CommandCode DeepSeek V4 Flash capabilities");
    assert_eq!(
        deepseek_capabilities.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        deepseek_capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
    );
    assert_eq!(deepseek_capabilities.context_limit, Some(1_000_000));
    assert_eq!(
        compiled
            .rule_for_model("provider-api", None, "claude-haiku-4-5-20251001")
            .and_then(|rule| rule.capabilities.as_ref())
            .and_then(|capabilities| capabilities.context_limit),
        Some(200_000)
    );
    assert_eq!(
        compiled.descriptor.service_entries[0].credential_profiles[0].account_usage_query,
        vulcan_provider_adapters::AccountUsageQueryKind::CommandCodeUsage
    );
}

/// Verifies APIKEY.FUN remains a strict intersection of group-visible models and local rules.
/// 验证 APIKEY.FUN 仍是分组可见模型与本地规则的严格交集。
#[test]
fn apikeyfun_manifest_is_a_closed_group_intersection() {
    // text is the exact bundled APIKEY.FUN authority read by production tests.
    // text 是生产测试读取的精确内置 APIKEY.FUN 权威。
    let text = load_bundled_manifests()
        .remove("apikeyfun")
        .expect("apikeyfun bundled manifest should exist");
    // document retains presence-sensitive rule and target declarations for structural guards.
    // document 保留存在性敏感的规则与目标声明用于结构守卫。
    let document: Value = serde_json::from_str(&text).expect("apikeyfun manifest should parse");
    assert_eq!(
        document["services"][0]["model_catalog"]["source_policy"],
        "discovery_only"
    );
    assert_eq!(
        document["model_defaults"]["catalog_membership_policy"],
        "declared_only"
    );
    assert_eq!(
        document["model_defaults"]["unknown_model_policy"],
        "declared_rule_required"
    );
    // geminiRoute separates the native v1beta execution base from OpenAI-style model discovery.
    // geminiRoute 将原生 v1beta 执行基址与 OpenAI 风格模型发现入口分离。
    let gemini_route = document["services"][0]["protocol_routes"]
        .as_array()
        .expect("apikeyfun protocol_routes should be an array")
        .iter()
        .find(|route| route["protocol_profile_id"] == "google.gemini.generate_content")
        .expect("apikeyfun Gemini protocol route should exist");
    assert_eq!(
        gemini_route["base_url_override"],
        "https://api.apikey.fun/v1beta"
    );
    assert_eq!(
        gemini_route["model_discovery_base_url_override"],
        "https://api.apikey.fun/v1"
    );
    assert_eq!(gemini_route["model_discovery_path"], "/models");
    // expectedProtocols is the complete local whitelist paired with its only declared wire route.
    // expectedProtocols 是完整本地白名单及其唯一声明 Wire 路由。
    let expected_protocols = [
        ("gpt-5.6-sol", "openai-responses.v1"),
        ("gpt-5.6-terra", "openai-responses.v1"),
        ("gpt-5.6-luna", "openai-responses.v1"),
        ("gpt-5.5", "openai-responses.v1"),
        ("gpt-5.4", "openai-responses.v1"),
        ("gpt-5.3-codex-spark", "openai-responses.v1"),
        ("claude-opus-5", "anthropic-messages.v1"),
        ("claude-fable-5", "anthropic-messages.v1"),
        ("claude-opus-4-8", "anthropic-messages.v1"),
        ("claude-opus-4-7", "anthropic-messages.v1"),
        ("claude-opus-4-6", "anthropic-messages.v1"),
        ("claude-sonnet-5", "anthropic-messages.v1"),
        ("claude-sonnet-4-6", "anthropic-messages.v1"),
        ("claude-opus-4-5-20251101", "anthropic-messages.v1"),
        ("claude-haiku-4-5", "anthropic-messages.v1"),
        ("grok-4.5", "openai-responses.v1"),
        ("grok-4.6", "openai-responses.v1"),
        ("grok-imagine-image", "openai-responses.v1"),
        ("grok-imagine-image-quality", "openai-responses.v1"),
        ("grok-imagine-video", "openai-responses.v1"),
        ("gemini-3.6-flash", "google.gemini.generate_content"),
        ("gemini-3.5-flash", "google.gemini.generate_content"),
        ("gemini-3.1-pro", "google.gemini.generate_content"),
        ("gemini-3.1-flash-lite", "google.gemini.generate_content"),
        ("gemini-3-flash", "google.gemini.generate_content"),
        ("glm-5.2", "openai-responses.v1"),
        ("glm-5.1", "openai-responses.v1"),
        ("kimi-k3", "anthropic-messages-video.v1"),
        ("kimi-k2.7-code", "anthropic-messages-video.v1"),
        ("kimi-k2.6", "anthropic-messages-video.v1"),
        ("deepseek-v4-pro", "openai-responses.v1"),
        ("deepseek-v4-flash", "openai-responses.v1"),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    // actualProtocols rejects prefixes and captures every exact discovery-intersection member.
    // actualProtocols 拒绝前缀并捕获每个精确发现交集成员。
    let actual_protocols = document["model_rules"]
        .as_array()
        .expect("apikeyfun model_rules should be an array")
        .iter()
        .map(|rule| {
            assert_eq!(rule["matcher"]["kind"], "exact");
            (
                rule["matcher"]["value"]
                    .as_str()
                    .expect("apikeyfun rule should declare an exact model id"),
                rule["selected_protocol"]
                    .as_str()
                    .expect("apikeyfun rule should bind one protocol"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual_protocols, expected_protocols);

    // deepseekReasoningEfforts keeps relay declarations identical to the official DeepSeek rules.
    // deepseekReasoningEfforts 确保中转声明与 DeepSeek 官方规则完全一致。
    let deepseek_text = load_bundled_manifests()
        .remove("deepseek")
        .expect("deepseek bundled manifest should exist");
    // deepseekDocument is the official capability authority used for relay parity checks.
    // deepseekDocument 是中转一致性检查所使用的官方能力权威。
    let deepseek_document: Value =
        serde_json::from_str(&deepseek_text).expect("deepseek manifest should parse");
    for model_id in ["deepseek-v4-pro", "deepseek-v4-flash"] {
        // officialRule is the exact official DeepSeek model declaration.
        // officialRule 是精确的 DeepSeek 官方模型声明。
        let official_rule = deepseek_document["model_rules"]
            .as_array()
            .expect("deepseek model_rules should be an array")
            .iter()
            .find(|rule| rule["matcher"]["value"] == model_id)
            .expect("official DeepSeek model rule should exist");
        // relayRule is the matching APIKEY.FUN relay declaration.
        // relayRule 是对应的 APIKEY.FUN 中转声明。
        let relay_rule = document["model_rules"]
            .as_array()
            .expect("apikeyfun model_rules should be an array")
            .iter()
            .find(|rule| rule["matcher"]["value"] == model_id)
            .expect("APIKEY.FUN DeepSeek model rule should exist");
        assert_eq!(
            relay_rule["capabilities"]["reasoning_efforts"],
            official_rule["capabilities"]["reasoning_efforts"],
            "APIKEY.FUN reasoning levels must match official DeepSeek for `{model_id}`"
        );
    }

    // targets is the exact reviewed extension inventory; the unusable Grok edit id is absent.
    // targets 是精确已审核扩展清单；不可用的 Grok 编辑标识不存在。
    let targets = document["tool_targets"]
        .as_array()
        .expect("apikeyfun tool_targets should be an array");
    // targetIds protects the five verified media routes as a closed set.
    // targetIds 把五条已验证媒体路由保护为封闭集合。
    let target_ids = targets
        .iter()
        .map(|target| {
            target["target_id"]
                .as_str()
                .expect("apikeyfun target should have an id")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        target_ids,
        BTreeSet::from([
            "gpt-image-2.generate",
            "gpt-image-2.edit",
            "grok-imagine-image.generate",
            "grok-imagine-image-quality.generate",
            "grok-imagine-video.generate",
        ])
    );
    assert!(text.contains("\"availability_model_ids_any\""));
    assert!(text.contains("\"response_body\": \"binary_resource\""));
    assert!(text.contains("\"path\": \"/generationConfig/maxOutputTokens\", \"value\": 65536"));
    assert!(text.contains("\"mapping\": {\"32000\": 32768}"));
    assert!(text.contains("\"unmatched\": \"keep_literal\""));
    // referenceBinding guarantees local multi-reference paths are not leaked into the wire body.
    // referenceBinding 保证本地多参考路径不会泄漏到 Wire 请求正文。
    let reference_binding = document["operations"]
        .as_array()
        .expect("apikeyfun operations should be an array")
        .iter()
        .find(|operation| operation["operation_id"] == "apikeyfun.grok-imagine-video.generate")
        .and_then(|operation| operation["upload_bindings"].as_array())
        .and_then(|bindings| {
            bindings
                .iter()
                .find(|binding| binding["source_path"] == "/reference_images")
        })
        .expect("grok video reference-image upload binding should exist");
    assert_eq!(reference_binding["item_source_path"], "/path");
    assert_eq!(reference_binding["item_target_path"], "/url");
    assert_eq!(reference_binding["remove_source"], true);
}

/// Verifies migrated provider-tool inventories remain complete and uniquely manifest-owned.
/// 验证已迁移供应商工具清单保持完整、唯一且仅由 Manifest 持有。
#[test]
fn migrated_tool_target_inventories_are_complete_and_unique() {
    let directory = bundled_manifests_directory();
    for (provider_id, target_count) in [
        ("tavily", 2_usize),
        ("alibaba", 51_usize),
        ("minimax", 34_usize),
    ] {
        let path = directory.join(format!("{provider_id}.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read bundled manifest {}", path.display()));
        let compiled =
            vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
                .unwrap_or_else(|error| {
                    panic!("bundled manifest for `{provider_id}` must compile: {error}")
                });
        assert_eq!(
            compiled.tool_targets.len(),
            target_count,
            "provider `{provider_id}` must own exactly {target_count} migrated tool targets"
        );
        let manifest_ids = compiled
            .tool_targets
            .iter()
            .map(|target| target.target_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            manifest_ids.len(),
            target_count,
            "provider `{provider_id}` manifest target identifiers must be unique"
        );
        for target in &compiled.tool_targets {
            assert_eq!(
                target.provider_id, provider_id,
                "target `{}` must retain its manifest owner",
                target.target_id
            );
        }
    }
}

/// Verifies every bundled system tool target uses only the generic execution adapter.
/// 验证每个内置系统工具目标只使用通用执行适配器。
///
/// This is the stable R0 guard against restoring a provider-named production adapter after the
/// 92-target inventory. It checks the compiled authority rather than raw JSON spelling.
/// 这是防止 92 条目标清单恢复供应商命名生产适配器的稳定 R0 守卫；它检查编译后的
/// 权威，而不是原始 JSON 拼写。
#[test]
fn bundled_system_tool_targets_use_only_generic_execution() {
    // target_count is the exact number of compiled production targets across all bundled manifests.
    // target_count 是全部内置 Manifest 编译后的精确生产目标数量。
    let mut target_count = 0_usize;
    for (provider_id, text) in load_bundled_manifests() {
        // compiled is the same resolved manifest authority consumed by production runtime.
        // compiled 是生产运行时消费的同一份已解析 Manifest 权威。
        let compiled =
            vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
                .unwrap_or_else(|error| {
                    panic!("bundled manifest for `{provider_id}` must compile: {error}")
                });
        for target in &compiled.tool_targets {
            assert_eq!(
                target.execution.adapter,
                vulcan_provider_adapters::ProviderToolAdapterKind::GenericJson,
                "provider `{provider_id}` target `{}` restored a non-generic adapter",
                target.target_id
            );
            target_count += 1;
        }
    }
    assert_eq!(
        target_count, 92,
        "the complete production target inventory must remain covered"
    );
}

/// Verifies bundled model rules never use an explicit empty operation list by accident.
/// 验证内置模型规则绝不会误用显式空操作列表。
///
/// An omitted operation field inherits the conservative provider or catalog capability, while
/// an explicit empty array intentionally clears every operation. This guard preserves that
/// presence-sensitive contract and prevents conversation models from silently disappearing.
/// 缺省操作字段会继承保守的供应商或目录能力，而显式空数组会有意清空全部操作；本守卫保护
/// 该存在性敏感契约，防止会话模型静默消失。
#[test]
fn bundled_model_rules_never_explicitly_clear_all_operations() {
    for (provider_id, text) in load_bundled_manifests() {
        let document: Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("parse bundled manifest `{provider_id}`: {error}"));
        let Some(rules) = document.get("model_rules").and_then(Value::as_array) else {
            continue;
        };
        for (index, rule) in rules.iter().enumerate() {
            let Some(operations) = rule
                .get("capabilities")
                .and_then(|capabilities| capabilities.get("operations"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            assert!(
                !operations.is_empty(),
                "provider `{provider_id}` model_rules[{index}] explicitly clears every operation"
            );
        }
    }
}
