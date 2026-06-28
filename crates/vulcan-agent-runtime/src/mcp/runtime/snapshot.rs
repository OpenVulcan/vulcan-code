use super::*;

#[derive(Debug, Clone)]
pub(super) struct McpRuntimeSnapshot {
    /// Server snapshots keyed by stable server identifier.
    /// 以稳定服务器标识为键保存的服务器快照集合。
    pub(super) servers: BTreeMap<String, McpRuntimeServerSnapshot>,
}

impl McpRuntimeSnapshot {
    /// Returns lightweight server summary rows for UI surfaces.
    /// 返回供界面表面使用的轻量服务器摘要行。
    pub(super) fn summary_rows(&self) -> Vec<String> {
        self.servers
            .values()
            .map(|server| {
                let base = format!(
                    "{} · {} · {} · {}",
                    server.status.label(),
                    server.name,
                    server.transport_label,
                    server.endpoint_summary
                );
                match &server.last_error {
                    Some(error) => format!("{base} · {error}"),
                    None => base,
                }
            })
            .collect()
    }

    /// Returns lightweight summary rows for runtime-visible servers that are actively connecting or already connected.
    /// 返回处于连接中或已连接状态的运行时可见服务器轻量摘要行集合。
    pub(super) fn active_summary_rows(&self) -> Vec<String> {
        self.servers
            .values()
            .filter(|server| {
                matches!(
                    server.status,
                    McpRuntimeServerStatus::Connecting | McpRuntimeServerStatus::Connected
                )
            })
            .map(|server| {
                let base = format!(
                    "{} · {} · {} · {}",
                    server.status.label(),
                    server.name,
                    server.transport_label,
                    server.endpoint_summary
                );
                match &server.last_error {
                    Some(error) => format!("{base} · {error}"),
                    None => base,
                }
            })
            .collect()
    }

    /// Returns one flat tool-descriptor list for all currently connected servers.
    /// 返回当前所有已连接服务器的扁平工具描述列表。
    pub(super) fn tool_descriptors(&self) -> Vec<McpRuntimeToolDescriptor> {
        self.servers
            .values()
            .filter(|server| server.enabled)
            .flat_map(|server| server.visible_tool_descriptors().iter().cloned())
            .collect()
    }

    /// Returns host-facing tool views filtered by one allowlist.
    /// 返回按单份白名单过滤后的宿主侧工具视图集合。
    pub(super) fn filtered_tool_views(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolView> {
        let mut occupied_public_tool_names = BTreeSet::new();

        self.tool_descriptors()
            .into_iter()
            .filter(|tool| {
                tool.matches_allowed_sets(
                    allowlist.allowed_server_ids(),
                    allowlist.allowed_tool_names(),
                )
            })
            .map(|tool| {
                let public_tool_id = allocate_unique_public_tool_name(
                    &tool.preferred_public_tool_name(),
                    &mut occupied_public_tool_names,
                );
                tool.host_tool_view(public_tool_id)
            })
            .collect()
    }

    /// Returns lightweight diagnostics for enabled MCP servers only.
    /// 仅返回已启用 MCP 服务器的轻量诊断信息。
    pub(super) fn enabled_server_diagnostics(&self) -> Vec<McpRuntimeServerDiagnostic> {
        self.servers
            .values()
            .filter(|server| server.enabled)
            .map(|server| McpRuntimeServerDiagnostic {
                name: server.name.clone(),
                status: server.status,
                tool_count: server.tools.len(),
                prompt_count: server.prompt_count,
                resource_count: server.resource_count,
                last_error: server.last_error.clone(),
            })
            .collect()
    }

    /// Builds one stable discovery report for enabled MCP servers only.
    /// 为已启用 MCP 服务器构建一份稳定发现报告。
    pub(super) fn enabled_server_discovery_report(&self) -> McpRuntimeDiscoveryReport {
        let diagnostics = self.enabled_server_diagnostics();
        let mut event_projections = Vec::with_capacity(diagnostics.len());
        let mut discovery_rows = Vec::with_capacity(diagnostics.len());
        let mut discovery_errors = Vec::new();
        let mut discovery_warnings = Vec::new();

        for diagnostic in diagnostics {
            let summary = diagnostic.summary_text();
            discovery_rows.push(summary);
            if let Some(warning) = diagnostic.warning_text() {
                discovery_errors.push(warning.clone());
                discovery_warnings.push(warning);
            }
            event_projections.push(diagnostic.to_runtime_event_projection());
        }

        McpRuntimeDiscoveryReport {
            event_projections,
            discovery_rows,
            discovery_errors,
            discovery_warnings,
        }
    }

    /// Builds one stable runtime overview that groups summary rows and the enabled-server discovery report.
    /// 构建一份聚合摘要行与已启用服务器发现报告的稳定运行时概览。
    pub(super) fn overview(&self) -> McpRuntimeOverview {
        McpRuntimeOverview::new(
            self.summary_rows(),
            self.active_summary_rows(),
            self.enabled_server_discovery_report(),
        )
    }
}
