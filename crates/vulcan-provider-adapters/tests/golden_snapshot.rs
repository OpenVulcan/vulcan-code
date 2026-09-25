//! Golden snapshot oracle for the manifest-compiled system provider registry.
//! Manifest 编译系统供应商注册表的 golden 快照 oracle。
//!
//! The golden file freezes the observable provider/service/credential/route shape built by
//! `ProviderRegistry::first_batch()` from the authoritative bundled manifests.
//! 该 golden 文件冻结 `ProviderRegistry::first_batch()` 构建的可观察供应商/服务/凭据/路由形态。
//! 其输入是权威内置 Manifest。

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use vulcan_provider_adapters::{
    ProviderDescriptor, ProviderRegistry, registered_native_embedding_batch_capabilities,
};

/// Golden file location relative to this crate.
/// golden 文件相对当前 crate 的位置。
const GOLDEN_RELATIVE_PATH: &str = "resources/golden/system-providers-v2.json";

/// Golden file for the semantic declarations of every bundled model rule.
/// 全部内置模型规则语义声明的 golden 文件。
const MODEL_SEMANTICS_GOLDEN_RELATIVE_PATH: &str =
    "resources/golden/system-model-semantics-v1.json";

/// Environment variable that regenerates the golden file instead of comparing.
/// 设置后重新生成 golden 文件而不是比较。
const GENERATE_ENV: &str = "VULCAN_GENERATE_GOLDEN";

/// Environment variable that regenerates the model semantic baseline.
/// 用于重新生成模型语义基线的环境变量。
const GENERATE_MODEL_SEMANTICS_ENV: &str = "VULCAN_GENERATE_MODEL_SEMANTICS_GOLDEN";

/// Returns the absolute golden file path.
/// 返回 golden 文件的绝对路径。
fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_RELATIVE_PATH)
}

/// Returns the reviewed all-provider model semantic baseline path.
/// 返回经审核的全供应商模型语义基线路径。
///
/// # Returns
/// 返回值
/// An absolute repository path to the semantic golden file.
/// 指向语义 golden 文件的仓库绝对路径。
fn model_semantics_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MODEL_SEMANTICS_GOLDEN_RELATIVE_PATH)
}

/// Serializes every first-batch provider into a stable provider-id-keyed document.
/// 将首批全部供应商序列化为以供应商 ID 为键的稳定文档。
fn snapshot_document() -> String {
    let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
    let mut providers: BTreeMap<String, &ProviderDescriptor> = BTreeMap::new();
    for provider in registry.providers() {
        providers.insert(provider.id.clone(), provider);
    }
    let mut document =
        serde_json::to_string_pretty(&providers).expect("providers must serialize to JSON");
    document.push('\n');
    document
}

/// Projects all bundled model rules into a stable, provider-neutral semantic document.
/// 将全部内置模型规则投影为稳定且不依赖供应商名称的语义文档。
///
/// # Returns
/// 返回值
/// A sorted snapshot of defaults and rules, excluding presentation-only rule fields.
/// 已排序的默认值及规则快照，不包含规则中仅用于展示的字段。
fn model_semantics_snapshot_document() -> String {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vulcan-runtime-assets")
        .join("resources")
        .join("provider-manifests")
        .join("manifests");
    let mut providers = BTreeMap::new();
    for entry in std::fs::read_dir(&directory).expect("read bundled manifests") {
        let path = entry.expect("read bundled manifest entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let provider_id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("manifest file has provider id")
            .to_string();
        let document: Value = serde_json::from_slice(&std::fs::read(&path).expect("read manifest"))
            .expect("parse manifest");
        let mut rules = document["model_rules"]
            .as_array()
            .expect("manifest model rules")
            .clone();
        for rule in &mut rules {
            let object = rule.as_object_mut().expect("model rule object");
            object.remove("display_name");
            object.remove("description");
        }
        rules.sort_by(|left, right| {
            left["rule_id"]
                .as_str()
                .expect("rule id")
                .cmp(right["rule_id"].as_str().expect("rule id"))
        });
        providers.insert(
            provider_id,
            serde_json::json!({
                "model_defaults": document["model_defaults"],
                "model_rules": rules,
            }),
        );
    }
    let mut output = serde_json::to_string_pretty(&providers).expect("serialize model semantics");
    output.push('\n');
    output
}

/// Verifies the manifest-compiled registry matches the committed golden snapshot.
/// 校验 Manifest 编译注册表与已提交 golden 快照一致。
///
/// Set `VULCAN_GENERATE_GOLDEN=1` to refresh the golden file after a deliberate shape change.
/// 有意变更形态后设置 `VULCAN_GENERATE_GOLDEN=1` 刷新 golden 文件。
#[test]
fn system_provider_registry_matches_golden_snapshot() {
    let actual = snapshot_document();
    let path = golden_path();

    if std::env::var_os(GENERATE_ENV).is_some() {
        std::fs::create_dir_all(path.parent().expect("golden path must have a parent"))
            .expect("create golden directory");
        std::fs::write(&path, actual).expect("write golden snapshot");
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden snapshot at {}", path.display()));
    let regeneration =
        format!("{GENERATE_ENV}=1 cargo test -p vulcan-provider-adapters --test golden_snapshot");
    assert_eq!(
        expected, actual,
        "system provider registry drifted from golden snapshot; regenerate with {regeneration}"
    );
}

/// Detects semantic drift across every bundled provider without provider-specific test code.
/// 无须逐供应商测试代码即可检测全部内置供应商的模型语义漂移。
///
/// The reviewed baseline is independent of the live manifest at comparison time; regeneration
/// requires reviewing the golden diff against provider evidence.
/// 比较时已审核基线独立于实时 Manifest；重建时须将 golden 差异与供应商证据逐项核对。
#[test]
fn all_bundled_model_semantics_match_reviewed_golden() {
    let actual = model_semantics_snapshot_document();
    let path = model_semantics_golden_path();
    if std::env::var_os(GENERATE_MODEL_SEMANTICS_ENV).is_some() {
        std::fs::create_dir_all(path.parent().expect("golden parent")).expect("create golden dir");
        std::fs::write(&path, actual).expect("write model semantic golden");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing model semantic golden at {}", path.display()));
    assert_eq!(
        expected, actual,
        "bundled model semantics drifted; review evidence and regenerate with {GENERATE_MODEL_SEMANTICS_ENV}=1"
    );
}

/// Verifies Singapore Model Studio exposes file-backed Batch only for its declared embeddings.
/// 验证新加坡 Model Studio 仅为其已声明向量模型暴露文件型 Batch。
#[test]
fn singapore_model_studio_exposes_declared_embedding_batch() {
    let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
    let singapore = registry
        .provider("alibaba")
        .expect("Alibaba provider should register")
        .service_entries
        .iter()
        .find(|entry| entry.id == "model-studio-singapore")
        .expect("Singapore Model Studio should register");

    for model_id in ["text-embedding-v3", "text-embedding-v4"] {
        let capabilities = registered_native_embedding_batch_capabilities(singapore, model_id)
            .expect("declared Singapore embedding should expose Batch");
        assert_eq!(capabilities.provider_maximum_requests, Some(50_000));
        assert_eq!(
            capabilities.provider_maximum_payload_bytes,
            Some(524_288_000)
        );
        assert_eq!(capabilities.provider_file_upload_qps, Some(3));
    }
    assert!(registered_native_embedding_batch_capabilities(singapore, "qwen3-rerank").is_none());
}
