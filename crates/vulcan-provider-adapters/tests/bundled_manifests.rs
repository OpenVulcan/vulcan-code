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
/// Every statically reviewed Token Rhythm conversation and image model.
/// 全部经过静态审查的基元律动会话与图片模型。
/// Every CommandCode model declared by CLI 1.54.1 and returned by the Provider API on 2026-09-16.
/// CommandCode CLI 1.54.1 声明且 2026-09-16 Provider API 返回的全部模型。
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
