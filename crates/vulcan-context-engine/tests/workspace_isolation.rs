//! Dependency-graph guardrails for the integrated VCE workspace member.
//! 已接入 VCE 工作区成员的依赖图护栏。

use serde_json::Value;
use std::collections::BTreeSet;
use std::process::Command;

/// Workspace packages and storage implementations forbidden as direct VCE dependencies.
/// 禁止成为 VCE 直接依赖的工作区包和存储实现。
const FORBIDDEN_DIRECT_DEPENDENCIES: &[&str] = &[
    "lancedb",
    "rusqlite",
    "sqlite",
    "vldb-controller-client",
    "vulcan-agent-runtime",
    "vulcan-file-watcher",
    "vulcan-service-kernel",
    "vulcan-vmm",
];

/// Return whether one dependency would couple VCE to memory or Codebase Memory.
/// 返回某项依赖是否会把 VCE 耦合到记忆系统或 Codebase Memory。
fn has_forbidden_family(name: &str) -> bool {
    name.starts_with("vulcan-memorymesh")
        || name.contains("codebase-memory")
        || name.contains("codebase_memory")
}

/// Load the current workspace package graph through Cargo's authoritative metadata command.
/// 通过 Cargo 权威元数据命令加载当前工作区包图。
fn workspace_metadata() -> Value {
    // Cargo discovers the enclosing workspace from the manifest directory without assuming a
    // fixed checkout depth, which keeps this gate valid in isolated crate verification too.
    // Cargo 会从清单目录发现外围工作区，无需假设固定检出层级，因此该门禁也适用于独立 crate 验证。
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata should start");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata should return JSON")
}

/// Prove that VCE has only its explicit utility and optional common-contract dependencies.
/// 证明 VCE 只依赖明确的工具库和可选共同契约。
#[test]
fn vce_has_no_runtime_memory_or_storage_dependency() {
    // The metadata package list is the authoritative source for direct dependencies.
    // 元数据包列表是直接依赖的权威来源。
    let metadata = workspace_metadata();
    let packages = metadata["packages"]
        .as_array()
        .expect("metadata packages should be an array");
    let vce = packages
        .iter()
        .find(|package| package["name"] == "vulcan-context-engine")
        .expect("VCE should be a workspace package");
    let direct_names = vce["dependencies"]
        .as_array()
        .expect("package dependencies should be an array")
        .iter()
        .map(|dependency| {
            dependency["name"]
                .as_str()
                .expect("dependency name should be a string")
        })
        .collect::<BTreeSet<_>>();
    // This exact allow-list prevents accidental runtime coupling during later refactors.
    // 该精确白名单防止后续重构意外引入运行时耦合。
    let expected = BTreeSet::from([
        "futures",
        "serde",
        "serde_json",
        "sha2",
        "tokio",
        "vulcan-ai-contract",
        "workspace-hack",
    ]);
    assert_eq!(direct_names, expected);
    assert!(direct_names.iter().all(|name| {
        !FORBIDDEN_DIRECT_DEPENDENCIES.contains(name) && !has_forbidden_family(name)
    }));
}

/// Prove that only the frozen host layers consume VCE directly.
/// 证明只有冻结的宿主层直接消费 VCE。
#[test]
fn vce_has_only_intended_workspace_consumers() {
    // The package dependency declarations reveal every direct workspace consumer.
    // 包依赖声明会暴露每个直接工作区消费者。
    let metadata = workspace_metadata();
    let packages = metadata["packages"]
        .as_array()
        .expect("metadata packages should be an array");
    let consumers = packages
        .iter()
        .filter_map(|package| {
            // VCE itself cannot be considered one of its consumers.
            // VCE 自身不能被视为自己的消费者。
            let package_name = package["name"].as_str()?;
            if package_name == "vulcan-context-engine" {
                return None;
            }
            let depends_on_vce = package["dependencies"]
                .as_array()?
                .iter()
                .any(|dependency| dependency["name"] == "vulcan-context-engine");
            depends_on_vce.then_some(package_name)
        })
        .collect::<BTreeSet<_>>();
    let expected = BTreeSet::from([
        "vulcan-agent-runtime",
        "vulcan-service-kernel",
        "vulcan-workspace-store",
    ]);
    assert_eq!(consumers, expected);
}
