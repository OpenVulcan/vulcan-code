#![forbid(unsafe_code)]

//! Codex session-layer extraction index for the Vulcan migration.
//! 用于 Vulcan 迁移的 Codex 会话层提取索引。

/// Vulcan adapter traits mounted around the Codex-compatible session core.
/// 挂载在 Codex 兼容会话核心周围的 Vulcan 适配器 trait。
pub mod adapter;
/// Bridge helpers from current Vulcan session-core records into the Codex-compatible contract.
/// 从当前 Vulcan session-core 记录桥接到 Codex 兼容契约的辅助逻辑。
pub mod bridge;
/// Codex-compatible thread, turn, event, and visibility contract types.
/// Codex 兼容的线程、轮次、事件和可见性契约类型。
pub mod contract;
/// User-facing display projection helpers for Codex-compatible events.
/// Codex 兼容事件的用户可见展示投影辅助逻辑。
pub mod display;
/// In-memory store implementation used by migration tests and adapters.
/// 迁移测试和适配器使用的内存存储实现。
pub mod memory;
/// Minimal Codex-compatible thread and turn runner.
/// 最小 Codex 兼容线程与轮次运行器。
pub mod runner;
/// Runtime facade that preserves Codex event append and hook ordering.
/// 保持 Codex 事件追加与 hook 顺序的运行时门面。
pub mod runtime;
/// Directory-backed segmented Codex-compatible session storage.
/// 基于目录的分段式 Codex 兼容会话存储。
pub mod segmented_store;
/// Main-session facade that binds runner, storage, and display projection.
/// 绑定 runner、存储与展示投影的主会话门面。
pub mod session_kernel;
/// Append-only event and overwrite-only state storage traits.
/// 追加式事件与覆盖式状态存储 trait。
pub mod storage;
/// Codex-compatible child-agent metadata and scheduler boundary.
/// Codex 兼容子代理元数据与调度边界。
pub mod subagent;
/// Projection from Codex-compatible events into Vulcan conversation state.
/// 从 Codex 兼容事件投影到 Vulcan 会话状态。
pub mod vulcan_projection;

pub use adapter::{
    CodexExtensionAdapterSet, CodexHookAdapter, CodexModelAdapter, CodexToolAdapter,
};
pub use bridge::codex_event_from_session_event_record;
pub use contract::{
    CodexEventScope, CodexEventVisibility, CodexSessionEvent, CodexSessionEventKind, CodexThreadId,
    CodexTurnId, CodexVisibleRole,
};
pub use display::{CodexDisplayItem, CodexDisplayProjection, project_codex_display_event};
pub use memory::InMemoryCodexSessionStore;
pub use runner::{
    CodexExternalStreamTurnRunner, CodexExternalTurnBeginRequest, CodexExternalTurnFinishRequest,
    CodexThreadState, CodexThreadStatus, CodexTurnRunRequest, CodexTurnRunner,
    CodexTurnTerminalStatus,
};
pub use runtime::CodexSessionRuntime;
pub use segmented_store::{SegmentedCodexSessionStore, SegmentedCodexSessionStoreOptions};
pub use session_kernel::CodexMainSessionKernel;
pub use storage::{CodexSessionStateStore, CodexSessionStore, CodexStorageWrite};
pub use subagent::{
    CodexAgentKind, CodexSubagentCompletionOutcome, CodexSubagentCoordinator, CodexSubagentMailbox,
    CodexSubagentMailboxEvent, CodexSubagentMailboxEventKind, CodexSubagentMetadata,
    CodexSubagentScheduler, CodexSubagentSpawnOutcome, CodexSubagentSpawnRequest,
    CodexSubagentStatus, InMemoryCodexSubagentMailbox,
};
pub use vulcan_projection::{
    CodexVulcanConversationProjection, codex_events_to_vulcan_conversation_state,
    codex_session_event_to_vulcan_record,
};

/// Exact upstream Codex commit used for this extraction.
/// 本次提取使用的 Codex 上游精确提交。
pub const UPSTREAM_CODEX_COMMIT: &str = "b89ce9a2bcedcfddf3a48f387b7912d602d6d87c";

/// Local upstream root captured inside this crate.
/// 本 crate 内保存的本地上游快照根目录。
pub const UPSTREAM_ROOT: &str = "upstream";

/// One extracted upstream source group.
/// 一组已提取的上游源文件范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamSourceGroup {
    /// Stable group identifier used by migration scripts and reports.
    /// 迁移脚本和报告使用的稳定分组标识。
    pub id: &'static str,
    /// Human-readable purpose of this source group.
    /// 该源文件分组的人类可读用途。
    pub purpose: &'static str,
    /// Original path in the Codex repository.
    /// 在 Codex 仓库中的原始路径。
    pub codex_path: &'static str,
    /// Captured path under this crate.
    /// 在本 crate 下保存的路径。
    pub local_path: &'static str,
}

/// Returns the extracted Codex source groups currently available for porting.
/// 返回当前可用于移植的 Codex 源文件分组。
pub const fn upstream_source_groups() -> &'static [UpstreamSourceGroup] {
    &UPSTREAM_SOURCE_GROUPS
}

/// Extracted Codex source groups captured as raw upstream snapshots.
/// 以原始上游快照形式保存的 Codex 源文件分组。
pub const UPSTREAM_SOURCE_GROUPS: [UpstreamSourceGroup; 14] = [
    UpstreamSourceGroup {
        id: "core-session",
        purpose: "main session lifecycle, turn execution, input queue, MCP review, and rollout reconstruction",
        codex_path: "codex-rs/core/src/session",
        local_path: "upstream/codex-rs/core/src/session",
    },
    UpstreamSourceGroup {
        id: "agent-registry",
        purpose: "agent metadata, registry, role resolution, and status helpers",
        codex_path: "codex-rs/core/src/agent",
        local_path: "upstream/codex-rs/core/src/agent",
    },
    UpstreamSourceGroup {
        id: "agent-control",
        purpose: "subagent spawn, fork, inheritance, live-agent control, and wait semantics",
        codex_path: "codex-rs/core/src/agent/control",
        local_path: "upstream/codex-rs/core/src/agent/control",
    },
    UpstreamSourceGroup {
        id: "multi-agent-tools",
        purpose: "model-visible multi-agent tool handlers for spawn, wait, resume, input, and close",
        codex_path: "codex-rs/core/src/tools/handlers/multi_agents",
        local_path: "upstream/codex-rs/core/src/tools/handlers/multi_agents",
    },
    UpstreamSourceGroup {
        id: "multi-agent-tools-v2",
        purpose: "newer multi-agent tool handlers for list, spawn, wait, message, followup, and close",
        codex_path: "codex-rs/core/src/tools/handlers/multi_agents_v2",
        local_path: "upstream/codex-rs/core/src/tools/handlers/multi_agents_v2",
    },
    UpstreamSourceGroup {
        id: "core-state",
        purpose: "session and turn runtime state, mailbox delivery phase, and service state",
        codex_path: "codex-rs/core/src/state",
        local_path: "upstream/codex-rs/core/src/state",
    },
    UpstreamSourceGroup {
        id: "core-tasks",
        purpose: "regular, compact, review, user-shell, and lifecycle task execution surfaces",
        codex_path: "codex-rs/core/src/tasks",
        local_path: "upstream/codex-rs/core/src/tasks",
    },
    UpstreamSourceGroup {
        id: "core-thread-runtime",
        purpose: "Codex thread manager, thread runtime, event mapping, truncation, turn metadata, and timing",
        codex_path: "codex-rs/core/src/{event_mapping.rs,codex_thread.rs,thread_manager.rs,thread_rollout_truncation.rs,turn_metadata.rs,turn_timing.rs}",
        local_path: "upstream/codex-rs/core/src",
    },
    UpstreamSourceGroup {
        id: "rollout",
        purpose: "append-only JSONL rollout recording, listing, metadata, search, compression, and state DB",
        codex_path: "codex-rs/rollout/src",
        local_path: "upstream/codex-rs/rollout/src",
    },
    UpstreamSourceGroup {
        id: "rollout-trace",
        purpose: "rollout trace models, tool/runtime reducers, conversation normalization, and agent edges",
        codex_path: "codex-rs/rollout-trace/src",
        local_path: "upstream/codex-rs/rollout-trace/src",
    },
    UpstreamSourceGroup {
        id: "app-server-protocol",
        purpose: "thread, turn, item, history, event mapping, and v2 display protocol types",
        codex_path: "codex-rs/app-server-protocol/src/protocol",
        local_path: "upstream/codex-rs/app-server-protocol/src/protocol",
    },
    UpstreamSourceGroup {
        id: "app-server-thread-turn-processors",
        purpose: "thread lifecycle, thread read/list/resume, turn start/steer, summary, and redaction processors",
        codex_path: "codex-rs/app-server/src/request_processors",
        local_path: "upstream/codex-rs/app-server/src/request_processors",
    },
    UpstreamSourceGroup {
        id: "thread-store",
        purpose: "thread store traits, live thread state, metadata sync, and in-memory store",
        codex_path: "codex-rs/thread-store/src",
        local_path: "upstream/codex-rs/thread-store/src",
    },
    UpstreamSourceGroup {
        id: "local-thread-store",
        purpose: "local thread creation, reading, listing, archive, search, and metadata update",
        codex_path: "codex-rs/thread-store/src/local",
        local_path: "upstream/codex-rs/thread-store/src/local",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the extraction index keeps the exact upstream commit and required source groups.
    /// 验证提取索引保留精确上游提交和必要源文件分组。
    #[test]
    fn source_groups_include_session_rollout_thread_store_and_agent_control() {
        let ids = upstream_source_groups()
            .iter()
            .map(|group| group.id)
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(
            UPSTREAM_CODEX_COMMIT,
            "b89ce9a2bcedcfddf3a48f387b7912d602d6d87c"
        );
        assert!(ids.contains("core-session"));
        assert!(ids.contains("agent-control"));
        assert!(ids.contains("multi-agent-tools"));
        assert!(ids.contains("multi-agent-tools-v2"));
        assert!(ids.contains("core-state"));
        assert!(ids.contains("core-tasks"));
        assert!(ids.contains("core-thread-runtime"));
        assert!(ids.contains("rollout"));
        assert!(ids.contains("rollout-trace"));
        assert!(ids.contains("app-server-protocol"));
        assert!(ids.contains("app-server-thread-turn-processors"));
        assert!(ids.contains("thread-store"));
    }
}
