//! Startup admission preserves exact unfinished activation requests without replaying effects.
//! 启动入场保留精确未完成激活请求，不重放副作用。

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use crate::{
    VulcanPluginActivationTransactionPhase as Phase,
    VulcanPluginActivationTransactionRecord as Record, VulcanPluginGenerationStore as Store,
};
use std::collections::BTreeMap;
use std::path::Path;
use uuid::Uuid;

/// Manager-owned obligations captured once before the first catalog publication.
/// 首次目录发布前一次性捕获的管理器自有义务。
#[derive(Default)]
pub(super) struct ActivationRecovery {
    /// An unreadable namespace inventory prevents safely identifying affected plugins.
    /// 不可读命名空间清单导致无法安全确定受影响插件。
    root_error: Option<String>,
    /// Exact originals and opaque failures remain associated with their literal namespace.
    /// 精确原请求及不透明失败持续关联其字面命名空间。
    plugins: BTreeMap<String, PluginRecovery>,
}

/// Frozen identities retain uncertainty even if later filesystem inspection loses a record.
/// 固定身份保留不确定性，即使后续文件系统检查丢失记录。
#[derive(Default)]
struct PluginRecovery {
    /// Target and predecessor are immutable; checkpoint progress alone may change.
    /// 目标及前驱不可变；仅检查点进度可以变化。
    originals: BTreeMap<Uuid, Record>,
    /// Opaque inventory failures require repair and a new startup inspection.
    /// 不透明清单失败要求修复后重新启动检查。
    error: Option<String>,
}

impl ActivationRecovery {
    /// Inspect `root` read-only and return obligations; storage errors become admission diagnostics.
    /// 只读检查 `root` 并返回义务；存储错误转为入场诊断。
    fn inspect(root: &Path) -> Self {
        // Missing optional storage is the sole successful empty-store case.
        // 可选存储缺失是唯一成功的空存储情形。
        let store = match Store::open_existing(root) {
            Ok(Some(store)) => store,
            Ok(None) => return Self::default(),
            Err(error) => {
                return Self {
                    root_error: Some(error.to_string()),
                    ..Self::default()
                };
            }
        };
        // Do not replace an unreadable inventory with a guessed set of plugin IDs.
        // 不用猜测的插件标识集合替代不可读清单。
        let plugin_ids = match store.activation_transaction_plugin_ids() {
            Ok(ids) => ids,
            Err(error) => {
                return Self {
                    root_error: Some(error.to_string()),
                    ..Self::default()
                };
            }
        };
        // Healthy unrelated namespaces stay outside this recovery gate.
        // 健康的无关命名空间不进入此恢复门控。
        let mut recovery = Self::default();
        for plugin_id in plugin_ids {
            // Inspection validates both the index and every original body before admission.
            // 入场前，检查同时校验索引及每份原始正文。
            let pending = match store.activation_recovery_records(&plugin_id) {
                Ok(records) if records.is_empty() => continue,
                Ok(records) => PluginRecovery {
                    originals: records
                        .into_iter()
                        .map(|record| (record.request_id(), record))
                        .collect(),
                    error: None,
                },
                Err(error) => PluginRecovery {
                    error: Some(error.to_string()),
                    ..PluginRecovery::default()
                },
            };
            recovery.plugins.insert(plugin_id, pending);
        }
        recovery
    }

    /// Reconcile known `root` obligations by exact original identity; return no execution grants.
    /// 按精确原始身份核对已知 `root` 义务；不返回执行授权。
    fn reconcile(&mut self, root: &Path) {
        if self.root_error.is_some() || self.plugins.is_empty() {
            return;
        }
        // A previously observed store disappearing is uncertainty, not an empty startup.
        // 已观察到的存储消失表示不确定性，而非空启动。
        let store = match Store::open_existing(root) {
            Ok(Some(store)) => store,
            Ok(None) => {
                self.root_error = Some("Observed activation store disappeared.".into());
                return;
            }
            Err(error) => {
                self.root_error = Some(error.to_string());
                return;
            }
        };
        for (plugin_id, recovery) in &mut self.plugins {
            if recovery.error.is_some() {
                continue;
            }
            // A valid terminal original alone cannot override a damaged namespace index.
            // 单个有效终态原请求不能覆盖损坏的命名空间索引。
            let pending = match store.activation_recovery_records(plugin_id) {
                Ok(records) => records,
                Err(error) => {
                    recovery.error = Some(error.to_string());
                    continue;
                }
            };
            // Only exact originals may resolve obligations, never absence from the pending list.
            // 仅精确原请求可以解除义务，绝不依据待处理清单中的缺失。
            let mut completed = Vec::new();
            for (request_id, original) in &recovery.originals {
                match store.activation_transaction(plugin_id, *request_id) {
                    Ok(Some(current))
                        if current.target() == original.target()
                            && current.predecessor() == original.predecessor() =>
                    {
                        if matches!(current.phase(), Phase::Completed | Phase::Aborted) {
                            completed.push(*request_id);
                        }
                    }
                    Ok(_) => {
                        recovery.error = Some(format!(
                            "Original activation request {request_id} disappeared or changed identity."
                        ));
                        break;
                    }
                    Err(error) => {
                        recovery.error = Some(error.to_string());
                        break;
                    }
                }
            }
            if recovery.error.is_some() {
                continue;
            }
            for request_id in completed {
                recovery.originals.remove(&request_id);
            }
            for record in pending {
                recovery
                    .originals
                    .entry(record.request_id())
                    .or_insert(record);
            }
        }
        self.plugins
            .retain(|_, recovery| recovery.error.is_some() || !recovery.originals.is_empty());
    }
}

impl VulcanPluginRuntimeManager {
    /// Capture startup obligations once before runtime preparation; return only manager lock failures.
    /// 在运行时准备前一次性捕获启动义务；仅返回管理器锁失败。
    /// Journal failures gate plugin admission while management and inspection remain available.
    /// 日志失败门控插件入场，同时保持管理及检查可用。
    pub fn initialize_activation_recovery(&self) -> Result<(), VulcanPluginRuntimeError> {
        // Use the same admission gate as publication so a late caller cannot reset observations.
        // 使用与发布相同的入场门，避免迟到调用者重置观察。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        // Shared state preserves originals across every clone of the original manager.
        // 共享状态在原管理器的全部克隆之间保留原请求。
        let mut recovery = self.state.activation_recovery.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin recovery lock is poisoned.")
        })?;
        recovery.get_or_insert_with(|| ActivationRecovery::inspect(&self.state.user_plugin_root));
        Ok(())
    }

    /// Resolve recovery diagnostics under the caller's publication gate for `permit` and current roots.
    /// 在调用方发布门下，按 `permit` 及当前根目录解析恢复诊断。
    /// Return a global diagnostic and per-plugin diagnostics; a permit only publishes one disabled original.
    /// 返回全局及各插件诊断；许可仅允许发布一个停用原请求。
    pub(super) fn activation_recovery_diagnostics(
        &self,
        permit: Option<&Record>,
    ) -> Result<(Option<String>, BTreeMap<String, String>), VulcanPluginRuntimeError> {
        // Lazy initialization protects callers that do not use the service bootstrap helper.
        // 延迟初始化保护未使用服务启动辅助函数的调用方。
        let mut guard = self.state.activation_recovery.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin recovery lock is poisoned.")
        })?;
        // The first observation is never replaced by a later empty inventory.
        // 首次观察绝不被后续空清单替换。
        let recovery =
            guard.get_or_insert_with(|| ActivationRecovery::inspect(&self.state.user_plugin_root));
        recovery.reconcile(&self.state.user_plugin_root);
        // Each diagnostic identifies original requests without exposing configuration values.
        // 每条诊断标识原请求，不暴露配置值。
        let mut diagnostics = BTreeMap::new();
        for (plugin_id, pending) in &recovery.plugins {
            if pending.error.is_none()
                && pending.originals.len() == 1
                && permit.is_some_and(|record| {
                    record.target().plugin_id == *plugin_id
                        && pending
                            .originals
                            .get(&record.request_id())
                            .is_some_and(|original| {
                                original.target() == record.target()
                                    && original.predecessor() == record.predecessor()
                            })
                })
            {
                continue;
            }
            // Opaque errors take precedence over any readable subset of the journal.
            // 不透明错误优先于日志中任何可读子集。
            let diagnostic = match &pending.error {
                Some(error) => format!("Plugin activation recovery requires inspection: {error}"),
                None => format!(
                    "Plugin activation recovery is pending for original requests: {}.",
                    pending
                        .originals
                        .keys()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            diagnostics.insert(plugin_id.clone(), diagnostic);
        }
        Ok((
            recovery.root_error.as_ref().map(|error| {
                format!("Plugin activation recovery inventory is unavailable: {error}")
            }),
            diagnostics,
        ))
    }
}
