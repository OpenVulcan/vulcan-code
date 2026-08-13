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
    assert_eq!(provider_count, 14);
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
/// 87-target migration. It checks the compiled authority rather than raw JSON spelling.
/// 这是防止 87 条目标迁移后恢复供应商命名生产适配器的稳定 R0 守卫；它检查编译后的
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
        target_count, 87,
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
