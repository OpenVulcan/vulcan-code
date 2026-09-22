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

/// Verifies tool-schema projection stays an explicit, closed per-rule choice.
/// 验证工具 Schema 投影始终是每条规则显式且封闭的选择。
///
/// A relay may serve a vendor model over its own protocol, so strict projection is not tied to one
/// protocol family. The standard requires that every rule either inherits the provider lightweight
/// default or declares strict, and that no provider changes that default.
/// 中转可以用自己的协议承载某厂商模型，因此严格投影不与某一协议族绑定。标准要求每条规则要么继承
/// 供应商的 lightweight 默认值、要么显式声明 strict，且没有供应商改动该默认值。
#[test]
fn bundled_tool_schema_projection_stays_explicit_and_closed() {
    for (provider_id, text) in load_bundled_manifests() {
        let manifest: vulcan_provider_adapters::ProviderManifestV1 = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("parse bundled manifest `{provider_id}`: {error}"));
        assert_eq!(
            manifest.model_defaults.tool_schema_projection,
            vulcan_provider_adapters::ToolSchemaProjectionMode::Lightweight,
            "system provider `{provider_id}` must leave the provider default lightweight"
        );
        let document: Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("parse bundled manifest JSON `{provider_id}`: {error}"));
        for rule in document["model_rules"].as_array().expect("model_rules array") {
            let rule_id = rule["rule_id"].as_str().unwrap_or("(anonymous)");
            let projection = rule["tool_schema_projection"].as_str();
            assert!(
                projection.is_none() || projection == Some("strict"),
                "rule `{rule_id}` in `{provider_id}` must either inherit the lightweight default or declare strict projection"
            );
        }
    }
}

/// Verifies model-access defaults stay owned by each manifest instead of another provider's data.
/// 验证模型访问默认值由各清单自身持有，而不是来自其它供应商的数据。
///
/// A model that another provider declares must resolve exactly like an undeclared identity here,
/// which is what a cross-provider leak would break. The check names no provider, so adding or
/// removing a provider or a model never requires editing it.
/// 另一供应商声明的模型在本清单中必须与未声明身份解析一致，跨供应商泄漏正是会破坏这一点。
/// 该检测不指名任何供应商，因此增删供应商或模型都不需要修改它。
#[test]
fn bundled_manifest_model_defaults_are_self_contained() {
    let manifests = load_bundled_manifests();
    let mut declared_elsewhere = Vec::new();
    for (owner, text) in &manifests {
        let document: Value =
            serde_json::from_str(text).unwrap_or_else(|error| panic!("parse `{owner}`: {error}"));
        for rule in document["model_rules"].as_array().expect("model_rules array") {
            if let Some(value) = rule["matcher"]["value"].as_str() {
                declared_elsewhere.push((owner.clone(), value.to_string()));
            }
        }
    }

    for (provider_id, text) in &manifests {
        let compiled =
            vulcan_provider_adapters::compile_manifest_text(text, ManifestOwnership::System)
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
        let document: Value =
            serde_json::from_str(text).unwrap_or_else(|error| panic!("parse `{provider_id}`: {error}"));
        let own_declarations: BTreeSet<&str> = document["model_rules"]
            .as_array()
            .expect("model_rules array")
            .iter()
            .filter_map(|rule| rule["matcher"]["value"].as_str())
            .collect();
        for (owner, identity) in &declared_elsewhere {
            // An identity this provider declares itself is not a foreign declaration.
            // 本供应商自己声明的身份不属于外部声明。
            if owner == provider_id || own_declarations.contains(identity.as_str()) {
                continue;
            }
            // Compare inside the same identifier namespace: a provider may legitimately resolve a
            // namespaced `vendor/model` identity differently from a bare model id.
            // 在同一标识命名空间内比较：供应商对命名空间化的 `vendor/model` 与裸模型 id 的解析
            // 可以合法地不同。
            let baseline = match identity.split_once('/') {
                Some((namespace, _)) => format!("{namespace}/unreviewed-model"),
                None => "unreviewed-model".to_string(),
            };
            assert_eq!(
                vulcan_provider_adapters::manifest_default_enabled(
                    &compiled,
                    service_entry_id,
                    None,
                    identity,
                ),
                vulcan_provider_adapters::manifest_default_enabled(
                    &compiled,
                    service_entry_id,
                    None,
                    &baseline,
                ),
                "`{provider_id}` must resolve `{identity}` (declared by `{owner}`) exactly like the undeclared `{baseline}`"
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
