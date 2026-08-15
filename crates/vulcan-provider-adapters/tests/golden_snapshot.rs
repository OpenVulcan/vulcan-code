//! Golden snapshot oracle for the manifest-compiled system provider registry.
//! Manifest 编译系统供应商注册表的 golden 快照 oracle。
//!
//! The golden file freezes the observable provider/service/credential/route shape built by
//! `ProviderRegistry::first_batch()` from the authoritative bundled manifests.
//! 该 golden 文件冻结 `ProviderRegistry::first_batch()` 构建的可观察供应商/服务/凭据/路由形态。
//! 其输入是权威内置 Manifest。

use std::collections::BTreeMap;
use std::path::PathBuf;
use vulcan_provider_adapters::{ProviderDescriptor, ProviderRegistry};

/// Golden file location relative to this crate.
/// golden 文件相对当前 crate 的位置。
const GOLDEN_RELATIVE_PATH: &str = "resources/golden/system-providers-v2.json";

/// Environment variable that regenerates the golden file instead of comparing.
/// 设置后重新生成 golden 文件而不是比较。
const GENERATE_ENV: &str = "VULCAN_GENERATE_GOLDEN";

/// Returns the absolute golden file path.
/// 返回 golden 文件的绝对路径。
fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_RELATIVE_PATH)
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
