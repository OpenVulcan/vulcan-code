//! The user catalog consists solely of managed selections recorded in the generation store.
//! 用户目录仅由代次存储中记录的受管选择组成。

use super::*;
use crate::{PinnedVulcanPluginGeneration, VulcanPluginGenerationStore};

#[cfg(test)]
mod deactivation_tests;
#[cfg(test)]
mod tests;

/// Return an empty user snapshot for root labelled with revision; selections are projected into it afterwards.
/// 返回标记 revision 的 root 空用户快照；随后将选择投影至其中。
fn empty_user_snapshot(root: &Path, revision: u64) -> VulcanPluginCatalogSnapshot {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut snapshot = VulcanPluginCatalogSnapshot::empty(canonical_root);
    snapshot.revision = revision;
    snapshot
}

/// Load root/policies/revision through one exact published removal owner, preserving ordinary peer selection semantics.
/// 通过单个精确已发布移除所有者加载 root／policies／revision，保留普通其他插件选择语义。
/// original supplies actual retained File authority; returns the complete snapshot or its classified original read error.
/// original 提供实际保留文件权威；返回完整快照或带类别原读取错误。
pub(super) fn load_user_catalog_with_deactivation(
    root: &Path,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
    revision: u64,
    original: &crate::VulcanPluginDeactivationReservation,
) -> crate::VulcanPluginStoreResult<VulcanPluginCatalogSnapshot> {
    // An actual removal owner cannot authorize an absent or newly initialized root.
    // 实际移除所有者不能授权缺失或新初始化根。
    let store = VulcanPluginGenerationStore::open_existing(root)?.ok_or_else(|| {
        catalog_store_error(
            crate::VulcanPluginStoreErrorCode::Conflict,
            "Guarded catalog requires its original existing generation store.",
        )
    })?;
    original.validate_published_for_catalog(&store)?;
    let mut snapshot = empty_user_snapshot(root, revision);
    // Enumerating active filenames acquires no namespace and cannot deadlock the retained owner.
    // 枚举活动文件名不取得命名空间，不能使保留所有者自锁。
    let published_ids = store.activation_plugin_ids()?;
    project_user_catalog_selections(
        &mut snapshot,
        &store,
        &published_ids,
        disabled_plugin_ids,
        enabled_plugin_ids,
        Some(original),
    )?;
    Ok(snapshot)
}

/// Construct a classified read-only catalog failure using the public error fields and no allocated candidate identity.
/// 使用公开错误字段且不携带已分配候选身份，构造带类别只读目录失败。
/// Code and message classify the actual read failure; return no retained generation because catalog inspection allocates none.
/// code 及 message 分类实际读取失败；目录检查不分配代次，因此返回不保留代次的错误。
fn catalog_store_error(
    code: crate::VulcanPluginStoreErrorCode,
    message: impl Into<String>,
) -> crate::VulcanPluginStoreError {
    crate::VulcanPluginStoreError {
        code,
        message: message.into(),
        retained_generation: None,
    }
}

/// Project one selection inventory into snapshot; an optional removal owner authorizes exactly its own plugin.
/// 将一个选择清单投影至 snapshot；可选移除所有者仅授权其自身插件。
/// Return owner/target errors directly; unrelated namespace failures preserve ordinary unavailable rows.
/// 直接返回所有者／目标错误；无关命名空间失败保留普通不可用条目。
fn project_user_catalog_selections(
    snapshot: &mut VulcanPluginCatalogSnapshot,
    store: &VulcanPluginGenerationStore,
    published_ids: &BTreeSet<String>,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
    guarded: Option<&crate::VulcanPluginDeactivationReservation>,
) -> crate::VulcanPluginStoreResult<()> {
    // The explicit guarded ID survives uninstall even when active enumeration no longer contains it.
    // 即使活动枚举不再包含此插件，显式守卫标识仍在卸载之后存续。
    let plugin_ids = published_ids
        .iter()
        .map(String::as_str)
        .chain(guarded.map(|original| original.original().plugin_id.as_str()))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    for plugin_id in plugin_ids {
        if validate_identifier(&plugin_id, "managed plugin id").is_err() {
            continue;
        }
        // Dispatch follows the declared literal ID; failure never switches to ordinary pinning.
        // 分派遵循声明字面标识；失败绝不切换至普通固定操作。
        let selection = if let Some(original) =
            guarded.filter(|original| original.original().plugin_id == plugin_id)
        {
            // The original namespace stays inside its actual guard while full selection and real pins are acquired.
            // 取得完整选择及真实固定句柄期间，原命名空间保持在其实际守卫内部。
            let selected = original.pin_active_for_catalog(store)?;
            Ok(materialize_selected_package(
                selected,
                &snapshot.root,
                published_ids.contains(&plugin_id),
                disabled_plugin_ids,
                enabled_plugin_ids,
            )
            .map_err(|message| {
                catalog_store_error(crate::VulcanPluginStoreErrorCode::Invalid, message)
            })?)
        } else {
            load_selected_package(
                store,
                &snapshot.root,
                &plugin_id,
                published_ids.contains(&plugin_id),
                disabled_plugin_ids,
                enabled_plugin_ids,
            )
        };
        match selection {
            Ok(ManagedSelection::Unmanaged | ManagedSelection::Tombstone) => {}
            Ok(ManagedSelection::Package(package)) => {
                snapshot.plugins.insert(plugin_id, *package);
            }
            Err(diagnostic) => {
                // Ordinary peer failures remain explicit unavailable rows.
                // 普通其他插件失败仍是显式不可用条目。
                let package =
                    unavailable_managed_package(&plugin_id, &snapshot.root, diagnostic.clone());
                snapshot.diagnostics.push(format!(
                    "Managed plugin `{plugin_id}` is unavailable: {diagnostic}"
                ));
                snapshot.plugins.insert(plugin_id, package);
            }
        }
    }
    Ok(())
}

/// Materialize a fully pinned selection into the sole None/tombstone/package projection without taking another namespace.
/// 将完整已固定选择物化为唯一缺失／停用／插件包投影，不取得另一命名空间。
/// The catalog root, published observation and policies retain their ordinary meaning; errors describe invalid selected bytes.
/// 目录根、发布观察及策略保留普通含义；错误描述无效选定字节。
fn materialize_selected_package(
    selected: crate::VulcanPluginActiveGeneration,
    catalog_root: &Path,
    was_listed: bool,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
) -> Result<ManagedSelection, String> {
    // Actual absence never substitutes for a previously enumerated published record.
    // 实际缺失绝不替代先前枚举出的已发布记录。
    let Some(activation) = selected.activation else {
        return if was_listed {
            Err(String::from("An enumerated activation record disappeared."))
        } else {
            Ok(ManagedSelection::Unmanaged)
        };
    };
    // A bundled settings record belongs to the bundled root and selects no user package.
    // 内建设置记录属于内建根，不选择任何用户插件包。
    if matches!(
        activation.package(),
        crate::VulcanPluginPackageSelection::Bundled {}
    ) {
        return Ok(ManagedSelection::Unmanaged);
    }
    // The validated selector distinguishes a real tombstone from unmanaged absence.
    // 已校验选择器区分真实停用记录和非受管缺失。
    let Some(generation) = selected.generation else {
        return Ok(ManagedSelection::Tombstone);
    };
    // The store pins credentials together with every selected package.
    // 存储随每个已选插件包一起固定凭据。
    let credentials = selected
        .credentials
        .ok_or_else(|| String::from("Selected package credentials are not pinned."))?;
    load_pinned_package(
        catalog_root,
        &activation,
        generation,
        credentials,
        disabled_plugin_ids,
        enabled_plugin_ids,
    )
    .map(|package| ManagedSelection::Package(Box::new(package)))
}

/// One validated activation revision together with the actual lease protecting its immutable package.
/// 一个已校验激活修订及保护其不可变插件包的实际租约。
#[derive(Debug, Clone)]
pub struct VulcanPluginManagedPackage {
    /// Persistent selection revision, independent from the process-local routing generation.
    /// 持久选择修订，独立于进程内路由代次。
    activation_revision: u64,
    /// Settings revision of the record this descriptor mirrors; settings edits republish the descriptor.
    /// 此描述符镜像记录的设置修订；设置编辑会重新发布描述符。
    settings_revision: u64,
    /// Real package ownership retained by every cloned descriptor or explicit generation handle.
    /// 每个克隆描述符或显式代次句柄保留的真实包所有权。
    generation: PinnedVulcanPluginGeneration,
    /// Approval captured from the same atomic selection.
    /// 从同一原子选择捕获的批准权限。
    approved_permissions: Vec<String>,
    /// Credential selection captured by the same persistent activation revision.
    /// 由相同持久激活修订捕获的凭据选择。
    credential_binding: crate::VulcanPluginCredentialBinding,
    /// Real credential retention remains live even for a managed package without MCP contributions.
    /// 即使受管插件包没有 MCP 贡献，实际凭据保留仍保持有效。
    _credential_lease: crate::PinnedVulcanPluginCredentials,
    /// Complete selected configuration shares the same activation revision as every other authority.
    /// 完整选定配置与其他全部权威共用相同激活修订。
    configuration: std::sync::Arc<crate::VulcanPluginConfigurationSelection>,
}

impl VulcanPluginManagedPackage {
    /// Compare every frozen selection authority against activation, including configuration and credentials.
    /// 比较所有固定选择权威与 activation，包括配置及凭据。
    /// Returns false for any mismatch; equal revisions alone never authorize a route handoff.
    /// 任何不匹配均返回假；仅修订相等绝不授权路由交接。
    pub fn matches_activation(&self, activation: &crate::VulcanPluginActivation) -> bool {
        self.activation_revision == activation.revision
            && self.settings_revision == activation.settings().revision
            && self.generation.identity().plugin_id == activation.plugin_id
            && activation.generation() == Some(self.generation.identity())
            && self.approved_permissions() == activation.approved_permissions()
            && self.configuration() == activation.configuration()
            && activation.credential_revision().is_some_and(|revision| {
                self.credential_binding
                    == crate::VulcanPluginCredentialBinding::Revision(revision.clone())
            })
    }

    /// Returns the exact persistent activation revision observed during selection.
    /// 返回选择时观察到的精确持久激活修订。
    pub fn activation_revision(&self) -> u64 {
        self.activation_revision
    }

    /// Returns the settings revision of the mirrored record.
    /// 返回所镜像记录的设置修订。
    pub fn settings_revision(&self) -> u64 {
        self.settings_revision
    }

    /// Returns the real generation lease; clone it to retain file ownership outside the catalog.
    /// 返回真实代次租约；在目录之外保留文件所有权时必须克隆此句柄。
    pub fn generation(&self) -> &PinnedVulcanPluginGeneration {
        &self.generation
    }

    /// Return the exact selected approval.
    /// 返回精确选定批准权限。
    pub fn approved_permissions(&self) -> &[String] {
        &self.approved_permissions
    }

    /// Borrow the selected credential authority without consulting another snapshot or file.
    /// 借用选定凭据权威，不查询其他快照或文件。
    pub fn credential_binding(&self) -> &crate::VulcanPluginCredentialBinding {
        &self.credential_binding
    }

    /// Borrow the complete frozen configuration authority.
    /// 借用完整固定配置权威。
    pub fn configuration(&self) -> &crate::VulcanPluginConfigurationSelection {
        &self.configuration
    }
}

impl VulcanPluginDescriptor {
    /// Return a copy of this managed descriptor mirroring `activation`, whose settings were edited in place.
    /// 返回镜像 `activation` 的此受管描述符副本，其设置已被就地编辑。
    /// The package selection and approval must be unchanged; `credentials` pins a newly selected credential revision,
    /// and without it the credential binding must be unchanged.
    /// 插件包选择及批准必须不变；`credentials` 固定新选定的凭据修订，未提供时凭据绑定必须不变。
    pub(crate) fn with_reconfigured_settings(
        &self,
        activation: &crate::VulcanPluginActivation,
        credentials: Option<crate::PinnedVulcanPluginCredentials>,
    ) -> Result<Self, String> {
        let managed = self
            .managed_package
            .as_ref()
            .ok_or_else(|| String::from("Only a managed plugin descriptor mirrors settings."))?;
        let selected_credentials = activation
            .credential_revision()
            .ok_or_else(|| String::from("A managed selection has no credential revision."))?;
        let credentials_match = match &credentials {
            Some(pinned) => pinned.record().revision() == selected_credentials,
            None => {
                managed.credential_binding
                    == crate::VulcanPluginCredentialBinding::Revision(selected_credentials.clone())
            }
        };
        if managed.activation_revision != activation.revision
            || activation.generation() != Some(managed.generation.identity())
            || managed.approved_permissions() != activation.approved_permissions()
            || !credentials_match
        {
            return Err(String::from(
                "Reconfigured settings belong to a different package selection.",
            ));
        }
        let configuration = Arc::new(activation.configuration().clone());
        let mut descriptor = self.clone();
        for server in &mut descriptor.mcp_servers {
            server.configuration = Some(Arc::clone(&configuration));
        }
        if let Some(pinned) = credentials {
            descriptor.bind_credentials(pinned);
        }
        if let Some(managed) = descriptor.managed_package.as_mut() {
            managed.settings_revision = activation.settings().revision;
            managed.configuration = configuration;
        }
        Ok(descriptor)
    }

    /// Return a copy of this bundled descriptor bound to the newly selected `credentials`.
    /// 返回绑定至新选定 `credentials` 的此内建描述符副本。
    pub(crate) fn with_bundled_credentials(
        &self,
        credentials: crate::PinnedVulcanPluginCredentials,
    ) -> Result<Self, String> {
        if self.origin != crate::VulcanPluginPackageOrigin::Bundled
            || self.managed_package.is_some()
        {
            return Err(String::from(
                "Only a bundled plugin descriptor binds bundled credentials.",
            ));
        }
        Ok(self.with_bound_credentials(credentials))
    }

    /// Return a copy of this descriptor whose servers read through `credentials`, without any selection check.
    /// 返回其服务器经 `credentials` 读取的此描述符副本，不做任何选择检查。
    /// Only a not yet committed candidate uses it; published descriptors come from the checked constructors.
    /// 仅尚未提交的候选使用它；已发布描述符来自经检查的构造函数。
    pub(crate) fn with_bound_credentials(
        &self,
        credentials: crate::PinnedVulcanPluginCredentials,
    ) -> Self {
        let mut descriptor = self.clone();
        descriptor.bind_credentials(credentials);
        descriptor
    }

    /// Bind every MCP server, and the managed package if any, to `credentials`.
    /// 将每个 MCP 服务器及受管插件包（如有）绑定至 `credentials`。
    fn bind_credentials(&mut self, credentials: crate::PinnedVulcanPluginCredentials) {
        let binding =
            crate::VulcanPluginCredentialBinding::Revision(credentials.record().revision().clone());
        for server in &mut self.mcp_servers {
            server.credential_binding = binding.clone();
            server.credential_lease = Some(credentials.clone());
        }
        if let Some(managed) = self.managed_package.as_mut() {
            managed.credential_binding = binding;
            managed._credential_lease = credentials;
        }
    }
}

impl VulcanPluginMcpServerDescriptor {
    /// Borrow the descriptor's selected configuration; no manager refresh can replace this historical value.
    /// 借用描述符的选定配置；管理器刷新不能替换此历史值。
    pub fn configuration(&self) -> Option<&crate::VulcanPluginConfigurationSelection> {
        self.configuration.as_deref()
    }
}

/// Loads the managed selections recorded under `root` with the existing user policies and `revision`.
/// 按既有用户策略及 `revision` 加载 `root` 下记录的受管选择。
/// Returns a snapshot with retained leases; a storage failure is reported as a diagnostic.
/// 返回保留租约的快照；存储失败以诊断形式报告。
pub(super) fn load_user_catalog(
    root: &Path,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
    revision: u64,
) -> VulcanPluginCatalogSnapshot {
    let mut snapshot = empty_user_snapshot(root, revision);
    // Discovery never initializes private managed storage.
    // 发现绝不初始化私有受管存储。
    let store = match VulcanPluginGenerationStore::open_existing(root) {
        Ok(Some(store)) => store,
        Ok(None) => return snapshot,
        Err(error) => {
            block_user_catalog(&mut snapshot, error.to_string());
            return snapshot;
        }
    };
    // Physical enumeration remains the sole published-namespace inventory.
    // 物理枚举仍是唯一已发布命名空间清单。
    let published_ids = match store.activation_plugin_ids() {
        Ok(plugin_ids) => plugin_ids,
        Err(error) => {
            block_user_catalog(&mut snapshot, error.to_string());
            return snapshot;
        }
    };
    if let Err(error) = project_user_catalog_selections(
        &mut snapshot,
        &store,
        &published_ids,
        disabled_plugin_ids,
        enabled_plugin_ids,
        None,
    ) {
        block_user_catalog(&mut snapshot, error.to_string());
    }
    snapshot
}

/// Explicit selection outcomes distinguish an absent or bundled record from durable removal and live ownership.
/// 显式选择结果区分缺失或内建记录、持久移除及活动所有权。
enum ManagedSelection {
    /// No record exists, or the record holds bundled settings and selects no user package.
    /// 没有记录，或记录保存内建设置且不选择用户插件包。
    Unmanaged,
    /// A durable null selection records an uninstalled plugin.
    /// 持久空选择记录已卸载插件。
    Tombstone,
    /// Exact validated descriptor with its managed lease.
    /// 携带受管租约的精确已校验描述符。
    Package(Box<VulcanPluginDescriptor>),
}

/// Load the pinned selection under catalog_root using the exact enabled and disabled policies.
/// 按精确启用及停用策略，在 catalog_root 下加载已固定选择。
/// Returns one descriptor retaining generation and credentials; both public discovery and candidate inspection use it.
/// 返回保留 generation 及 credentials 的单个描述符；公开发现及候选检查共同使用此入口。
pub(crate) fn load_pinned_package(
    catalog_root: &Path,
    activation: &crate::VulcanPluginActivation,
    generation: PinnedVulcanPluginGeneration,
    credentials: crate::PinnedVulcanPluginCredentials,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
) -> Result<VulcanPluginDescriptor, String> {
    // Package parsing remains the sole manifest and contribution authority for both selection paths.
    // 插件包解析仍是两条选择路径共同的唯一清单及贡献权威。
    let mut package = load_vulcan_plugin_package(
        catalog_root,
        generation.package_root(),
        disabled_plugin_ids,
        enabled_plugin_ids,
    );
    // A snapshot must never combine one persistent selection with another physical package.
    // 快照绝不能把一个持久选择与另一个物理插件包组合。
    let identity = generation.identity();
    if activation.generation() != Some(identity)
        || activation.plugin_id != identity.plugin_id
        || package.id != identity.plugin_id
        || package.version.as_deref() != Some(identity.version.as_str())
        || package.content_fingerprint != identity.content_fingerprint
    {
        return Err(String::from(
            "Managed package descriptor differs from its pinned generation identity.",
        ));
    }
    // Frozen credentials come from this exact selection, never from the currently published revision.
    // 固定凭据来自此精确选择，绝不来自当前发布修订。
    let credential_binding = crate::VulcanPluginCredentialBinding::Revision(
        activation
            .generation_credential_revision()
            .map_err(|error| error.to_string())?
            .clone(),
    );
    // All contributions share the same complete selected document and actual credential reader.
    // 全部贡献共享同一完整选定文档及实际凭据读者。
    let configuration = Arc::new(activation.configuration().clone());
    for server in &mut package.mcp_servers {
        server.credential_binding = credential_binding.clone();
        server.credential_lease = Some(credentials.clone());
        server.configuration = Some(Arc::clone(&configuration));
    }
    package.managed_package = Some(VulcanPluginManagedPackage {
        activation_revision: activation.revision,
        settings_revision: activation.settings().revision,
        approved_permissions: activation.approved_permissions().to_vec(),
        credential_binding,
        _credential_lease: credentials,
        configuration,
        generation,
    });
    Ok(package)
}

/// Resolves one `plugin_id` under the store fence and applies only the supplied enablement policies.
/// 在存储屏障内解析一个 `plugin_id`，并仅应用传入的启停策略。
/// `was_listed` rejects vanished committed records; errors never select a substitute package.
/// `was_listed` 用于拒绝已提交记录消失；发生错误时绝不选择替代插件包。
fn load_selected_package(
    store: &VulcanPluginGenerationStore,
    catalog_root: &Path,
    plugin_id: &str,
    was_listed: bool,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
) -> Result<ManagedSelection, String> {
    // Ordinary callers acquire their own namespace fence before the unique selection materializer.
    // 普通调用方在唯一选择物化器之前取得自身命名空间屏障。
    let selected = store
        .pin_active(plugin_id)
        .map_err(|error| error.to_string())?;
    materialize_selected_package(
        selected,
        catalog_root,
        was_listed,
        disabled_plugin_ids,
        enabled_plugin_ids,
    )
}

/// Builds a non-executable diagnostic row using `catalog_root`, not an unverified package target.
/// 使用 `catalog_root` 构建不可执行诊断条目，不使用未经校验的包目标。
/// The supplied `plugin_id` and `diagnostic` identify the failed selection without exposing contributions.
/// 传入的 `plugin_id` 与 `diagnostic` 标识失败选择，不暴露任何贡献。
fn unavailable_managed_package(
    plugin_id: &str,
    catalog_root: &Path,
    diagnostic: String,
) -> VulcanPluginDescriptor {
    VulcanPluginDescriptor {
        id: plugin_id.to_owned(),
        version: None,
        package_root: catalog_root.to_path_buf(),
        managed_package: None,
        origin: VulcanPluginPackageOrigin::UserInstalled,
        content_fingerprint: String::new(),
        availability: VulcanPluginAvailability::Unavailable,
        desired_enabled: false,
        runtime_state: VulcanPluginRuntimeState::Unavailable,
        runtime_execution_available: false,
        generation: 0,
        active_session_count: 0,
        runtime_errors: Vec::new(),
        name: plugin_id.to_owned(),
        capability: String::from("Managed plugin selection is unavailable."),
        locales: BTreeMap::new(),
        tools: Vec::new(),
        hooks: Vec::new(),
        mcp_servers: Vec::new(),
        prompt_contributions: Vec::new(),
        skills: Vec::new(),
        agents: Vec::new(),
        manifest: None,
        diagnostics: vec![diagnostic],
    }
}

/// Blocks user discovery after an unscoped storage failure, retaining diagnostic rows and bundled independence.
/// 无法确定影响范围的存储失败会阻止用户包发现，同时保留诊断条目及内置包独立性。
/// `snapshot` is replaced in memory only; `diagnostic` explains the storage failure.
/// `snapshot` 仅在内存中替换；`diagnostic` 说明存储失败原因。
fn block_user_catalog(snapshot: &mut VulcanPluginCatalogSnapshot, diagnostic: String) {
    snapshot.diagnostics.push(format!(
        "Managed plugin storage is unavailable: {diagnostic}"
    ));
    for package in snapshot.plugins.values_mut() {
        *package = unavailable_managed_package(&package.id, &snapshot.root, diagnostic.clone());
    }
}
