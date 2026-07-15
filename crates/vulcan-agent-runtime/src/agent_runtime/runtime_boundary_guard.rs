//! Regression guards that keep the service runtime boundary from reintroducing removed desktop dependencies.
//! 防止服务运行时边界重新引入已移除桌面依赖的回归护栏。

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

/// Verifies the workspace manifest no longer declares the removed legacy desktop shell and UI-only crates.
/// 验证工作区清单文件不再声明已移除的旧桌面壳与 UI 专属 crate。
#[test]
fn workspace_manifest_does_not_reintroduce_removed_desktop_members() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("agent-runtime manifest should live under crates")
        .join("Cargo.toml");
    let manifest_text =
        fs::read_to_string(&manifest_path).expect("workspace Cargo.toml should remain readable");

    let removed_entries = [
        ["apps/", "vulcan-code-", "gui"].concat(),
        ["crates/", "vulcan-agent-", "view"].concat(),
        ["crates/", "vulcan-", "markdown"].concat(),
        ["crates/", "vulcan-", "i", "ced-widget"].concat(),
        ["i", "ced", " ="].concat(),
        ["i", "ced", "_aw ="].concat(),
        ["i", "ced", "_fonts ="].concat(),
    ];
    for removed_entry in removed_entries {
        assert!(
            !manifest_text.contains(&removed_entry),
            "workspace manifest must not reintroduce `{removed_entry}`: {}",
            manifest_path.display()
        );
    }
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
            let approved = approved_suffixes.contains(&relative_path);

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
    let runtime_manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let agent_mcp_root = runtime_manifest_dir.join("src").join("mcp");
    let mcp_runtime_root = runtime_manifest_dir
        .parent()
        .expect("agent-runtime crate should remain nested under crates/")
        .join("vulcan-mcp-runtime")
        .join("src");

    let offending_files = [agent_mcp_root, mcp_runtime_root]
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

/// Verifies agent-runtime MCP glue keeps concrete transport launch wiring centralized in `mcp/config.rs`.
/// 验证 agent-runtime MCP 胶水层继续将具体传输启动接线集中保留在 `mcp/config.rs` 中。
#[test]
fn agent_mcp_sources_keep_launch_spec_mapping_centralized() {
    let agent_mcp_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("mcp");
    let rust_files = collect_rust_source_files(&agent_mcp_root);
    let approved_launch_spec_file = agent_mcp_root.join("config.rs");

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
        "agent MCP sources must keep launch-spec mapping centralized in config.rs: {:?}",
        offending_launch_spec_files
    );
}

/// Verifies agent runtime sources do not directly import concrete MCP transport implementations after the runtime boundary split.
/// 验证在 MCP 运行时边界拆分后，agent runtime 源码不再直接导入具体 MCP 传输实现类型。
#[test]
fn agent_runtime_sources_do_not_directly_import_concrete_mcp_transports() {
    let runtime_src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let rust_files = collect_rust_source_files(&runtime_src_root);

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
        "agent runtime sources must not directly import concrete MCP transports again: {:?}",
        offending_files
    );
}

/// Verifies tool-manager preparation surfaces consume MCP tool bindings instead of reaching back to the raw runtime handle or legacy tool-view type.
/// 验证工具管理器准备链路改为消费 MCP 工具绑定，而不是重新回退到原始运行时句柄或旧工具视图类型。
#[test]
fn runtime_tool_preparation_surfaces_stay_on_mcp_tool_bindings() {
    let runtime_src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let bridge_path = runtime_src_root.join("tooling").join("bridge.rs");
    let preparation_path = runtime_src_root
        .join("agent_runtime")
        .join("execution_preparation.rs");
    let bootstrap_resources_path = runtime_src_root
        .join("agent_runtime")
        .join("runtime_stream_bootstrap_resources.rs");

    let bridge_source =
        fs::read_to_string(&bridge_path).expect("tooling/bridge.rs should remain readable");
    let preparation_source = fs::read_to_string(&preparation_path)
        .expect("execution_preparation.rs should remain readable");
    let bootstrap_resources_source = fs::read_to_string(&bootstrap_resources_path)
        .expect("runtime_stream_bootstrap_resources.rs should remain readable");

    assert!(
        !bridge_source.contains("McpRuntimeHandle"),
        "tooling/bridge.rs must not reintroduce direct McpRuntimeHandle usage: {}",
        bridge_path.display()
    );
    assert!(
        !bridge_source.contains("McpRuntimeToolView"),
        "tooling/bridge.rs must not reintroduce legacy McpRuntimeToolView usage: {}",
        bridge_path.display()
    );
    assert!(
        bridge_source.contains("McpRuntimeToolBinding"),
        "tooling/bridge.rs should continue consuming McpRuntimeToolBinding: {}",
        bridge_path.display()
    );
    assert!(
        !preparation_source.contains("McpRuntimeHandle"),
        "execution_preparation.rs must not reintroduce direct McpRuntimeHandle usage: {}",
        preparation_path.display()
    );
    assert!(
        !bootstrap_resources_source.contains("McpRuntimeHandle"),
        "runtime_stream_bootstrap_resources.rs must not reintroduce direct McpRuntimeHandle usage: {}",
        bootstrap_resources_path.display()
    );
}

/// Verifies runtime-stream execution stays on the stable MCP access surface instead of reintroducing raw runtime handles.
/// 验证 runtime-stream 执行链继续停留在稳定 MCP 访问面之上，而不会重新引回原始运行时句柄。
#[test]
fn runtime_stream_inner_layers_do_not_reintroduce_raw_mcp_runtime_handles() {
    let runtime_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("agent_runtime");
    let outer_entry_path = runtime_root.join("runtime_stream.rs");
    let launch_path = runtime_root.join("runtime_stream_launch.rs");
    let preparation_path = runtime_root.join("runtime_stream_preparation.rs");
    let preparation_resources_path = runtime_root.join("runtime_stream_preparation_resources.rs");

    let outer_entry_source =
        fs::read_to_string(&outer_entry_path).expect("runtime_stream.rs should remain readable");
    let launch_source =
        fs::read_to_string(&launch_path).expect("runtime_stream_launch.rs should remain readable");
    let preparation_source = fs::read_to_string(&preparation_path)
        .expect("runtime_stream_preparation.rs should remain readable");
    let preparation_resources_source = fs::read_to_string(&preparation_resources_path)
        .expect("runtime_stream_preparation_resources.rs should remain readable");

    assert!(
        outer_entry_source.contains("mcp_runtime: Option<McpRuntimeAccess>"),
        "runtime_stream.rs should accept the stable McpRuntimeAccess surface at the outer entrypoint: {}",
        outer_entry_path.display()
    );
    assert!(
        outer_entry_source.contains("configured_mcp_runtime_access"),
        "runtime_stream.rs should build its configured fallback through configured_mcp_runtime_access: {}",
        outer_entry_path.display()
    );
    assert!(
        !outer_entry_source.contains("McpRuntimeHandle"),
        "runtime_stream.rs must not reintroduce raw McpRuntimeHandle usage anymore: {}",
        outer_entry_path.display()
    );
    assert!(
        launch_source.contains("McpRuntimeAccess"),
        "runtime_stream_launch.rs should consume McpRuntimeAccess instead of raw handles: {}",
        launch_path.display()
    );
    assert!(
        !launch_source.contains("McpRuntimeHandle"),
        "runtime_stream_launch.rs must not reintroduce raw McpRuntimeHandle usage: {}",
        launch_path.display()
    );
    assert!(
        preparation_source.contains("McpRuntimeAccess"),
        "runtime_stream_preparation.rs should consume McpRuntimeAccess instead of raw handles: {}",
        preparation_path.display()
    );
    assert!(
        !preparation_source.contains("McpRuntimeHandle"),
        "runtime_stream_preparation.rs must not reintroduce raw McpRuntimeHandle usage: {}",
        preparation_path.display()
    );
    assert!(
        preparation_resources_source.contains("McpRuntimeAccess"),
        "runtime_stream_preparation_resources.rs should consume McpRuntimeAccess instead of raw handles: {}",
        preparation_resources_path.display()
    );
    assert!(
        !preparation_resources_source.contains("McpRuntimeHandle"),
        "runtime_stream_preparation_resources.rs must not reintroduce raw McpRuntimeHandle usage: {}",
        preparation_resources_path.display()
    );
}

/// Verifies top-level service workbench construction paths stay on stable MCP runtime access objects.
/// 验证顶层服务工作台构建路径继续使用稳定 MCP 运行时访问对象。
#[test]
fn service_workbench_paths_stay_on_mcp_runtime_access() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("agent-runtime manifest should live under crates");
    let service_kernel_root = workspace_root
        .join("crates")
        .join("vulcan-service-kernel")
        .join("src");

    let service_kernel_source = collect_rust_source_files(&service_kernel_root)
        .into_iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        service_kernel_source.contains("mcp_runtime: Option<McpRuntimeAccess>"),
        "service kernel should retain MCP runtime state through the stable McpRuntimeAccess field under: {}",
        service_kernel_root.display()
    );
    assert!(
        !service_kernel_source.contains("mcp_runtime: Option<McpRuntimeHandle>"),
        "service kernel must not keep a raw McpRuntimeHandle as its agent-runtime field under: {}",
        service_kernel_root.display()
    );
    assert!(
        service_kernel_source.contains("bootstrap_mcp_runtime_access"),
        "service kernel should bootstrap the stable McpRuntimeAccess surface under: {}",
        service_kernel_root.display()
    );
    assert!(
        service_kernel_source.contains("stream_runtime_run_with_intervention_and_plugins("),
        "service kernel should pass stable MCP and plugin runtime surfaces into the agent stream entrypoint under: {}",
        service_kernel_root.display()
    );
    assert!(
        !service_kernel_source.contains("McpRuntimeAccess::from_configuration"),
        "service kernel must not manually assemble McpRuntimeAccess through a wide constructor under: {}",
        service_kernel_root.display()
    );
}

/// Verifies the removed MCP runtime projection shim does not reappear in the agent runtime module tree.
/// 验证已经移除的 MCP 运行时投影中间层不会重新出现在 agent runtime 模块树中。
#[test]
fn agent_runtime_mod_does_not_reintroduce_mcp_runtime_projection_module() {
    let mod_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("agent_runtime")
        .join("mod.rs");
    let projection_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("agent_runtime")
        .join("mcp_runtime_projection.rs");

    let mod_source =
        fs::read_to_string(&mod_path).expect("agent_runtime/mod.rs should remain readable");

    assert!(
        !mod_source.contains("pub mod mcp_runtime_projection;"),
        "agent_runtime/mod.rs must not reintroduce mcp_runtime_projection module wiring: {}",
        mod_path.display()
    );
    assert!(
        !projection_path.exists(),
        "agent_runtime/mcp_runtime_projection.rs must remain deleted: {}",
        projection_path.display()
    );
}

/// Verifies the runtime foundation continues exposing MCP summary and discovery reads through the unified overview surface.
/// 验证运行时基础架构继续通过统一 overview 视图暴露 MCP 摘要与发现读取能力。
#[test]
fn runtime_foundation_mcp_reads_stay_on_unified_overview_surface() {
    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("agent_runtime")
        .join("runtime.rs");
    let mcp_runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("mcp")
        .join("runtime.rs");
    let mcp_runtime_tools_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("mcp")
        .join("runtime")
        .join("tools.rs");
    let runtime_source =
        fs::read_to_string(&runtime_path).expect("agent_runtime/runtime.rs should remain readable");
    let mcp_runtime_source =
        fs::read_to_string(&mcp_runtime_path).expect("mcp/runtime.rs should remain readable");
    let mcp_runtime_tools_source = fs::read_to_string(&mcp_runtime_tools_path)
        .expect("mcp/runtime/tools.rs should remain readable");

    assert!(
        runtime_source.contains("pub fn mcp_overview(&self) -> McpRuntimeOverview"),
        "RuntimeFoundation should keep exposing the unified MCP overview surface: {}",
        runtime_path.display()
    );
    assert!(
        !runtime_source.contains("pub fn mcp_rows(&self)"),
        "RuntimeFoundation must not reintroduce the legacy mcp_rows surface: {}",
        runtime_path.display()
    );
    assert!(
        !runtime_source.contains("pub fn enabled_mcp_server_rows(&self)"),
        "RuntimeFoundation must not reintroduce the legacy enabled_mcp_server_rows surface: {}",
        runtime_path.display()
    );
    assert!(
        !runtime_source.contains("pub fn enabled_mcp_discovery_report(&self)"),
        "RuntimeFoundation must not reintroduce the legacy enabled_mcp_discovery_report surface: {}",
        runtime_path.display()
    );
    assert!(
        runtime_source.contains("mcp_runtime_access: McpRuntimeAccess"),
        "RuntimeFoundation should keep MCP reads behind the unified McpRuntimeAccess field: {}",
        runtime_path.display()
    );
    assert!(
        mcp_runtime_source.contains("configured_overview: McpRuntimeOverview")
            || mcp_runtime_tools_source.contains("configured_overview: McpRuntimeOverview"),
        "MCP runtime should keep the configured fallback as a direct overview field: {}",
        mcp_runtime_path.display()
    );
    assert!(
        !mcp_runtime_source.contains("McpRegistrySummary"),
        "MCP runtime must not reintroduce the old McpRegistrySummary wrapper: {}",
        mcp_runtime_path.display()
    );
}
