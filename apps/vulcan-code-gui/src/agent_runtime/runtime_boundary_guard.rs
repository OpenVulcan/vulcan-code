//! Regression guards that keep `vulcan-code-gui` free from reintroducing the removed `rig` runtime dependency.
//! 防止 `vulcan-code-gui` 重新引入已移除 `rig` 运行时依赖的回归护栏。

use std::fs;
use std::path::{Path, PathBuf};

/// Collects one recursive file list rooted at the given directory while keeping only Rust source files.
/// 递归收集给定目录下的文件列表，同时仅保留 Rust 源文件。
fn collect_rust_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_rust_source_files(&path));
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            files.push(path);
        }
    }

    files
}

/// Verifies the GUI crate manifest no longer declares a direct `rig` workspace dependency.
/// 验证 GUI crate 清单文件已不再声明直接的 `rig` 工作区依赖。
#[test]
fn gui_crate_manifest_does_not_depend_on_rig_workspace() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .expect("vulcan-code-gui Cargo.toml should remain readable");

    assert!(
        !manifest_text.contains("rig.workspace = true"),
        "GUI crate manifest must not reintroduce `rig.workspace = true`: {}",
        manifest_path.display()
    );
}

/// Verifies the agent runtime source tree no longer references `rig` types or imports after host-owned transports took over the execution chain.
/// 验证在宿主自有传输接管执行链后，agent runtime 源码树不再引用 `rig` 类型或导入。
#[test]
fn agent_runtime_sources_do_not_reference_rig_runtime_symbols() {
    let runtime_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("agent_runtime");
    let rust_files = collect_rust_source_files(&runtime_root);

    let offending_files = rust_files
        .into_iter()
        .filter_map(|path| {
            if path.file_name().and_then(|value| value.to_str())
                == Some("runtime_boundary_guard.rs")
            {
                return None;
            }
            let source = fs::read_to_string(&path).ok()?;
            let contains_rig_symbol = source.contains("use rig::")
                || source.contains(" rig::")
                || source.contains("(rig::")
                || source.contains("<rig::")
                || source.contains("\nrig::");
            contains_rig_symbol.then_some(path)
        })
        .collect::<Vec<_>>();

    assert!(
        offending_files.is_empty(),
        "agent_runtime must not reference rig runtime symbols again: {:?}",
        offending_files
    );
}

/// Verifies module-level dead-code suppressions remain limited to explicitly approved reserve decoder modules.
/// 验证模块级 dead_code 抑制仅保留在显式批准的储备解码器模块中。
#[test]
fn agent_runtime_module_level_dead_code_suppressions_stay_narrow() {
    let runtime_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("agent_runtime");
    let rust_files = collect_rust_source_files(&runtime_root);
    let approved_suffixes = [Path::new("provider_protocol_anthropic_decoders\\jsonl.rs")];

    let offending_files = rust_files
        .into_iter()
        .filter_map(|path| {
            if path.file_name().and_then(|value| value.to_str())
                == Some("runtime_boundary_guard.rs")
            {
                return None;
            }
            let source = fs::read_to_string(&path).ok()?;
            let has_module_level_suppression = source.contains("#![allow(dead_code)]");
            if !has_module_level_suppression {
                return None;
            }

            let relative_path = path.strip_prefix(&runtime_root).ok()?;
            let approved = approved_suffixes
                .iter()
                .any(|approved_suffix| relative_path == *approved_suffix);

            (!approved).then_some(path)
        })
        .collect::<Vec<_>>();

    assert!(
        offending_files.is_empty(),
        "agent_runtime must not grow new module-level dead_code suppressions: {:?}",
        offending_files
    );
}
