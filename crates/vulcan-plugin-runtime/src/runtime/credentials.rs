//! Credential edits stage a complete new revision and select it through the plugin's settings section.
//! 凭据编辑暂存完整新修订，并通过插件设置段选定它。
//! An edit is prepared first, so the host can prove the candidate's MCP servers ready before committing it.
//! 编辑先被准备，使宿主可在提交前证明候选 MCP 服务器已就绪。

#[cfg(test)]
mod tests;

use super::configuration::{ConfigurationOwner, configuration_owner};
use super::{VulcanPluginManagerState, VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use crate::{
    PinnedVulcanPluginCredentials, VulcanPluginCatalogSnapshot, VulcanPluginCredentialBackend,
    VulcanPluginCredentialBinding, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues, VulcanPluginDescriptor, VulcanPluginGenerationStore,
    VulcanPluginSettingsChange,
};
use std::sync::Arc;

/// Convert a store error into a runtime error without adding secret context.
/// 将存储错误转换为运行时错误，不附加秘密上下文。
fn store_error(error: crate::VulcanPluginStoreError) -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new(error.to_string())
}

/// The package authority whose settings section selects the plugin's credentials.
/// 其设置段选定插件凭据的插件包权威。
enum CredentialOwner<'a> {
    /// A managed user package; its descriptor mirrors the selected revision.
    /// 受管用户插件包；其描述符镜像所选修订。
    Managed(&'a crate::VulcanPluginManagedPackage),
    /// A bundled package; its descriptor binds the bundled record's revision.
    /// 内建插件包；其描述符绑定内建记录的修订。
    Bundled,
}

impl<'a> CredentialOwner<'a> {
    /// Classify `plugin` by the authority that owns its settings; unavailable rows are rejected.
    /// 按拥有其设置的权威对 `plugin` 分类；不可用条目被拒绝。
    fn of(plugin: &'a VulcanPluginDescriptor) -> Result<Self, VulcanPluginRuntimeError> {
        Ok(match configuration_owner(plugin)? {
            ConfigurationOwner::Managed(managed) => Self::Managed(managed),
            ConfigurationOwner::Bundled => Self::Bundled,
        })
    }

    /// Return the binding the plugin's servers currently read through.
    /// 返回插件服务器当前读取所经的绑定。
    fn binding(&self, plugin: &VulcanPluginDescriptor) -> VulcanPluginCredentialBinding {
        match self {
            Self::Managed(managed) => managed.credential_binding().clone(),
            // Catalog binding gives every server of a bundled plugin the same binding.
            // 目录绑定为内建插件的每个服务器提供相同绑定。
            Self::Bundled => plugin
                .mcp_servers
                .first()
                .map_or(VulcanPluginCredentialBinding::Unconfigured, |server| {
                    server.credential_binding.clone()
                }),
        }
    }
}

/// Marks one plugin as having an unfinished credential edit until dropped.
/// 在释放前将单个插件标记为存在未完成凭据编辑。
struct CredentialEditToken {
    /// Manager state holding the set of edited plugins.
    /// 保存被编辑插件集合的管理器状态。
    state: Arc<VulcanPluginManagerState>,
    /// Plugin whose edit this token owns.
    /// 此令牌所拥有编辑的插件。
    plugin_id: String,
}

impl Drop for CredentialEditToken {
    fn drop(&mut self) {
        // A poisoned set only stays conservative: collection keeps skipping this plugin.
        // 集合中毒时只会保持保守：回收继续跳过此插件。
        if let Ok(mut edits) = self.state.credential_edits.lock() {
            edits.remove(&self.plugin_id);
        }
    }
}

/// A staged and pinned credential revision of one plugin that is not yet selected.
/// 单个插件已暂存并固定、但尚未选定的凭据修订。
/// The host materializes the candidate's MCP servers, then commits or discards the edit.
/// 宿主物化候选的 MCP 服务器，然后提交或丢弃此编辑。
pub struct VulcanPluginCredentialEdit {
    /// Binding the merged values were read from; commit requires it to be unchanged.
    /// 读取合并值所经的绑定；提交要求其保持不变。
    expected: VulcanPluginCredentialBinding,
    /// Shared owner of the staged revision.
    /// 已暂存修订的共享所有者。
    pinned: PinnedVulcanPluginCredentials,
    /// The current descriptor rebound to the staged revision.
    /// 重新绑定至已暂存修订的当前描述符。
    candidate: VulcanPluginDescriptor,
    /// Store holding the settings record and the staged revision.
    /// 保存设置记录及已暂存修订的存储。
    store: VulcanPluginGenerationStore,
    /// Keeps collection away from this plugin until the edit ends.
    /// 在编辑结束前使回收远离此插件。
    _token: CredentialEditToken,
}

impl VulcanPluginCredentialEdit {
    /// Return the staged revision.
    /// 返回已暂存修订。
    pub fn revision(&self) -> &VulcanPluginCredentialRevision {
        self.pinned.record().revision()
    }

    /// Return the current descriptor rebound to the staged revision, for materializing candidate MCP servers.
    /// 返回重新绑定至已暂存修订的当前描述符，用于物化候选 MCP 服务器。
    pub fn candidate(&self) -> &VulcanPluginDescriptor {
        &self.candidate
    }
}

impl VulcanPluginRuntimeManager {
    /// Stage `changes` to the declared MCP secrets of `plugin_id` as one new, not yet selected credential revision.
    /// 将对 `plugin_id` 声明 MCP 秘密的 `changes` 暂存为单个尚未选定的新凭据修订。
    ///
    /// # Parameters
    /// 参数
    /// `changes` names declared slots only: Some sets a nonempty value and None clears it; unnamed slots keep their
    /// current values. `backend` is the secure store holding every revision.
    /// `changes` 只命名声明槽位：Some 设置非空值，None 清除；未命名槽位保留当前值。`backend` 为保存全部修订的安全存储。
    ///
    /// # Returns
    /// 返回值
    /// Returns the prepared edit; nothing is selected until `commit_plugin_credentials`.
    /// 返回已准备编辑；`commit_plugin_credentials` 之前不选定任何内容。
    pub fn prepare_plugin_credentials(
        &self,
        plugin_id: &str,
        changes: &VulcanPluginCredentialValues,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<VulcanPluginCredentialEdit, VulcanPluginRuntimeError> {
        let token = self.begin_credential_edit(plugin_id)?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        let owner = CredentialOwner::of(plugin)?;
        let keys = crate::generation_store::declared_keys(plugin);
        if keys.is_empty() {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` declares no MCP secrets."
            )));
        }
        if let Some(key) = changes.keys().find(|key| !keys.contains(key)) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "MCP secret `{}` is not declared by `{plugin_id}:{}`.",
                key.secret_key, key.server_id
            )));
        }
        if changes.values().flatten().any(String::is_empty) {
            return Err(VulcanPluginRuntimeError::new(
                "MCP secret values must not be empty; clear a secret instead.",
            ));
        }
        let expected = owner.binding(plugin);
        let values = merged_values(plugin_id, &keys, changes, &expected, backend)?;
        let store = self.credential_store(&owner)?;
        let revision = VulcanPluginCredentialRevision::allocate();
        let pinned = stage_and_pin(&store, plugin, &owner, keys, &revision, &values, backend)?;
        Ok(VulcanPluginCredentialEdit {
            expected,
            candidate: plugin.with_bound_credentials(pinned.clone()),
            pinned,
            store,
            _token: token,
        })
    }

    /// Select the staged revision of `edit` in its plugin's settings and republish the plugin bound to it.
    /// 在插件设置中选定 `edit` 的已暂存修订，并重新发布绑定至该修订的插件。
    /// Returns the published snapshot; on failure the candidate is collected through `backend`.
    /// 返回已发布快照；失败时经 `backend` 回收候选。
    pub fn commit_plugin_credentials(
        &self,
        edit: VulcanPluginCredentialEdit,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        let VulcanPluginCredentialEdit {
            expected,
            pinned,
            candidate,
            store,
            _token,
        } = edit;
        let plugin_id = candidate.id.clone();
        drop(candidate);
        let committed = self.select_credentials(&plugin_id, &store, &expected, pinned);
        if committed.is_err() {
            // The unreferenced candidate has no remaining owner, so collection removes it.
            // 未被引用的候选已无剩余所有者，因此回收会移除它。
            let _ = store.collect_unreachable_credentials(&plugin_id, backend);
        }
        committed
    }

    /// Abandon `edit` and collect its staged revision through `backend`.
    /// 放弃 `edit` 并经 `backend` 回收其已暂存修订。
    pub fn discard_plugin_credentials(
        &self,
        edit: VulcanPluginCredentialEdit,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let VulcanPluginCredentialEdit {
            pinned,
            candidate,
            store,
            _token,
            ..
        } = edit;
        let plugin_id = candidate.id.clone();
        drop((pinned, candidate));
        store
            .collect_unreachable_credentials(&plugin_id, backend)
            .map(|_| ())
            .map_err(store_error)
    }

    /// Collect the credential revisions of `plugin_id` that no record, transaction or live lease reaches.
    /// 回收 `plugin_id` 中没有记录、事务或实际租约可达的凭据修订。
    /// Returns the collected revisions; nothing is collected without a store or during an edit of the plugin.
    /// 返回已回收修订；没有存储或该插件正被编辑时不回收任何内容。
    pub fn collect_unreachable_plugin_credentials(
        &self,
        plugin_id: &str,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<Vec<VulcanPluginCredentialRevision>, VulcanPluginRuntimeError> {
        // Holding the set keeps an edit from starting while its namespace is swept.
        // 持有集合使编辑无法在其命名空间被清扫时开始。
        let edits = self.state.credential_edits.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin credential edits are poisoned.")
        })?;
        if edits.contains(plugin_id) {
            return Ok(Vec::new());
        }
        let Some(store) = VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(store_error)?
        else {
            return Ok(Vec::new());
        };
        store
            .collect_unreachable_credentials(plugin_id, backend)
            .map_err(store_error)
    }

    /// Collect unreachable credential revisions of every plugin with a credential namespace.
    /// 回收每个具有凭据命名空间的插件中不可达的凭据修订。
    /// Each plugin is attempted; the first failure is returned after all were tried.
    /// 每个插件均会尝试；全部尝试后返回首个失败。
    pub fn collect_all_unreachable_plugin_credentials(
        &self,
        backend: &dyn VulcanPluginCredentialBackend,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let Some(store) = VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(store_error)?
        else {
            return Ok(());
        };
        let plugin_ids = store.credential_plugin_ids().map_err(store_error)?;
        let mut first_error = None;
        for plugin_id in plugin_ids {
            if let Err(error) = self.collect_unreachable_plugin_credentials(&plugin_id, backend) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Mark `plugin_id` as being edited; a concurrent edit of the same plugin is refused.
    /// 将 `plugin_id` 标记为正在编辑；同一插件的并发编辑被拒绝。
    fn begin_credential_edit(
        &self,
        plugin_id: &str,
    ) -> Result<CredentialEditToken, VulcanPluginRuntimeError> {
        let mut edits = self.state.credential_edits.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin credential edits are poisoned.")
        })?;
        if !edits.insert(plugin_id.to_owned()) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "A credential edit of `{plugin_id}` is already in progress."
            )));
        }
        Ok(CredentialEditToken {
            state: Arc::clone(&self.state),
            plugin_id: plugin_id.to_owned(),
        })
    }

    /// Open the store holding `owner`'s settings; a bundled plugin's first edit initializes the user root.
    /// 打开保存 `owner` 设置的存储；内建插件的首次编辑初始化用户根目录。
    fn credential_store(
        &self,
        owner: &CredentialOwner<'_>,
    ) -> Result<VulcanPluginGenerationStore, VulcanPluginRuntimeError> {
        match owner {
            CredentialOwner::Managed(_) => {
                VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
                    .map_err(store_error)?
                    .ok_or_else(|| {
                        VulcanPluginRuntimeError::new("Managed plugin store is missing.")
                    })
            }
            CredentialOwner::Bundled => {
                std::fs::create_dir_all(&self.state.user_plugin_root).map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "Failed to create user plugin root: {error}"
                    ))
                })?;
                VulcanPluginGenerationStore::open(&self.state.user_plugin_root).map_err(store_error)
            }
        }
    }

    /// Select the pinned revision in the settings section of `plugin_id` and republish its descriptor.
    /// 在 `plugin_id` 的设置段中选定已固定修订，并重新发布其描述符。
    /// `expected` is the binding the merged values were read from; any other binding means a concurrent change.
    /// `expected` 为读取合并值所经的绑定；其他绑定表示存在并发变化。
    fn select_credentials(
        &self,
        plugin_id: &str,
        store: &VulcanPluginGenerationStore,
        expected: &VulcanPluginCredentialBinding,
        pinned: PinnedVulcanPluginCredentials,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&registration, None)?;
        let snapshot = self.snapshot();
        let current = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Plugin credential owner is no longer installed.")
        })?;
        let owner = CredentialOwner::of(current)?;
        if &owner.binding(current) != expected {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin credentials changed concurrently; retry the edit.",
            ));
        }
        let change = VulcanPluginSettingsChange {
            configuration: None,
            credential_revision: Some(pinned.record().revision().clone()),
        };
        let reconfigured = match owner {
            CredentialOwner::Managed(managed) => {
                let committed = store
                    .reconfigure(
                        plugin_id,
                        managed.activation_revision(),
                        managed.settings_revision(),
                        change,
                    )
                    .map_err(store_error)?;
                current.with_reconfigured_settings(&committed, Some(pinned))
            }
            CredentialOwner::Bundled => {
                let settings_revision = store
                    .activation(plugin_id)
                    .map_err(store_error)?
                    .map_or(0, |record| record.settings().revision);
                store
                    .configure_bundled(plugin_id, settings_revision, change)
                    .map_err(store_error)?;
                current.with_bundled_credentials(pinned)
            }
        }
        .map_err(VulcanPluginRuntimeError::new)?;
        self.publish_settings_edit(plugin_id, Some(reconfigured))
    }
}

/// Build complete values for `keys`: named slots take `changes`, the others keep the value read through `binding`.
/// 为 `keys` 构建完整值：已命名槽位取 `changes`，其余保留经 `binding` 读取的值。
fn merged_values(
    plugin_id: &str,
    keys: &[VulcanPluginCredentialKey],
    changes: &VulcanPluginCredentialValues,
    binding: &VulcanPluginCredentialBinding,
    backend: &dyn VulcanPluginCredentialBackend,
) -> Result<VulcanPluginCredentialValues, VulcanPluginRuntimeError> {
    let mut values = VulcanPluginCredentialValues::new();
    for key in keys {
        let value = match (changes.get(key), binding) {
            (Some(change), _) => change.clone(),
            (None, VulcanPluginCredentialBinding::Unconfigured) => None,
            // Backend error text is not propagated because a foreign backend may include secret values.
            // 不传播后端错误文本，因为外部后端可能包含秘密值。
            (None, VulcanPluginCredentialBinding::Revision(revision)) => {
                backend.read(plugin_id, revision, key).map_err(|_| {
                    VulcanPluginRuntimeError::new(
                        "Failed to read the current plugin credentials securely.",
                    )
                })?
            }
        };
        values.insert(key.clone(), value);
    }
    Ok(values)
}

/// Stage `values` as `revision` for `owner`'s package and pin the staged revision.
/// 为 `owner` 的插件包将 `values` 暂存为 `revision`，并固定已暂存修订。
/// A failed write discards the candidate; a failed pin collects it, since nothing references it.
/// 写入失败会丢弃候选；固定失败会回收它，因为没有任何引用。
fn stage_and_pin(
    store: &VulcanPluginGenerationStore,
    plugin: &VulcanPluginDescriptor,
    owner: &CredentialOwner<'_>,
    keys: Vec<VulcanPluginCredentialKey>,
    revision: &VulcanPluginCredentialRevision,
    values: &VulcanPluginCredentialValues,
    backend: &dyn VulcanPluginCredentialBackend,
) -> Result<PinnedVulcanPluginCredentials, VulcanPluginRuntimeError> {
    let mut staging = match owner {
        CredentialOwner::Managed(managed) => {
            store.begin_credential_staging(managed.generation(), revision.clone(), values)
        }
        CredentialOwner::Bundled => {
            store.begin_bundled_credential_staging(&plugin.id, keys, revision.clone(), values)
        }
    }
    .map_err(store_error)?;
    let staged = staging.write_and_finish(backend, values);
    // The exclusive writer lease must be released before discarding or pinning the revision.
    // 丢弃或固定修订之前必须释放排他写入租约。
    drop(staging);
    if let Err(error) = staged {
        let _ = store.discard_credential_staging(&plugin.id, revision, backend);
        return Err(store_error(error));
    }
    match owner {
        CredentialOwner::Managed(managed) => store.pin_credentials(managed.generation(), revision),
        CredentialOwner::Bundled => store.pin_bundled_credentials(&plugin.id, revision),
    }
    .map_err(|error| {
        let _ = store.collect_unreachable_credentials(&plugin.id, backend);
        store_error(error)
    })
}
