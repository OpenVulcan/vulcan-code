//! Connection-owned, coalesced MCP catalog refreshes.
//! 连接持有的 MCP 目录合并刷新。

use super::{
    McpRuntimeInner, McpServerConfig, McpTransport, McpTransportEvent,
    apply_server_optional_capability_refresh_outcome,
    build_server_optional_capability_refresh_outcome, event_channel_error,
    load_server_prompt_count, load_server_resource_count, receive_server_runtime_event,
    refresh_server_tools,
};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Records exactly which independent capability surfaces became dirty.
/// 记录具体哪些独立能力表面已经失效。
#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct CatalogRefreshDemand {
    tools: bool,
    prompts: bool,
    resources: bool,
}

impl CatalogRefreshDemand {
    /// Initial connection refreshes both optional catalogs after the mandatory tool read.
    /// 初次连接在必需工具读取之后刷新两项可选目录。
    pub(super) const OPTIONAL: Self = Self {
        tools: false,
        prompts: true,
        resources: true,
    };

    fn observe(&mut self, event: McpTransportEvent) -> Result<(), String> {
        match event {
            McpTransportEvent::ToolsChanged => self.tools = true,
            McpTransportEvent::PromptsChanged => self.prompts = true,
            McpTransportEvent::ResourcesChanged => self.resources = true,
            McpTransportEvent::Disconnected { reason } => return Err(reason),
        }
        Ok(())
    }
}

/// Retains independent errors so refreshing resources cannot hide a failed prompt discovery.
/// 保留独立错误，防止刷新资源时隐藏 prompt 发现失败。
pub(super) struct OptionalCapabilityState {
    prompts: Result<usize, String>,
    resources: Result<usize, String>,
}

impl Default for OptionalCapabilityState {
    fn default() -> Self {
        Self {
            prompts: Ok(0),
            resources: Ok(0),
        }
    }
}

/// Waits for one event and coalesces only the finite queue observed at that instant.
/// 等待一个事件，并仅合并该时刻观察到的有限队列。
pub(super) async fn receive_demand(
    receiver: &mut broadcast::Receiver<McpTransportEvent>,
    server_name: &str,
) -> Result<CatalogRefreshDemand, String> {
    let first = receive_server_runtime_event(receiver, server_name).await?;
    let mut demand = CatalogRefreshDemand::default();
    demand.observe(first)?;
    // New arrivals are processed next time; an active producer cannot starve refresh indefinitely.
    // 新到事件留给下一轮处理；活跃生产者不能无限饿死刷新。
    let pending = receiver.len();
    for _ in 0..pending {
        match receiver.try_recv() {
            Ok(event) => demand.observe(event)?,
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                return Err(event_channel_error(
                    broadcast::error::RecvError::Lagged(skipped),
                    server_name,
                ));
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                return Err(event_channel_error(
                    broadcast::error::RecvError::Closed,
                    server_name,
                ));
            }
        }
    }
    Ok(demand)
}

/// Refreshes only dirty remote catalogs and republishes retained optional warnings after a tool update.
/// 仅刷新失效的远端目录，并在工具更新后重新发布保留的可选能力警告。
pub(super) async fn refresh_catalog(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
    state: &mut OptionalCapabilityState,
    demand: CatalogRefreshDemand,
) -> Result<(), String> {
    if demand.tools {
        refresh_server_tools(inner, server, transport.clone()).await?;
    }
    refresh_optional(inner, server, transport.as_ref(), state, demand).await;
    Ok(())
}

/// Queries independent optional surfaces concurrently when both are dirty, without duplicate reads.
/// 两项可选表面同时失效时并发查询，且不产生重复读取。
pub(super) async fn refresh_optional(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: &dyn McpTransport,
    state: &mut OptionalCapabilityState,
    demand: CatalogRefreshDemand,
) {
    let (prompts, resources) = tokio::join!(
        async {
            if demand.prompts {
                Some(load_server_prompt_count(server, transport).await)
            } else {
                None
            }
        },
        async {
            if demand.resources {
                Some(load_server_resource_count(server, transport).await)
            } else {
                None
            }
        },
    );
    if let Some(result) = prompts {
        state.prompts = result;
    }
    if let Some(result) = resources {
        state.resources = result;
    }
    let outcome = build_server_optional_capability_refresh_outcome(
        state.prompts.clone(),
        state.resources.clone(),
    );
    apply_server_optional_capability_refresh_outcome(inner, &server.id, outcome);
}

#[cfg(test)]
mod tests {
    use super::super::{
        McpRuntimePurpose, McpRuntimeServerStatus, McpRuntimeSnapshot, McpServerRegistry,
        initial_server_snapshot,
    };
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{
        RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use vulcan_mcp_runtime::{McpRuntimeError, McpToolSchema, Prompt, Resource};

    struct CountingTransport {
        events: broadcast::Sender<McpTransportEvent>,
        prompt_calls: AtomicUsize,
        resource_calls: AtomicUsize,
        prompt_fails: AtomicBool,
    }

    #[async_trait::async_trait]
    impl McpTransport for CountingTransport {
        fn endpoint(&self) -> &str {
            "test://selective-catalog"
        }
        fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
            self.events.subscribe()
        }
        async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
            Ok(Vec::new())
        }
        async fn list_prompts(&self) -> Result<Vec<Prompt>, McpRuntimeError> {
            self.prompt_calls.fetch_add(1, Ordering::SeqCst);
            if self.prompt_fails.load(Ordering::SeqCst) {
                Err(McpRuntimeError::transport("fixture prompt failure"))
            } else {
                Ok(Vec::new())
            }
        }
        async fn list_resources(&self) -> Result<Vec<Resource>, McpRuntimeError> {
            self.resource_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }
        async fn call_tool(
            &self,
            _name: &str,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value, McpRuntimeError> {
            panic!("catalog refresh must never execute tools")
        }
    }

    /// Duplicate notifications are coalesced, but a queued disconnection still wins.
    /// 重复通知会被合并，但队列中的断连仍优先生效。
    #[tokio::test]
    async fn pending_notifications_are_coalesced_without_hiding_disconnect() {
        let (sender, mut receiver) = broadcast::channel(8);
        for event in [
            McpTransportEvent::ToolsChanged,
            McpTransportEvent::ToolsChanged,
            McpTransportEvent::PromptsChanged,
            McpTransportEvent::ResourcesChanged,
        ] {
            sender.send(event).expect("queued event");
        }
        assert_eq!(
            receive_demand(&mut receiver, "fixture")
                .await
                .expect("coalesced demand"),
            CatalogRefreshDemand {
                tools: true,
                prompts: true,
                resources: true
            }
        );
        assert!(matches!(
            receiver.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        sender
            .send(McpTransportEvent::ToolsChanged)
            .expect("dirty tool");
        sender
            .send(McpTransportEvent::Disconnected {
                reason: "fixture disconnected".to_string(),
            })
            .expect("disconnect");
        assert_eq!(
            receive_demand(&mut receiver, "fixture")
                .await
                .expect_err("disconnect must win"),
            "fixture disconnected"
        );
    }

    /// Resource-only and tool-only refreshes neither reread prompts nor hide their prior failure.
    /// 仅刷新资源或工具时，不重新读取 prompt，也不隐藏其先前失败。
    #[tokio::test]
    async fn selected_refreshes_preserve_independent_errors_and_query_counts() {
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: "selective".to_string(),
            name: "Selective".to_string(),
            enabled: false,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: "unused".to_string(),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let mut snapshot = initial_server_snapshot(&server);
        snapshot.status = McpRuntimeServerStatus::Connected;
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);
        let inner = Arc::new(McpRuntimeInner {
            registry: McpServerRegistry::new(vec![server.clone()]),
            purpose: McpRuntimePurpose::Managed,
            transports: RwLock::new(BTreeMap::new()),
            resource_releases: RwLock::new(BTreeMap::new()),
            auth_sessions: tokio::sync::RwLock::new(BTreeMap::new()),
            snapshot: RwLock::new(McpRuntimeSnapshot {
                servers: BTreeMap::from([(server.id.clone(), snapshot)]),
            }),
            retry_counts: RwLock::new(BTreeMap::new()),
            shutdown_tx,
        });
        let (events, _) = broadcast::channel(8);
        let transport = Arc::new(CountingTransport {
            events,
            prompt_calls: AtomicUsize::new(0),
            resource_calls: AtomicUsize::new(0),
            prompt_fails: AtomicBool::new(true),
        });
        let mut state = OptionalCapabilityState::default();
        refresh_optional(
            &inner,
            &server,
            transport.as_ref(),
            &mut state,
            CatalogRefreshDemand::OPTIONAL,
        )
        .await;
        transport.prompt_fails.store(false, Ordering::SeqCst);
        refresh_optional(
            &inner,
            &server,
            transport.as_ref(),
            &mut state,
            CatalogRefreshDemand {
                resources: true,
                ..Default::default()
            },
        )
        .await;
        refresh_catalog(
            &inner,
            &server,
            transport.clone(),
            &mut state,
            CatalogRefreshDemand {
                tools: true,
                ..Default::default()
            },
        )
        .await
        .expect("tools refresh");
        assert_eq!(transport.prompt_calls.load(Ordering::SeqCst), 1);
        assert_eq!(transport.resource_calls.load(Ordering::SeqCst), 2);
        assert!(
            inner.snapshot.read().expect("snapshot").servers[&server.id]
                .last_error
                .as_deref()
                .is_some_and(|message| message.contains("fixture prompt failure"))
        );
        refresh_optional(
            &inner,
            &server,
            transport.as_ref(),
            &mut state,
            CatalogRefreshDemand {
                prompts: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(transport.prompt_calls.load(Ordering::SeqCst), 2);
        assert_eq!(transport.resource_calls.load(Ordering::SeqCst), 2);
        assert!(
            inner.snapshot.read().expect("snapshot").servers[&server.id]
                .last_error
                .is_none()
        );
    }
}
