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

/// Verifies the MCP runtime layers no longer rely on generic dead-code suppressions after stdio transport options became first-class runtime inputs.
/// 验证在 stdio 传输选项成为一等运行时输入后，MCP 运行时层不再依赖通用 dead_code 抑制。
#[test]
fn mcp_runtime_sources_do_not_reintroduce_dead_code_suppressions() {
    let gui_manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let gui_mcp_root = gui_manifest_dir.join("src").join("mcp");
    let mcp_runtime_root = gui_manifest_dir
        .parent()
        .expect("gui crate should remain nested under apps/")
        .join("vulcan-mcp-runtime")
        .join("src");

    let offending_files = [gui_mcp_root, mcp_runtime_root]
        .into_iter()
        .flat_map(|root| collect_rust_source_files(&root))
        .filter_map(|path| {
            let source = fs::read_to_string(&path).ok()?;
            let has_dead_code_suppression =
                source.contains("#![allow(dead_code)]") || source.contains("#[allow(dead_code)]");
            has_dead_code_suppression.then_some(path)
        })
        .collect::<Vec<_>>();

    assert!(
        offending_files.is_empty(),
        "mcp runtime sources must not reintroduce dead_code suppressions: {:?}",
        offending_files
    );
}

/// Verifies GUI-side MCP glue keeps concrete transport launch wiring centralized in `mcp/config.rs`.
/// 验证 GUI 侧 MCP 胶水层继续将具体传输启动接线集中保留在 `mcp/config.rs` 中。
#[test]
fn gui_mcp_sources_keep_launch_spec_mapping_centralized() {
    let gui_mcp_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("mcp");
    let rust_files = collect_rust_source_files(&gui_mcp_root);
    let approved_launch_spec_file = gui_mcp_root.join("config.rs");

    let offending_launch_spec_files = rust_files
        .iter()
        .filter_map(|path| {
            let source = fs::read_to_string(path).ok()?;
            let references_launch_spec = source.contains("McpTransportLaunchSpec");
            (references_launch_spec && *path != approved_launch_spec_file).then_some(path.clone())
        })
        .collect::<Vec<_>>();

    assert!(
        offending_launch_spec_files.is_empty(),
        "GUI MCP sources must keep launch-spec mapping centralized in config.rs: {:?}",
        offending_launch_spec_files
    );
}

/// Verifies GUI sources do not directly import concrete MCP transport implementations after the runtime boundary split.
/// 验证在 MCP 运行时边界拆分后，GUI 源码不再直接导入具体 MCP 传输实现类型。
#[test]
fn gui_sources_do_not_directly_import_concrete_mcp_transports() {
    let gui_src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let rust_files = collect_rust_source_files(&gui_src_root);

    let offending_files = rust_files
        .into_iter()
        .filter_map(|path| {
            if path.file_name().and_then(|value| value.to_str())
                == Some("runtime_boundary_guard.rs")
            {
                return None;
            }
            let source = fs::read_to_string(&path).ok()?;
            let references_concrete_transport = source.contains("StdioTransport")
                || source.contains("HttpTransport")
                || source.contains("SseTransport");
            references_concrete_transport.then_some(path)
        })
        .collect::<Vec<_>>();

    assert!(
        offending_files.is_empty(),
        "GUI sources must not directly import concrete MCP transports again: {:?}",
        offending_files
    );
}
