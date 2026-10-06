//! Per-plugin ledger of recent Hook and Action execution failures.
//! 按插件保存最近 Hook 与 Action 执行失败的账本。
//!
//! A failed call stops only itself; the ledger keeps what happened so users can inspect it on the
//! plugin. It is history, not cleanup evidence: drains never clear it and it never gates lifecycle
//! operations, unlike cleanup diagnostics.
//! 失败调用只终止其自身；账本保留发生的事实，供用户在插件上查看。它是历史记录而非清理证据：
//! 排空不会清空它，它也不像清理诊断那样阻挡生命周期操作。

use std::collections::{BTreeMap, VecDeque};

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};

/// Failures retained per plugin; older records are dropped first.
/// 每个插件保留的失败记录数；最早的记录先被丢弃。
const VULCAN_PLUGIN_RUNTIME_ERROR_CAPACITY: usize = 20;

/// Plugin entry point whose execution failed.
/// 执行失败的插件入口类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginRuntimeErrorKind {
    /// A manifest-declared host Hook.
    /// 清单声明的宿主 Hook。
    Hook,
    /// A manifest-declared business Action, from the host or a plugin page.
    /// 清单声明的业务 Action，来自宿主或插件页面。
    Action,
}

/// One retained execution failure.
/// 一条已保留的执行失败记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulcanPluginRuntimeErrorRecord {
    /// Unix epoch milliseconds when the failure was recorded.
    /// 记录失败时的 Unix 毫秒时间戳。
    pub at_unix_ms: u64,
    /// Entry point kind that failed.
    /// 失败的入口类型。
    pub kind: VulcanPluginRuntimeErrorKind,
    /// Manifest identifier of the failed Hook or Action.
    /// 失败 Hook 或 Action 的清单标识。
    pub name: String,
    /// Host-reported failure message.
    /// 宿主报告的失败消息。
    pub message: String,
}

/// Bounded failure history keyed by plugin identifier.
/// 以插件标识为键的有界失败历史。
#[derive(Default)]
pub(super) struct RuntimeErrorLedger(BTreeMap<String, VecDeque<VulcanPluginRuntimeErrorRecord>>);

impl RuntimeErrorLedger {
    /// Returns plugin_id's records, oldest first.
    /// 返回 plugin_id 的记录，按时间先后排列。
    pub(super) fn entries(&self, plugin_id: &str) -> Vec<VulcanPluginRuntimeErrorRecord> {
        self.0
            .get(plugin_id)
            .map(|records| records.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl VulcanPluginRuntimeManager {
    /// Appends one failure of plugin_id's `name` entry point, evicting the oldest beyond capacity.
    /// 追加 plugin_id 的 `name` 入口的一次失败，超出容量时淘汰最早记录。
    pub(super) fn record_runtime_error(
        &self,
        plugin_id: &str,
        kind: VulcanPluginRuntimeErrorKind,
        name: &str,
        error: &VulcanPluginRuntimeError,
    ) {
        let at_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            });
        // The ledger holds plain history, so a poisoned lock still guards consistent data.
        // 账本只保存普通历史，因此中毒的锁仍守护一致数据。
        let mut ledger = self
            .state
            .runtime_errors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let records = ledger.0.entry(plugin_id.to_owned()).or_default();
        if records.len() == VULCAN_PLUGIN_RUNTIME_ERROR_CAPACITY {
            records.pop_front();
        }
        records.push_back(VulcanPluginRuntimeErrorRecord {
            at_unix_ms,
            kind,
            name: name.to_owned(),
            message: error.to_string(),
        });
    }

    /// Returns plugin_id's retained execution failures, oldest first.
    /// 返回 plugin_id 已保留的执行失败，按时间先后排列。
    pub fn runtime_errors(&self, plugin_id: &str) -> Vec<VulcanPluginRuntimeErrorRecord> {
        self.state
            .runtime_errors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries(plugin_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ledger keeps the newest failures per plugin and evicts the oldest beyond capacity.
    /// 账本按插件保留最新失败，超出容量时淘汰最早记录。
    #[test]
    fn ledger_keeps_the_newest_failures_per_plugin() {
        let root = tempfile::tempdir().expect("create ledger test root");
        let manager = VulcanPluginRuntimeManager::without_runtime(
            root.path().join("plugins"),
            root.path().join("user-plugins"),
            root.path().join("bridge"),
            root.path().join("distribution"),
            root.path().join("state"),
            root.path().join("runtime"),
        );
        for index in 0..=VULCAN_PLUGIN_RUNTIME_ERROR_CAPACITY {
            manager.record_runtime_error(
                "noisy",
                VulcanPluginRuntimeErrorKind::Hook,
                "precheck",
                &VulcanPluginRuntimeError::new(format!("failure {index}")),
            );
        }
        manager.record_runtime_error(
            "quiet",
            VulcanPluginRuntimeErrorKind::Action,
            "refresh",
            &VulcanPluginRuntimeError::new("only failure"),
        );
        let noisy = manager.runtime_errors("noisy");
        assert_eq!(noisy.len(), VULCAN_PLUGIN_RUNTIME_ERROR_CAPACITY);
        assert_eq!(noisy[0].message, "failure 1");
        assert_eq!(
            noisy[VULCAN_PLUGIN_RUNTIME_ERROR_CAPACITY - 1].message,
            format!("failure {VULCAN_PLUGIN_RUNTIME_ERROR_CAPACITY}")
        );
        assert_eq!(manager.runtime_errors("quiet").len(), 1);
        assert!(manager.runtime_errors("absent").is_empty());
    }
}
