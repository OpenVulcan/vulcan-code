//! Settings edits advance only the settings revision; superseded credential revisions are collected by reachability.
//! 设置编辑只递增设置修订；被取代的凭据修订按可达性回收。

use super::{
    VulcanPluginActivation, VulcanPluginCredentialBackend, VulcanPluginCredentialPhase,
    VulcanPluginCredentialRevision, VulcanPluginGenerationStore, VulcanPluginPackageSelection,
    VulcanPluginSettings, VulcanPluginSettingsChange, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result,
};

impl VulcanPluginGenerationStore {
    /// Apply `change` to the settings of `plugin_id` when both expected revisions still match.
    /// 两个预期修订仍匹配时，将 `change` 应用到 `plugin_id` 的设置。
    /// The package selection and its revision stay unchanged; an unfinished activation transaction rejects the edit as Busy.
    /// 插件包选择及其修订保持不变；存在未完成激活事务时以 Busy 拒绝编辑。
    /// A new credential revision must be staged for the selected package; callers keep it pinned until this returns,
    /// so a concurrent reachability sweep cannot collect it before it is referenced.
    /// 新凭据修订须已为所选插件包暂存；调用方在此返回前保持其固定，使并发可达性回收无法在引用前回收它。
    /// Returns the committed record.
    /// 返回已提交记录。
    pub fn reconfigure(
        &self,
        plugin_id: &str,
        expected_package_revision: u64,
        expected_settings_revision: u64,
        change: VulcanPluginSettingsChange,
    ) -> Result<VulcanPluginActivation> {
        let _namespace = self.namespace_lock(plugin_id, false)?;
        // A transaction freezes its predecessor settings; editing them underneath would invalidate its handoff.
        // 事务固定其前驱设置；在其下编辑会使交接失效。
        self.ensure_no_pending_activation_locked(plugin_id)?;
        let current = self.read_activation(plugin_id)?.ok_or_else(|| {
            Error::new(
                Code::Conflict,
                "Plugin has no published selection to configure.",
            )
        })?;
        if current.revision != expected_package_revision
            || current.settings.revision != expected_settings_revision
        {
            return Err(Error::new(
                Code::Conflict,
                "Plugin activation or settings revision changed; inspect the current record.",
            ));
        }
        let identity = match &current.package {
            VulcanPluginPackageSelection::Generation { identity } => identity,
            VulcanPluginPackageSelection::Bundled {} => {
                return Err(Error::invalid(
                    "Bundled plugin settings are not stored by this release.",
                ));
            }
            VulcanPluginPackageSelection::Uninstalled {} => {
                return Err(Error::new(
                    Code::Conflict,
                    "An uninstalled plugin has no settings to configure.",
                ));
            }
        };
        // Validation runs against the exact package the record still selects.
        // 校验针对记录当前仍选择的精确插件包。
        let package = self.pin_locked(identity)?;
        let configuration = change
            .configuration
            .unwrap_or_else(|| current.settings.configuration.clone());
        package.validate_configuration(&configuration)?;
        let credential_revision = change
            .credential_revision
            .unwrap_or_else(|| current.settings.credential_revision.clone());
        if credential_revision != current.settings.credential_revision {
            self.validate_staged_credentials_locked(&package, &credential_revision)?;
        }
        let revision = current
            .settings
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::invalid("Plugin settings revision is exhausted."))?;
        let next = VulcanPluginActivation {
            settings: VulcanPluginSettings {
                revision,
                configuration,
                credential_revision,
            },
            ..current
        };
        self.write_activation(&next)?;
        Ok(next)
    }

    /// Collect every credential revision of `plugin_id` that nothing can reach any more; return the collected revisions.
    /// 回收 `plugin_id` 中已不可达的全部凭据修订；返回已回收修订。
    /// The current record's revision, revisions held by unfinished transactions and revisions with live reader or
    /// writer leases are kept; unfinished transaction-owned preparations are left to their transaction.
    /// 保留当前记录的修订、未完成事务持有的修订及存在实际读写租约的修订；事务所属的未完成准备交由其事务处理。
    pub fn collect_unreachable_credentials(
        &self,
        plugin_id: &str,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<Vec<VulcanPluginCredentialRevision>> {
        // A snapshot is enough: each cleanup re-checks reachability under its own exclusive fence.
        // 快照即可：每次清理都在自身排他屏障下重新检查可达性。
        let candidates = {
            let _namespace = self.namespace_lock(plugin_id, true)?;
            let current = self
                .read_activation(plugin_id)?
                .map(|record| record.settings.credential_revision);
            let mut candidates = Vec::new();
            for revision in self.credential_revisions_locked(plugin_id)? {
                if current.as_ref() == Some(&revision) {
                    continue;
                }
                let Some(record) = self.read_credential_record_locked(plugin_id, &revision)? else {
                    continue;
                };
                let retired = match record.phase() {
                    VulcanPluginCredentialPhase::Staged
                    | VulcanPluginCredentialPhase::Discarding => true,
                    VulcanPluginCredentialPhase::Preparing
                        if record.activation_request().is_none() =>
                    {
                        false
                    }
                    VulcanPluginCredentialPhase::Preparing
                    | VulcanPluginCredentialPhase::Discarded => {
                        continue;
                    }
                };
                candidates.push((revision, retired));
            }
            candidates
        };
        let mut collected = Vec::new();
        for (revision, retired) in candidates {
            match self.cleanup_credentials(plugin_id, &revision, backend, retired) {
                Ok(true) => collected.push(revision),
                Ok(false) => {}
                // Busy means a transaction or a live lease still reaches this revision.
                // Busy 表示仍有事务或实际租约可达此修订。
                Err(error) if error.code == Code::Busy => {}
                Err(error) => return Err(error),
            }
        }
        Ok(collected)
    }
}
