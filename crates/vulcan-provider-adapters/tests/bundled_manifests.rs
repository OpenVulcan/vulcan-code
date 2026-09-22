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
    "deepseek-v4-flash-vision-exp",
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
    "muse-spark-1.2-contributor",
    "qwen3.5-plus",
    "qwen3.6-plus",
    "qwen3.7-max",
    "qwen3.7-plus",
    "qwen3.8-max",
];

/// Every statically reviewed Token Rhythm conversation and image model.
/// 全部经过静态审查的基元律动会话与图片模型。
const TOKEN_RHYTHM_MODELS: &[&str] = &[
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "deepseek-v4-flash-0731",
    "deepseek-v4-pro-0813",
    "glm-5",
    "glm-5.1",
    "glm-5.2",
    "minimax-m2.5",
    "minimax-m2.7",
    "kimi-k2.5",
    "kimi-k2.6",
    "kimi-k2.7-code",
    "mimo-v2.5-pro",
    "qwen3.7-max",
    "qwen3.8-max",
    "seed-2.1-turbo",
    "seed-2.1-pro",
    "qwen-image-2.0",
    "wan2.7-image",
];

/// Every CommandCode model declared by CLI 1.54.1 and returned by the Provider API on 2026-09-16.
/// CommandCode CLI 1.54.1 声明且 2026-09-16 Provider API 返回的全部模型。
const COMMANDCODE_MODELS: &[&str] = &[
    "claude-sonnet-5",
    "claude-sonnet-4-6",
    "claude-fable-5",
    "claude-fable-5-1",
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
    "deepseek/deepseek-v4-flash-vision-exp",
    "deepseek/deepseek-v4-flash-fast",
    "deepseek/deepseek-v4.1-flash",
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
    "z-ai/glm-5.3-flash",
    "MiniMaxAI/MiniMax-M3",
    "MiniMaxAI/MiniMax-M2.7",
    "MiniMaxAI/MiniMax-M2.5",
    "xiaomi/mimo-v2.5-pro",
    "xiaomi/mimo-v2.5",
    "Qwen/Qwen3.8-Max",
    "Qwen/Qwen3.8-Max-0902",
    "Qwen/Qwen3.8-27B",
    "Qwen/Qwen3.8-Flash",
    "Qwen/Qwen3.7-Max",
    "Qwen/Qwen3.7-Plus",
    "Qwen/Qwen3.7-Flash",
    "Qwen/Qwen3.6-Max-Preview",
    "Qwen/Qwen3.6-Plus",
    "stepfun/Step-3.7-Flash",
    "stepfun/Step-3.5-Flash",
    "tencent/hy3-paid",
    "tencent/hy4-preview",
    "google/gemini-3.7-flash",
    "google/gemini-3.8-flash",
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
    "meta/muse-spark-1.3",
    "meta/muse-spark-1.3-contributor",
    "meituan/LongCat-2.0:free",
    "inclusionai/ling-3.0-flash-sante:free",
    "xai/grok-4.5",
    "xai/grok-4.6",
];

/// Returns one bundled service entry by its stable `service_entry_id`.
///
/// A service position is data, not contract: regenerating a manifest can insert or reorder entries,
/// so contract assertions locate the reviewed service by identity instead of by array index.
/// 按稳定 `service_entry_id` 返回一个内置服务入口。服务位置属于数据而非契约：清单重新生成时
/// 可能插入或重排条目，因此契约断言按身份定位被审查的服务，而不是按数组下标。
fn bundled_service_entry<'a>(document: &'a Value, service_entry_id: &str) -> &'a Value {
    document["services"]
        .as_array()
        .and_then(|services| {
            services
                .iter()
                .find(|service| service["service_entry_id"] == service_entry_id)
        })
        .unwrap_or_else(|| {
            panic!("bundled manifest no longer declares service entry `{service_entry_id}`")
        })
}

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
    // The verified count must cover exactly the bundled manifest file set. Asserting that relation
    // instead of a literal keeps this test generation-independent: adding or removing a provider
    // must never require editing it.
    // 已验证数量必须恰好覆盖内置 Manifest 文件集合。断言该关系而非字面量，使本测试与代际无关：
    // 新增或移除供应商都不需要修改本测试。
    let manifest_files = load_bundled_manifests().len();
    assert!(
        manifest_files > 0,
        "the bundled generation must contain at least one manifest"
    );
    assert_eq!(
        provider_count, manifest_files,
        "the integrity index must pin exactly the bundled manifest file set"
    );
}

/// Verifies the native Codex entries expose the reviewed multimodal and reasoning contract.
/// 验证原生 Codex 条目公开已审核的多模态与思考能力契约。
#[test]
fn openai_codex_model_rules_keep_multimodal_effort_contract() {
    let text = load_bundled_manifests()
        .remove("openai")
        .expect("openai bundled manifest should exist");
    let document: Value = serde_json::from_str(&text).expect("openai manifest should parse");
    let model_rules = document["model_rules"]
        .as_array()
        .expect("openai model_rules should be an array");
    let expected_efforts = serde_json::json!(["low", "medium", "high", "xhigh", "max"]);

    for model_id in [
        "gpt-5.6-luna",
        "gpt-5.6-terra",
        "gpt-5.6-sol",
        "gpt-6-astra",
    ] {
        let rule = model_rules
            .iter()
            .find(|rule| rule["matcher"]["value"] == model_id)
            .unwrap_or_else(|| panic!("OpenAI Codex model rule should exist for `{model_id}`"));
        assert_eq!(rule["matcher"]["kind"], "exact");
        assert_eq!(rule["selected_protocol"], "openai-codex-responses.v1");
        assert_eq!(
            rule["scope"]["service_entry_ids"],
            serde_json::json!(["codex-subscription"])
        );
        assert_eq!(
            rule["capabilities"]["operations"],
            serde_json::json!(["conversation_respond"])
        );
        assert_eq!(
            rule["capabilities"]["input_resources"],
            serde_json::json!(["file", "image"])
        );
        assert_eq!(
            rule["capabilities"]["output_resources"],
            serde_json::json!([])
        );
        assert_eq!(rule["capabilities"]["reasoning"], "native");
        assert_eq!(
            rule["capabilities"]["reasoning_control"],
            "effort_selection"
        );
        assert_eq!(rule["capabilities"]["reasoning_efforts"], expected_efforts);
        assert_eq!(rule["capabilities"]["context_limit"], 272000);
        assert_eq!(rule["codex_behavior"]["wire_dialect"], "lite");
        assert_eq!(rule["codex_behavior"]["tool_mode"], "code_mode_only");
        assert_eq!(rule["codex_behavior"]["apply_patch_tool_type"], "freeform");
        assert_eq!(rule["codex_behavior"]["shell_type"], "unified_exec");
        assert_eq!(
            rule["codex_behavior"]["web_search_tool_type"],
            "text_and_image"
        );
        assert_eq!(
            rule["codex_behavior"]["tool_output_truncation"]["limit"],
            10000
        );
        assert_eq!(rule["codex_behavior"]["comp_hash"], "3000");
    }

    let sol_rule = model_rules
        .iter()
        .find(|rule| rule["matcher"]["value"] == "gpt-5.6-sol")
        .expect("GPT-5.6 Sol Codex rule should exist");
    let astra_rule = model_rules
        .iter()
        .find(|rule| rule["matcher"]["value"] == "gpt-6-astra")
        .expect("GPT-6 Astra Codex rule should exist");
    assert_eq!(astra_rule["capabilities"], sol_rule["capabilities"]);
    assert_eq!(astra_rule["codex_behavior"]["multi_agent_version"], "v2");
    assert_eq!(
        astra_rule["codex_behavior"]["multi_agent_reasoning_effort"],
        "xhigh"
    );
    assert_eq!(
        astra_rule["codex_behavior"]["experimental_supported_tools"],
        serde_json::json!(["send_user_message_async", "clock"])
    );
    let luna_rule = model_rules
        .iter()
        .find(|rule| rule["matcher"]["value"] == "gpt-5.6-luna")
        .expect("GPT-5.6 Luna Codex rule should exist");
    assert_eq!(luna_rule["codex_behavior"]["multi_agent_version"], "v1");
}

/// Verifies AgentSpace keeps its ChatGPT Plan Responses route separate from Codex image generation.
/// 验证 AgentSpace 将 ChatGPT Plan 的 Responses 路由与 Codex 图片生成调用分离。
#[test]
fn agentspace_chatgpt_plan_keeps_codex_image_contract() {
    let text = load_bundled_manifests()
        .remove("agentspace")
        .expect("agentspace bundled manifest should exist");
    let document: Value = serde_json::from_str(&text).expect("agentspace manifest should parse");
    let service = &document["services"][0];

    assert_eq!(service["service_entry_id"], "chatgpt-plan");
    assert_eq!(service["display_name"], "ChatGPT Plan");
    assert_eq!(service["base_url"], "https://api.agent.space/v1");
    assert_eq!(
        service["protocol_routes"][0]["protocol_profile_id"],
        "openai-codex-responses.v1"
    );
    assert_eq!(service["protocol_routes"][0]["endpoint_path"], "/responses");
    assert_eq!(
        service["protocol_routes"][0]["responses_features"]["recovery"],
        "agent_space_result_and_usage_reconciliation_v1"
    );

    let model_ids = service["model_catalog"]["models"]
        .as_array()
        .expect("AgentSpace model catalog should be an array")
        .iter()
        .map(|model| {
            model["model_id"]
                .as_str()
                .expect("AgentSpace catalog model should have an id")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        model_ids,
        BTreeSet::from([
            String::from("gpt-5.6-luna"),
            String::from("gpt-5.6-terra"),
            String::from("gpt-5.6-sol"),
            String::from("gpt-6-astra"),
            String::from("gpt-image-2"),
        ])
    );
    assert!(
        document["model_rules"]
            .as_array()
            .expect("AgentSpace model rules should be an array")
            .iter()
            .all(|rule| rule.get("codex_behavior").is_none()),
        "AgentSpace must not inherit OpenAI Codex behavior from matching model names"
    );

    let image_target = document["tool_targets"]
        .as_array()
        .expect("AgentSpace tool_targets should be an array")
        .iter()
        .find(|target| target["target_id"] == "chatgpt-plan.gpt-image-2.generate")
        .expect("AgentSpace GPT Image 2 target should exist");
    assert_eq!(
        image_target["execution"]["endpoint_path"],
        "/images/generations"
    );
    assert_eq!(
        image_target["execution"]["execution_kind"],
        "http_json_sync"
    );
    assert_eq!(
        image_target["input_schema"]["properties"]
            .as_object()
            .map(|properties| { properties.keys().cloned().collect::<BTreeSet<_>>() }),
        Some(BTreeSet::from([
            String::from("background"),
            String::from("model"),
            String::from("n"),
            String::from("prompt"),
            String::from("quality"),
            String::from("size"),
        ]))
    );
    assert!(
        image_target["input_schema"]["properties"]
            .get("stream")
            .is_none()
    );
    assert!(
        image_target["input_schema"]["properties"]
            .get("response_format")
            .is_none()
    );
}

/// Verifies only Gemini model rules opt into strict tool-schema projection in system manifests.
/// 验证系统清单中只有 Gemini 模型规则显式启用严格工具 Schema 投影。
#[test]
fn bundled_tool_schema_projection_is_strict_only_for_gemini_rules() {
    for (provider_id, text) in load_bundled_manifests() {
        let manifest: vulcan_provider_adapters::ProviderManifestV1 = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("parse bundled manifest `{provider_id}`: {error}"));
        assert_eq!(
            manifest.model_defaults.tool_schema_projection,
            vulcan_provider_adapters::ToolSchemaProjectionMode::Lightweight,
            "system provider `{provider_id}` must leave the provider default lightweight"
        );
        for rule in manifest.model_rules {
            let is_gemini = rule.matcher.value.to_ascii_lowercase().contains("gemini");
            if is_gemini {
                assert_eq!(
                    rule.tool_schema_projection,
                    Some(vulcan_provider_adapters::ToolSchemaProjectionMode::Strict),
                    "Gemini rule `{}` in `{provider_id}` must declare strict projection",
                    rule.rule_id
                );
            } else {
                assert_eq!(
                    rule.tool_schema_projection, None,
                    "non-Gemini rule `{}` in `{provider_id}` must inherit lightweight projection",
                    rule.rule_id
                );
            }
        }
    }
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

/// Verifies MiniMax M2.x and M3 expose only their exact provider-owned reasoning contracts.
/// 验证 MiniMax M2.x 与 M3 只公开其精确供应商自有思考契约。
#[test]
fn minimax_manifest_reasoning_contracts_are_exact() {
    // path identifies the single authoritative MiniMax bundled manifest.
    // path 标识唯一权威的 MiniMax 内置清单。
    let path = bundled_manifests_directory().join("minimax.json");
    let text = std::fs::read_to_string(&path).expect("read MiniMax bundled manifest");
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("MiniMax bundled manifest must compile");
    // m2 owns one provider-adaptive level and cannot be disabled.
    // m2 只持有一个供应商自适应档位且不能关闭。
    let m2 = compiled
        .rule_for_model("global", None, "MiniMax-M2.7-highspeed")
        .and_then(|rule| rule.capabilities.as_ref())
        .expect("MiniMax M2.x capabilities must exist");
    assert_eq!(
        m2.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::ProviderAdaptive
    );
    assert_eq!(
        m2.reasoning_efforts,
        [vulcan_ai_contract::ReasoningEffort::Adaptive]
    );
    // m3 owns exactly the off/adaptive toggle and no OpenAI strength levels.
    // m3 精确持有关闭/自动开关，不包含 OpenAI 强度档位。
    let m3 = compiled
        .rule_for_model("global", None, "MiniMax-M3")
        .and_then(|rule| rule.capabilities.as_ref())
        .expect("MiniMax M3 capabilities must exist");
    assert_eq!(
        m3.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::Toggle
    );
    assert_eq!(
        m3.reasoning_efforts,
        [
            vulcan_ai_contract::ReasoningEffort::None,
            vulcan_ai_contract::ReasoningEffort::Adaptive,
        ]
    );
}

/// Verifies MiniMax image understanding mirrors the official mmx CLI image-delivery contract.
/// 验证 MiniMax 图片理解与官方 mmx CLI 图片投递契约保持一致。
///
/// # Returns
/// 返回值
/// Returns normally only when the bundled operation declares image input and every supported MIME mapping.
/// 仅当内置操作声明图片输入并包含全部受支持 MIME 映射时正常返回。
#[test]
fn minimax_image_understanding_matches_official_mmx_cli_contract() {
    // path identifies the single authoritative MiniMax bundled manifest.
    // path 标识唯一权威的 MiniMax 内置清单。
    let path = bundled_manifests_directory().join("minimax.json");
    // text preserves the exact published document for production compiler validation.
    // text 保留用于生产编译器校验的精确发布文档。
    let text = std::fs::read_to_string(&path).expect("read MiniMax bundled manifest");
    // compiled exposes the same resolved operation and upload channel used at runtime.
    // compiled 公开运行时使用的同一已解析操作与上传通道。
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("MiniMax bundled manifest must compile");
    // operation owns the fixed single-image understanding facade.
    // operation 持有固定的单图理解外壳。
    let operation = compiled
        .operations
        .iter()
        .find(|operation| operation.operation_id == "minimax.mini_max_coding_plan_vision")
        .expect("MiniMax Coding Plan vision operation must exist");
    assert_eq!(
        operation.input_resources,
        [vulcan_ai_contract::ResourceKind::Image],
        "MiniMax image understanding must declare its consumed modality"
    );
    // channel is the exact Data-URI materialization route used by image_path.
    // channel 是 image_path 使用的精确 Data URI 物化路由。
    let channel = compiled
        .upload_channels
        .iter()
        .find(|channel| channel.upload_channel_id == "inline.image-data-url")
        .expect("MiniMax image Data-URI channel must exist");
    // expected_mime_types mirrors the image extension map in current mmx CLI source.
    // expected_mime_types 对齐当前 mmx CLI 源码中的图片扩展名映射。
    let expected_mime_types = [
        "image/jpeg",
        "image/png",
        "image/webp",
        "image/heic",
        "image/heif",
    ];
    assert_eq!(channel.accepted_mime_types, expected_mime_types);
    assert_eq!(channel.max_bytes, Some(50 * 1024 * 1024));
    // binding proves canonical image_path is removed only after it becomes upstream image_url.
    // binding 证明规范 image_path 仅在转为上游 image_url 后才会移除。
    let binding = operation
        .upload_bindings
        .first()
        .expect("MiniMax Coding Plan vision upload binding must exist");
    assert_eq!(binding.source_path, "/image_path");
    assert_eq!(binding.target_path, "/image_url");
    for (extension, mime_type) in [
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("png", "image/png"),
        ("webp", "image/webp"),
        ("heic", "image/heic"),
        ("heif", "image/heif"),
    ] {
        assert_eq!(
            binding
                .mime_types_by_extension
                .get(extension)
                .map(String::as_str),
            Some(mime_type),
            "MiniMax image MIME mapping drifted for `{extension}`"
        );
    }
}

/// Verifies MiniMax synchronous speech uses bounded URL delivery instead of embedding hex audio.
/// 验证 MiniMax 同步语音使用有界 URL 交付，而不是在响应中嵌入十六进制音频。
///
/// # Returns
/// 返回值
/// Returns normally only when every bundled MiniMax speech target requests and decodes URL audio.
/// 仅当全部 MiniMax 内置语音目标都请求并解码 URL 音频时正常返回。
#[test]
fn minimax_speech_uses_url_delivery_for_long_synchronous_output() {
    // path identifies the single authoritative MiniMax bundled manifest.
    // path 标识唯一权威的 MiniMax 内置清单。
    let path = bundled_manifests_directory().join("minimax.json");
    // text preserves the exact document for both raw profile and compiled target assertions.
    // text 保留精确文档，供原始 Profile 与已编译目标共同断言。
    let text = std::fs::read_to_string(&path).expect("read MiniMax bundled manifest");
    // document exposes the literal provider wire format selected by the request profile.
    // document 公开请求 Profile 选择的供应商线格式字面量。
    let document: Value = serde_json::from_str(&text).expect("parse MiniMax bundled manifest");
    // request_profile owns the fixed non-streaming MiniMax speech request projection.
    // request_profile 持有固定的 MiniMax 非流式语音请求投影。
    let request_profile = document["request_profiles"]
        .as_array()
        .expect("MiniMax request profiles")
        .iter()
        .find(|profile| profile["profile_id"] == "minimax.speech.request.v1")
        .expect("MiniMax speech request profile must exist");
    assert_eq!(request_profile["payload"]["revision"], 2);
    assert_eq!(
        request_profile["payload"]["steps"][0]["template"]["output_format"], "url",
        "MiniMax speech must not return size-amplified hex audio in JSON"
    );
    // compiled exposes the exact target contracts consumed by the production executor.
    // compiled 公开生产执行器消费的精确目标契约。
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("MiniMax bundled manifest must compile");
    // expected_target_ids freezes the complete global and China speech target inventory.
    // expected_target_ids 冻结全球与中国区完整语音目标清单。
    let expected_target_ids = [
        "global.speech-2.8-hd",
        "global.speech-2.8-turbo",
        "global.speech-2.6",
        "global.speech-02",
        "cn.speech-2.8-hd",
        "cn.speech-2.8-turbo",
        "cn.speech-2.6",
        "cn.speech-02",
    ];
    // expected_host_suffixes reuses the reviewed MiniMax and provider-storage authorities.
    // expected_host_suffixes 复用已审查的 MiniMax 与供应商存储域名权限。
    let expected_host_suffixes = [
        "api.minimax.io",
        "api.minimaxi.com",
        ".minimax.chat",
        ".hailuoai.com",
        ".aliyuncs.com",
    ];
    for target_id in expected_target_ids {
        // target is one production speech contract selected by the shared operation.
        // target 是共享操作选择的一份生产语音契约。
        let target = compiled
            .tool_targets
            .iter()
            .find(|candidate| candidate.target_id == target_id)
            .unwrap_or_else(|| panic!("missing MiniMax speech target `{target_id}`"));
        // decoder is the primary generated-audio carrier; subtitles remain a separate optional resource.
        // decoder 是主要生成音频载体；字幕继续作为独立可选资源。
        let decoder = target
            .contract
            .output
            .resource_decoders
            .first()
            .expect("MiniMax speech target must declare an audio decoder");
        assert_eq!(decoder.source_path, "/data/audio");
        assert_eq!(
            decoder.encoding,
            vulcan_provider_adapters::ProviderToolResourceEncoding::PublicHttpUrl
        );
        assert_eq!(
            decoder
                .download_host_suffixes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            expected_host_suffixes
        );
        assert_eq!(
            decoder.max_bytes, None,
            "URL resources are bounded by the host downloader instead of an inline decoder"
        );
    }
}

/// Verifies MiniMax no longer publishes any music capability after the feature moved providers.
/// 验证音乐能力迁移到其他供应商后，MiniMax 不再发布任何音乐能力。
#[test]
fn minimax_manifest_excludes_music_capability() {
    // path identifies the single authoritative MiniMax bundled manifest.
    // path 标识唯一权威的 MiniMax 内置清单。
    let path = bundled_manifests_directory().join("minimax.json");
    // text is the exact published document whose removed music surface must stay absent.
    // text 是必须持续排除已删除音乐能力面的精确发布文档。
    let text = std::fs::read_to_string(&path).expect("read MiniMax bundled manifest");
    assert!(
        !text.contains("music_generate") && !text.contains("minimax.music"),
        "MiniMax must not retain music operations or profiles"
    );
    // compiled is the same runtime authority and prevents a hidden music target from surviving compilation.
    // compiled 是运行时使用的同一权威，防止隐藏音乐目标在编译后继续存在。
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("MiniMax bundled manifest must compile");
    assert!(
        compiled
            .operations
            .iter()
            .all(|operation| operation.operation != "music_generate"),
        "compiled MiniMax operations must not expose music generation"
    );
}

/// Verifies DeepSeek exposes only the current official identities with exact protocol scopes.
/// 验证 DeepSeek 仅公开当前官方模型身份及其精确协议作用域。
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
        BTreeSet::from(["deepseek-flash", "deepseek-v4-pro"])
    );
    assert_eq!(
        compiled.catalog_membership_policy,
        vulcan_provider_adapters::CatalogMembershipPolicy::DeclaredOnly
    );
    assert_eq!(
        compiled.unknown_model_policy,
        vulcan_provider_adapters::UnknownModelPolicy::DeclaredRuleRequired
    );
    // retiredModelIds contains identities merged into Flash or removed from the official catalog.
    // retiredModelIds 包含已合并至 Flash 或已从官方目录移除的模型身份。
    let retired_model_ids = [
        "deepseek-chat",
        "deepseek-reasoner",
        "deepseek-v4-flash-preview",
        "deepseek-v4-flash",
        "deepseek-v4.1-flash-expires-on-0910",
        "deepseek-v4-flash-vision-exp",
    ];
    // retiredModelId checks every removed identity against the compiled exact-rule table.
    // retiredModelId 逐一检查所有已移除身份不会命中编译后的精确规则表。
    for retired_model_id in retired_model_ids {
        assert!(
            compiled
                .rule_for_model("official-api", None, retired_model_id)
                .is_none(),
            "retired DeepSeek model {retired_model_id} must not remain declared"
        );
    }
    // flash is the current V4.1 official identity of the static catalog; v4-pro is the retained V4 identity.
    // flash 是静态目录内的当前 V4.1 官方身份；v4-pro 是保留在用的 V4 身份。
    let flash = compiled
        .rule_for_model("official-api", None, "deepseek-flash")
        .expect("DeepSeek V4.1 Flash rule must exist");
    assert_eq!(flash.display_name.as_deref(), Some("DeepSeek V4.1 Flash"));
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
    // flashCapabilities is the model-level authority for reasoning limits and image input.
    // flashCapabilities 是思考限制与图片输入的模型级权威。
    let flash_capabilities = flash
        .capabilities
        .as_ref()
        .expect("DeepSeek V4.1 Flash capabilities must exist");
    assert_eq!(
        flash_capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::Image]
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
    assert_eq!(flash_capabilities.context_limit, Some(1_000_000));
    assert_eq!(flash_capabilities.output_limit, Some(393_216));
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
    // imageConstraint freezes the limits representable by the current attachment contract.
    // imageConstraint 冻结当前附件契约能够表达的图片限制。
    let image_constraint = flash
        .input_resource_constraints
        .first()
        .expect("Flash must declare an image input constraint");
    assert_eq!(flash.input_resource_constraints.len(), 1);
    assert_eq!(
        image_constraint.resource_kind,
        vulcan_ai_contract::ResourceKind::Image
    );
    assert_eq!(
        image_constraint.accepted_mime_types,
        vec!["image/jpeg", "image/png", "image/gif", "image/webp",]
    );
    assert_eq!(image_constraint.max_bytes, Some(32 * 1024 * 1024));
    assert_eq!(image_constraint.max_count, Some(600));
    assert_eq!(
        image_constraint.reference_policy,
        vulcan_provider_adapters::ResourceReferencePolicy::LocalOnly
    );
    assert!(image_constraint.upload_channel_refs.is_empty());
    assert_eq!(
        flash.provenance.provenance_ref.as_deref(),
        Some("https://api-docs.deepseek.com/zh-cn/guides/vision")
    );
    // v4-pro is its own current identity and must not be mirrored against flash: the official
    // provider retired v4-flash, and the two surviving models differ in input modalities.
    // v4-pro 是独立的在用身份，不得与 flash 镜像：官方已下线 v4-flash，且现存两个模型的输入模态不同。
    let pro = compiled
        .rule_for_model("official-api", None, "deepseek-v4-pro")
        .expect("DeepSeek V4 Pro rule must exist");
    assert_eq!(pro.display_name.as_deref(), Some("DeepSeek V4 Pro"));
    assert_eq!(pro.selected_protocol.as_deref(), Some("openai-responses.v1"));
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
    // proCapabilities stays text-only while V4.1 Flash carries image understanding.
    // proCapabilities 保持纯文本，而 V4.1 Flash 携带图片理解能力。
    let pro_capabilities = pro
        .capabilities
        .as_ref()
        .expect("DeepSeek V4 Pro capabilities must exist");
    assert!(
        pro_capabilities.input_resources.is_empty(),
        "V4 Pro must not declare image input while V4.1 Flash declares image understanding"
    );
    assert!(pro.input_resource_constraints.is_empty());
    assert_eq!(pro_capabilities.context_limit, Some(1_000_000));
    assert_eq!(pro_capabilities.output_limit, Some(393_216));
    assert_eq!(
        pro_capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::None,
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::Low,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
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
    let service = bundled_service_entry(&document, "go-api");
    assert_eq!(
        service.pointer("/credential_profiles/0/default_usage_scopes"),
        Some(&serde_json::json!(["interactive_agent"])),
        "OpenCode Go must keep conversation-only as its initial default, not an internal-use ceiling"
    );
    // routes is the only OpenCode Go conversation service route collection.
    // routes 是唯一的 OpenCode Go 会话服务路由集合。
    let routes = service
        .pointer("/protocol_routes")
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
    assert_eq!(
        service.pointer("/session_header"),
        Some(&serde_json::json!("x-opencode-session")),
        "OpenCode Go must project the current session through its configured header"
    );

    // compiled is the same resolved authority used by discovery and execution.
    // compiled 是发现与执行共同使用的同一份解析权威。
    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("bundled OpenCode Go manifest must compile");
    assert_eq!(
        compiled
            .provider()
            .service_entries
            .first()
            .and_then(|entry| entry.session_header.as_deref()),
        Some("x-opencode-session"),
        "compiled OpenCode Go service entry must retain its session header"
    );
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

    // visionFlash must remain execution-equivalent to Flash except for exact local image input.
    // visionFlash 除精确本地图片输入外，必须与 Flash 保持执行等价。
    let vision_flash = compiled
        .rule_for_model("go-api", None, "deepseek-v4-flash-vision-exp")
        .expect("OpenCode Go DeepSeek V4 Flash Vision Exp rule must exist");
    assert_eq!(
        &vision_flash.supported_protocols,
        &flash.supported_protocols
    );
    assert_eq!(&vision_flash.selected_protocol, &flash.selected_protocol);
    assert_eq!(
        &vision_flash.protocol_capabilities,
        &flash.protocol_capabilities
    );
    assert_eq!(
        &vision_flash.request_profile_refs,
        &flash.request_profile_refs
    );
    assert_eq!(&vision_flash.model_parameters, &flash.model_parameters);
    // visionFlashCapabilities normalizes only the intentional modality and provenance differences.
    // visionFlashCapabilities 仅规范化有意存在的模态与证据来源差异。
    let mut vision_flash_capabilities = vision_flash
        .capabilities
        .clone()
        .expect("Vision Flash capabilities must exist");
    let flash_capabilities = flash
        .capabilities
        .as_ref()
        .expect("Flash capabilities must exist");
    assert_eq!(
        vision_flash_capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::Image]
    );
    vision_flash_capabilities.input_resources = flash_capabilities.input_resources.clone();
    vision_flash_capabilities.evidence_revision = flash_capabilities.evidence_revision.clone();
    assert_eq!(&vision_flash_capabilities, flash_capabilities);
    assert_eq!(vision_flash.input_resource_constraints.len(), 1);
    let vision_image_constraint = &vision_flash.input_resource_constraints[0];
    assert_eq!(
        vision_image_constraint.resource_kind,
        vulcan_ai_contract::ResourceKind::Image
    );
    assert_eq!(
        vision_image_constraint.accepted_mime_types,
        vec!["image/jpeg", "image/png", "image/gif", "image/webp"]
    );
    assert_eq!(vision_image_constraint.max_bytes, Some(32 * 1024 * 1024));
    assert_eq!(vision_image_constraint.max_count, Some(600));
    assert_eq!(
        vision_image_constraint.reference_policy,
        vulcan_provider_adapters::ResourceReferencePolicy::LocalOnly
    );
    assert!(vision_image_constraint.upload_channel_refs.is_empty());

    // muse is a Responses-only 1 Mi-token text-and-image model with fixed-on selectable reasoning.
    // muse 是仅走 Responses、上下文 1 Mi Token 且思考固定开启并可选档位的文本图片模型。
    let muse = compiled
        .rule_for_model("go-api", None, "muse-spark-1.2-contributor")
        .expect("OpenCode Go Muse Spark 1.2 Contributor rule must exist");
    assert_eq!(
        muse.supported_protocols,
        vec![String::from("openai-responses.v1")]
    );
    assert_eq!(
        muse.selected_protocol.as_deref(),
        Some("openai-responses.v1")
    );
    let muse_capabilities = muse
        .capabilities
        .as_ref()
        .expect("Muse capabilities must exist");
    assert_eq!(
        muse_capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::Image]
    );
    assert_eq!(
        muse_capabilities.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        muse_capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::Minimal,
            vulcan_ai_contract::ReasoningEffort::Low,
            vulcan_ai_contract::ReasoningEffort::Medium,
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::XHigh,
        ]
    );
    assert_eq!(muse_capabilities.context_limit, Some(1_048_576));
    assert_eq!(muse_capabilities.output_limit, Some(131_072));
    assert_eq!(muse.input_resource_constraints.len(), 1);
    let muse_image_constraint = &muse.input_resource_constraints[0];
    assert_eq!(
        muse_image_constraint.resource_kind,
        vulcan_ai_contract::ResourceKind::Image
    );
    assert_eq!(
        muse_image_constraint.accepted_mime_types,
        vec!["image/png", "image/jpeg", "image/gif", "image/webp"]
    );
    assert_eq!(muse_image_constraint.max_bytes, Some(20 * 1024 * 1024));
    assert_eq!(
        muse_image_constraint.reference_policy,
        vulcan_provider_adapters::ResourceReferencePolicy::LocalOnly
    );
    assert!(muse_image_constraint.upload_channel_refs.is_empty());

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

/// Verifies Token Rhythm freezes its static catalog, protocol routes, reasoning maps, headers, and image targets.
/// 验证基元律动固化静态目录、Chat 路由、思考映射、请求头与图片目标。
#[test]
fn tokenrhythm_manifest_freezes_the_reviewed_provider_contract() {
    // The source manifest is the only authority for this non-discovery system provider.
    // 源清单是此无动态发现系统供应商的唯一权威。
    let path = bundled_manifests_directory().join("tokenrhythm.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("read bundled manifest {}", path.display()));
    let document: Value = serde_json::from_str(&text).expect("parse Token Rhythm manifest JSON");
    let service = bundled_service_entry(&document, "public-api");

    assert_eq!(
        service.pointer("/base_url"),
        Some(&Value::String(String::from(
            "https://tokenrhythm.studio/v1"
        )))
    );
    assert_eq!(
        service.pointer("/request_headers/http-referer"),
        Some(&Value::String(String::from("https://opensquilla.ai")))
    );
    assert_eq!(
        service.pointer("/request_headers/x-title"),
        Some(&Value::String(String::from("OpenSquilla")))
    );
    // request_headers freezes provider-owned headers without overriding the shared User-Agent.
    // request_headers 冻结供应商自有请求头，且不覆盖共享 User-Agent。
    let request_headers = service["request_headers"]
        .as_object()
        .expect("Token Rhythm fixed request headers must exist");
    assert_eq!(
        request_headers.len(),
        2,
        "Token Rhythm must leave User-Agent ownership to the shared runtime"
    );
    assert_eq!(
        service.pointer("/model_catalog/source_policy"),
        Some(&Value::String(String::from("manifest_only")))
    );
    assert_eq!(
        service.pointer("/model_catalog/discovery_kind"),
        Some(&Value::String(String::from("manifest_static")))
    );
    let routes = service["protocol_routes"]
        .as_array()
        .expect("Token Rhythm protocol routes must exist");
    assert_eq!(
        routes.len(),
        3,
        "Token Rhythm exposes the reviewed LLM routes"
    );
    let route_by_protocol = |protocol_id: &str| {
        routes
            .iter()
            .find(|route| route["protocol_profile_id"] == protocol_id)
            .unwrap_or_else(|| panic!("missing Token Rhythm route `{protocol_id}`"))
    };
    assert_eq!(
        route_by_protocol("openai-chat-completions.v1")["endpoint_path"],
        "/chat/completions"
    );
    assert_eq!(
        route_by_protocol("anthropic-messages.v1")["endpoint_path"],
        "/messages"
    );
    assert_eq!(
        route_by_protocol("anthropic-messages.v1")["authentication"],
        "x_api_key"
    );
    assert_eq!(
        route_by_protocol("anthropic-messages.v1")["request_headers"]["anthropic-version"],
        "2023-06-01"
    );
    assert_eq!(
        route_by_protocol("openai-responses.v1")["endpoint_path"],
        "/responses"
    );

    let compiled =
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .expect("bundled Token Rhythm manifest must compile");
    let declared_model_ids = compiled
        .catalog_models
        .get("public-api")
        .expect("Token Rhythm must declare its complete static inventory")
        .iter()
        .map(|model| model.model_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        declared_model_ids,
        TOKEN_RHYTHM_MODELS.iter().copied().collect::<BTreeSet<_>>()
    );
    assert!(
        compiled
            .rule_instances
            .iter()
            .all(|rule| rule.matcher.kind == vulcan_provider_adapters::MatcherKind::Exact)
    );

    let protocol_matrix = [
        ("deepseek-v4-flash", "anthropic-messages.v1"),
        ("deepseek-v4-pro", "anthropic-messages.v1"),
        ("deepseek-v4-flash-0731", "openai-responses.v1"),
        ("deepseek-v4-pro-0813", "anthropic-messages.v1"),
        ("glm-5", "anthropic-messages.v1"),
        ("glm-5.1", "anthropic-messages.v1"),
        ("glm-5.2", "anthropic-messages.v1"),
        ("minimax-m2.5", "anthropic-messages.v1"),
        ("minimax-m2.7", "anthropic-messages.v1"),
        ("kimi-k2.5", "anthropic-messages.v1"),
        ("kimi-k2.6", "anthropic-messages.v1"),
        ("kimi-k2.7-code", "openai-chat-completions.v1"),
        ("mimo-v2.5-pro", "anthropic-messages.v1"),
        ("qwen3.7-max", "openai-responses.v1"),
        ("qwen3.8-max", "openai-responses.v1"),
        ("seed-2.1-turbo", "openai-responses.v1"),
        ("seed-2.1-pro", "openai-responses.v1"),
    ];
    for (model_id, expected_protocol) in protocol_matrix {
        let rule = compiled
            .rule_for_model("public-api", Some("api-key"), model_id)
            .unwrap_or_else(|| panic!("missing Token Rhythm protocol rule `{model_id}`"));
        assert_eq!(
            rule.supported_protocols,
            vec![String::from(expected_protocol)],
            "Token Rhythm supported protocol drifted for `{model_id}`"
        );
        assert_eq!(
            rule.selected_protocol.as_deref(),
            Some(expected_protocol),
            "Token Rhythm selected protocol drifted for `{model_id}`"
        );
        if expected_protocol != "openai-chat-completions.v1" {
            assert!(
                rule.request_profile_refs.is_empty(),
                "non-Chat Token Rhythm model `{model_id}` must not inherit Chat request profiles"
            );
        }
    }
    let kimi_chat = compiled
        .rule_for_model("public-api", Some("api-key"), "kimi-k2.7-code")
        .expect("Kimi K2.7 Code Chat rule must exist");
    assert_eq!(
        kimi_chat.request_profile_refs,
        vec![String::from("tokenrhythm.chat.reasoning.fixed.v1")]
    );
    let flash_0731 = compiled
        .rule_for_model("public-api", Some("api-key"), "deepseek-v4-flash-0731")
        .expect("DeepSeek V4 Flash 0731 Responses rule must exist");
    assert_eq!(
        flash_0731
            .protocol_capabilities
            .get("openai-responses.v1")
            .expect("DeepSeek V4 Flash 0731 Responses capabilities must exist")
            .provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch]
    );

    // The reviewed model families freeze their provider-specific reasoning control matrix.
    // 已审核的模型家族固化各自供应商特定的思考控制矩阵。
    let reasoning_matrix = [
        (
            [
                "deepseek-v4-flash",
                "deepseek-v4-pro",
                "qwen3.7-max",
                "qwen3.8-max",
                "seed-2.1-turbo",
                "seed-2.1-pro",
                "mimo-v2.5-pro",
            ]
            .as_slice(),
            vulcan_ai_contract::ReasoningControlMode::Toggle,
            vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::High,
            ],
        ),
        (
            ["deepseek-v4-flash-0731", "deepseek-v4-pro-0813"].as_slice(),
            vulcan_ai_contract::ReasoningControlMode::EffortSelection,
            vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
        ),
        (
            [
                "glm-5",
                "glm-5.1",
                "minimax-m2.5",
                "minimax-m2.7",
                "kimi-k2.5",
                "kimi-k2.6",
                "kimi-k2.7-code",
            ]
            .as_slice(),
            vulcan_ai_contract::ReasoningControlMode::FixedOn,
            Vec::new(),
        ),
        (
            ["glm-5.2"].as_slice(),
            vulcan_ai_contract::ReasoningControlMode::FixedOn,
            vec![
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
        ),
    ];
    for (model_ids, expected_control, expected_efforts) in reasoning_matrix {
        for model_id in model_ids {
            let rule = compiled
                .rule_for_model("public-api", Some("api-key"), model_id)
                .unwrap_or_else(|| panic!("missing Token Rhythm rule `{model_id}`"));
            let capabilities = rule
                .capabilities
                .as_ref()
                .expect("Token Rhythm reasoning capabilities");
            assert_eq!(
                capabilities.reasoning_control, expected_control,
                "Token Rhythm reasoning control drifted for `{model_id}`"
            );
            assert_eq!(
                capabilities.reasoning_efforts, expected_efforts,
                "Token Rhythm reasoning effort matrix drifted for `{model_id}`"
            );
        }
    }

    // The two reviewed image models are opt-in generation-only extensions.
    // 两个已审查图片模型都是默认关闭且仅用于生成的扩展。
    for model_id in ["qwen-image-2.0", "wan2.7-image"] {
        let rule = compiled
            .rule_for_model("public-api", Some("api-key"), model_id)
            .unwrap_or_else(|| panic!("missing Token Rhythm image rule `{model_id}`"));
        assert!(!rule.default_enabled);
        let capabilities = rule.capabilities.as_ref().expect("image capabilities");
        assert_eq!(
            capabilities.operations,
            vec![vulcan_ai_contract::AiOperation::ImageGenerate]
        );
        assert_eq!(
            capabilities.reasoning_control,
            vulcan_ai_contract::ReasoningControlMode::FixedOff
        );
    }
    assert_eq!(compiled.operations.len(), 1);
    assert_eq!(compiled.tool_targets.len(), 2);
    // expected_image_targets binds each public extension identity to its exact upstream model.
    // expected_image_targets 把每个公共扩展身份绑定到精确的上游模型。
    let expected_image_targets = [
        ("qwen-image-2-0.generate", "qwen-image-2.0"),
        ("wan2-7-image.generate", "wan2.7-image"),
    ];
    for (target_id, model_id) in expected_image_targets {
        // target is the compiled production contract consumed by the generic executor.
        // target 是通用执行器消费的已编译生产契约。
        let target = compiled
            .tool_targets
            .iter()
            .find(|candidate| candidate.target_id == target_id)
            .unwrap_or_else(|| panic!("missing Token Rhythm image target `{target_id}`"));
        assert_eq!(target.upstream_target_id, model_id);
        assert_eq!(target.execution.endpoint_path, "/images/generations");
        assert_eq!(
            target.contract.input_schema["properties"]["model"]["const"],
            model_id
        );
        assert_eq!(
            target.contract.input_schema["properties"]["size"]["const"],
            "1024x1024"
        );
        assert_eq!(target.contract.input_schema["properties"]["n"]["const"], 1);
        // decoder freezes the reviewed response pointer, MIME, and exact OSS download authority.
        // decoder 冻结已审查的响应指针、MIME 与精确 OSS 下载权限。
        let [decoder] = target.contract.output.resource_decoders.as_slice() else {
            panic!("Token Rhythm image target `{target_id}` must expose one decoder");
        };
        assert_eq!(decoder.source_path, "/data/0/url");
        assert_eq!(
            decoder.encoding,
            vulcan_provider_adapters::ProviderToolResourceEncoding::HttpsUrl
        );
        assert_eq!(decoder.mime_type.as_deref(), Some("image/png"));
        assert_eq!(
            decoder.download_host_suffixes,
            vec![String::from(
                "tokenrhythm-prod-backup-oss.oss-cn-beijing.aliyuncs.com"
            )]
        );
    }

    // Standard and Flash profiles preserve the reviewed fixed effort projection.
    // 标准与 Flash Profile 保留已审查的固定档位投影。
    let reasoning_mapping = |profile_id: &str| {
        document["request_profiles"]
            .as_array()
            .expect("Token Rhythm request profiles")
            .iter()
            .find(|profile| profile["profile_id"] == profile_id)
            .expect("Token Rhythm reasoning profile")["payload"]["steps"]
            .as_array()
            .expect("Token Rhythm profile steps")
            .iter()
            .find(|step| step["op"] == "enum_map" && step["path"] == "/reasoning_effort")
            .expect("Token Rhythm reasoning enum map")["mapping"]
            .clone()
    };
    let flash_mapping = reasoning_mapping("tokenrhythm.chat.reasoning.flash.v1");
    let glm52_mapping = reasoning_mapping("tokenrhythm.chat.reasoning.glm52.v1");
    assert_eq!(flash_mapping["low"], "low");
    assert_eq!(flash_mapping["high"], "high");
    assert_eq!(flash_mapping["max"], "max");
    assert_eq!(glm52_mapping["high"], "high");
    assert_eq!(glm52_mapping["max"], "max");
    let standard_thinking_mapping = document["request_profiles"]
        .as_array()
        .expect("Token Rhythm request profiles")
        .iter()
        .find(|profile| profile["profile_id"] == "tokenrhythm.chat.reasoning.standard.v1")
        .expect("Token Rhythm standard reasoning profile")["payload"]["steps"]
        .as_array()
        .expect("Token Rhythm standard profile steps")
        .iter()
        .find(|step| step["op"] == "enum_map" && step["path"] == "/thinking")
        .expect("Token Rhythm standard thinking enum map")["mapping"]
        .clone();
    assert_eq!(
        standard_thinking_mapping["none"]["type"], "disabled",
        "standard Chat must explicitly disable thinking for none"
    );
    assert_eq!(
        standard_thinking_mapping["high"]["type"], "enabled",
        "standard Chat must explicitly enable thinking for high"
    );
    let standard_profile = document["request_profiles"]
        .as_array()
        .expect("Token Rhythm request profiles")
        .iter()
        .find(|profile| profile["profile_id"] == "tokenrhythm.chat.reasoning.standard.v1")
        .expect("Token Rhythm standard reasoning profile");
    assert!(
        !standard_profile["payload"]["steps"]
            .as_array()
            .expect("Token Rhythm standard profile steps")
            .iter()
            .any(|step| step["op"] == "enum_map" && step["path"] == "/reasoning_effort"),
        "toggle-only models must not receive a provider effort value"
    );
    let fixed_profile = document["request_profiles"]
        .as_array()
        .expect("Token Rhythm request profiles")
        .iter()
        .find(|profile| profile["profile_id"] == "tokenrhythm.chat.reasoning.fixed.v1")
        .expect("Token Rhythm fixed reasoning profile");
    assert_eq!(
        fixed_profile["payload"]["steps"],
        serde_json::json!([{"op": "set_literal", "path": "/stream_options/include_usage", "value": true}])
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
    let service = bundled_service_entry(&document, "provider-api");
    assert_eq!(
        service.pointer("/base_url"),
        Some(&Value::String(String::from(
            "https://api.commandcode.ai/provider/v1"
        )))
    );
    assert_eq!(
        service.pointer("/request_projector"),
        Some(&Value::String(String::from("direct")))
    );
    assert_eq!(
        service.pointer("/credential_profiles/0/key_prefix"),
        None,
        "CommandCode validates API keys remotely instead of enforcing a local prefix"
    );
    assert_eq!(
        service.pointer("/credential_profiles/1/key_prefix"),
        None,
        "browser-delivered CommandCode keys must not be rejected by an obsolete prefix rule"
    );
    assert_eq!(
        service.pointer("/credential_profiles/0/account_usage_query"),
        Some(&Value::String(String::from("command_code_usage")))
    );
    let routes = service
        .pointer("/protocol_routes")
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

    // textOnlyModels is the exact online subset of the CLI 1.54.1 text-only registry.
    // textOnlyModels 是 CLI 1.54.1 纯文本注册表中的在线精确子集。
    let text_only_models = BTreeSet::from([
        "deepseek/deepseek-v4-pro",
        "deepseek/deepseek-v4-flash",
        "deepseek/deepseek-v4-flash-fast",
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
        "tencent/hy4-preview",
        "meituan/LongCat-2.0:free",
        "inclusionai/ling-3.0-flash-sante:free",
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
            | "tencent/hy4-preview"
            | "google/gemini-3.8-flash"
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
            "deepseek/deepseek-v4-pro"
            | "deepseek/deepseek-v4-flash"
            | "deepseek/deepseek-v4-flash-vision-exp"
            | "zai-org/GLM-5.2" => vec![
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
            "zai-org/GLM-5.3"
            | "z-ai/glm-5.3-flash"
            | "deepseek/deepseek-v4-flash-fast"
            | "deepseek/deepseek-v4.1-flash" => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
            "Qwen/Qwen3.8-Max"
            | "Qwen/Qwen3.8-Max-0902"
            | "Qwen/Qwen3.8-27B"
            | "Qwen/Qwen3.8-Flash" => vec![
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::XHigh,
            ],
            "sakana/fugu-ultra" => vec![
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::XHigh,
            ],
            "xai/grok-4.6" | "meta/muse-spark-1.3-contributor" => vec![
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
    // verifiedLimits pin the newly admitted API models and the corrected GPT-5.5 context.
    // verifiedLimits 固定新纳入 API 模型的限制以及修正后的 GPT-5.5 上下文。
    let verified_limits = [
        ("claude-fable-5-1", 1_000_000, None),
        ("deepseek/deepseek-v4-flash-vision-exp", 1_000_000, None),
        ("deepseek/deepseek-v4-flash-fast", 1_000_000, None),
        ("deepseek/deepseek-v4.1-flash", 1_000_000, None),
        ("z-ai/glm-5.3-flash", 1_048_576, Some(131_072)),
        ("Qwen/Qwen3.8-Max-0902", 1_000_000, None),
        ("Qwen/Qwen3.8-27B", 262_144, Some(32_768)),
        ("Qwen/Qwen3.8-Flash", 1_000_000, None),
        ("meituan/LongCat-2.0:free", 1_048_576, None),
        ("tencent/hy4-preview", 1_048_576, None),
        ("google/gemini-3.8-flash", 1_000_000, None),
        (
            "inclusionai/ling-3.0-flash-sante:free",
            262_144,
            Some(32_768),
        ),
        ("meta/muse-spark-1.3", 1_048_576, None),
        ("meta/muse-spark-1.3-contributor", 1_048_576, None),
        ("gpt-5.5", 400_000, None),
    ];
    for (model_id, context_limit, output_limit) in verified_limits {
        let capabilities = compiled
            .rule_for_model("provider-api", None, model_id)
            .and_then(|rule| rule.capabilities.as_ref())
            .unwrap_or_else(|| panic!("missing CommandCode capabilities `{model_id}`"));
        assert_eq!(
            capabilities.context_limit,
            Some(context_limit),
            "{model_id}"
        );
        assert_eq!(capabilities.output_limit, output_limit, "{model_id}");
    }
    // fableRestrictions retain the official Max-and-above access rule for both credential paths.
    // fableRestrictions 为两种凭证路径保留官方 Max 及以上计划限制。
    for credential_profile in document["services"][0]["credential_profiles"]
        .as_array()
        .expect("CommandCode credential profiles")
    {
        for plan_id in ["individual-pro", "individual-pro-v1"] {
            assert!(
                credential_profile["account_model_access"]["plan_rules"][plan_id]
                    ["blocked_model_ids"]
                    .as_array()
                    .expect("CommandCode plan block list")
                    .contains(&Value::String(String::from("claude-fable-5-1"))),
                "Claude Fable 5.1 must be unavailable on {plan_id}"
            );
        }
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

/// Verifies APIKEY.FAN remains a strict intersection of group-visible models and local rules.
/// 验证 APIKEY.FAN 仍是分组可见模型与本地规则的严格交集。
#[test]
fn apikeyfan_manifest_is_a_closed_group_intersection() {
    // text is the exact bundled APIKEY.FAN authority read by production tests.
    // text 是生产测试读取的精确内置 APIKEY.FAN 权威。
    let text = load_bundled_manifests()
        .remove("apikeyfan")
        .expect("apikeyfan bundled manifest should exist");
    // document retains presence-sensitive rule and target declarations for structural guards.
    // document 保留存在性敏感的规则与目标声明用于结构守卫。
    let document: Value = serde_json::from_str(&text).expect("apikeyfan manifest should parse");
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
        .expect("apikeyfan protocol_routes should be an array")
        .iter()
        .find(|route| route["protocol_profile_id"] == "google.gemini.generate_content")
        .expect("apikeyfan Gemini protocol route should exist");
    assert_eq!(
        gemini_route["base_url_override"],
        "https://api.apikey.fan/v1beta"
    );
    assert_eq!(
        gemini_route["model_discovery_base_url_override"],
        "https://api.apikey.fan/v1"
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
        ("gpt-image-2", "openai-responses.v1"),
        ("gpt-image-2.5-flare", "openai-responses.v1"),
        ("gpt-image-2.5-sunburst", "openai-responses.v1"),
        ("grok-imagine-video-1.5", "openai-responses.v1"),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    // actualProtocols rejects prefixes and captures every exact discovery-intersection member.
    // actualProtocols 拒绝前缀并捕获每个精确发现交集成员。
    let actual_protocols = document["model_rules"]
        .as_array()
        .expect("apikeyfan model_rules should be an array")
        .iter()
        .map(|rule| {
            assert_eq!(rule["matcher"]["kind"], "exact");
            (
                rule["matcher"]["value"]
                    .as_str()
                    .expect("apikeyfan rule should declare an exact model id"),
                rule["selected_protocol"]
                    .as_str()
                    .expect("apikeyfan rule should bind one protocol"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual_protocols, expected_protocols);

    // The relay DeepSeek declarations are frozen against the relay's own reviewed facts.
    // The official DeepSeek manifest is deliberately not consulted: one provider's
    // configuration must not depend on another provider's configuration.
    // 中转的 DeepSeek 声明按中转自身的已验证事实冻结；刻意不查阅官方清单：
    // 任何供应商的配置都不应依赖另一个供应商的配置。
    for model_id in ["deepseek-v4-pro", "deepseek-v4-flash"] {
        // relayRule is the relay's own reviewed DeepSeek declaration.
        // relayRule 是中转自身已审核的 DeepSeek 声明。
        let relay_rule = document["model_rules"]
            .as_array()
            .expect("apikeyfan model_rules should be an array")
            .iter()
            .find(|rule| rule["matcher"]["value"] == model_id)
            .unwrap_or_else(|| panic!("APIKEY.FAN DeepSeek model rule `{model_id}` should exist"));
        assert_eq!(
            relay_rule["selected_protocol"],
            "openai-responses.v1",
            "APIKEY.FAN DeepSeek rule `{model_id}` must bind Responses"
        );
        assert_eq!(
            relay_rule["capabilities"]["reasoning_efforts"],
            serde_json::json!(["none", "high", "low", "max"]),
            "APIKEY.FAN reasoning levels for `{model_id}` must match the relay's reviewed declaration"
        );
    }

    // targets is the exact reviewed extension inventory; the unusable Grok edit id is absent.
    // targets 是精确已审核扩展清单；不可用的 Grok 编辑标识不存在。
    let targets = document["tool_targets"]
        .as_array()
        .expect("apikeyfan tool_targets should be an array");
    // targetIds protects the five verified media routes as a closed set.
    // targetIds 把五条已验证媒体路由保护为封闭集合。
    let target_ids = targets
        .iter()
        .map(|target| {
            target["target_id"]
                .as_str()
                .expect("apikeyfan target should have an id")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        target_ids,
        BTreeSet::from([
            "gpt-image-2.generate",
            "gpt-image-2.5-flare.generate",
            "gpt-image-2.5-sunburst.generate",
            "grok-imagine-image.generate",
            "grok-imagine-image-quality.generate",
            "grok-imagine-video-1.5.generate",
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
        .expect("apikeyfan operations should be an array")
        .iter()
        .find(|operation| operation["operation_id"] == "apikeyfan.grok-imagine-video.generate")
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
    // Sweep every bundled provider and assert per-provider structure, not a per-provider total: a
    // target count is data, so pinning it would force a test edit on every inventory change.
    // 扫描全部内置供应商并断言每个供应商的结构，而不是每个供应商的总数：总数属于数据，钉住它
    // 会让每次库存变化都要求改测试。
    for (provider_id, text) in load_bundled_manifests() {
        let compiled =
            vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
                .unwrap_or_else(|error| {
                    panic!("bundled manifest for `{provider_id}` must compile: {error}")
                });
        let manifest_ids = compiled
            .tool_targets
            .iter()
            .map(|target| target.target_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            manifest_ids.len(),
            compiled.tool_targets.len(),
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
/// This is the stable R0 guard against restoring a provider-named production adapter. It checks the
/// compiled authority rather than raw JSON spelling, and it asserts each target's adapter instead of
/// an inventory total, so it stays independent of how many providers and targets the generation has.
/// 这是防止恢复供应商命名生产适配器的稳定 R0 守卫。它检查编译后的权威而非原始 JSON 拼写，并
/// 逐个断言目标的适配器而不是库存总数，因此与代际包含多少供应商和目标无关。
#[test]
fn bundled_system_tool_targets_use_only_generic_execution() {
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
    assert!(
        target_count > 0,
        "the bundled generation must expose at least one production tool target to guard"
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
