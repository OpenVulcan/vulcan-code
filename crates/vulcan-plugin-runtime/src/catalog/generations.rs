//! Managed selections override legacy package discovery without changing installation state.
//! 受管选择覆盖旧包发现结果，不改变安装状态。

use super::*;
use crate::{PinnedVulcanPluginGeneration, VulcanPluginGenerationStore};

#[cfg(test)]
mod tests;

/// One validated activation revision together with the actual lease protecting its immutable package.
/// 一个已校验激活修订及保护其不可变插件包的实际租约。
#[derive(Debug, Clone)]
pub struct VulcanPluginManagedPackage {
    /// Persistent selection revision, independent from the process-local routing generation.
    /// 持久选择修订，独立于进程内路由代次。
    activation_revision: u64,
    /// Real package ownership retained by every cloned descriptor or explicit generation handle.
    /// 每个克隆描述符或显式代次句柄保留的真实包所有权。
    generation: PinnedVulcanPluginGeneration,
    /// Approval captured from the same atomic selection, absent only for the legacy format.
    /// 从同一原子选择捕获的批准权限，仅旧格式缺失。
    approved_permissions: Option<Vec<String>>,
    /// Credential selection captured by the same persistent activation revision.
    /// 由相同持久激活修订捕获的凭据选择。
    credential_binding: crate::VulcanPluginCredentialBinding,
    /// Real credential retention remains live even for a managed package without MCP contributions.
    /// 即使受管插件包没有 MCP 贡献，实际凭据保留仍保持有效。
    _credential_lease: Option<crate::PinnedVulcanPluginCredentials>,
    /// Complete selected configuration shares the same activation revision as every other authority.
    /// 完整选定配置与其他全部权威共用相同激活修订。
    configuration: Option<std::sync::Arc<crate::VulcanPluginConfigurationSelection>>,
}

impl VulcanPluginManagedPackage {
    /// Returns the exact persistent activation revision observed during selection.
    /// 返回选择时观察到的精确持久激活修订。
    pub fn activation_revision(&self) -> u64 {
        self.activation_revision
    }

    /// Returns the real generation lease; clone it to retain file ownership outside the catalog.
    /// 返回真实代次租约；在目录之外保留文件所有权时必须克隆此句柄。
    pub fn generation(&self) -> &PinnedVulcanPluginGeneration {
        &self.generation
    }

    /// Return the exact selected approval; absence explicitly retains the version-one sidecar authority.
    /// 返回精确选定批准权限；缺失明确保留第一版附属文件权威。
    pub fn approved_permissions(&self) -> Option<&[String]> {
        self.approved_permissions.as_deref()
    }

    /// Borrow the selected credential authority without consulting another snapshot or file.
    /// 借用选定凭据权威，不查询其他快照或文件。
    pub fn credential_binding(&self) -> &crate::VulcanPluginCredentialBinding {
        &self.credential_binding
    }

    /// Borrow the complete frozen configuration authority; None explicitly retains the old file contract.
    /// 借用完整固定配置权威；None 明确保留旧文件契约。
    pub fn configuration(&self) -> Option<&crate::VulcanPluginConfigurationSelection> {
        self.configuration.as_deref()
    }
}

impl VulcanPluginMcpServerDescriptor {
    /// Borrow the descriptor's selected configuration; no manager refresh can replace this historical value.
    /// 借用描述符的选定配置；管理器刷新不能替换此历史值。
    pub fn configuration(&self) -> Option<&crate::VulcanPluginConfigurationSelection> {
        self.configuration.as_deref()
    }
}

/// Loads `root` with the existing user policies and `revision`, then applies exact managed selections.
/// 按既有用户策略及 `revision` 加载 `root`，再应用精确受管选择。
/// Returns a snapshot with retained leases; corruption blocks affected user packages instead of reviving legacy bytes.
/// 返回保留租约的快照；损坏会阻止受影响用户包，而不会重新启用旧包字节。
pub(super) fn load_user_catalog(
    root: &Path,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
    revision: u64,
) -> VulcanPluginCatalogSnapshot {
    // Legacy discovery remains explicit and does not migrate or rewrite existing packages.
    // 旧包发现保持显式，不迁移或改写既有插件包。
    let mut snapshot =
        discover_plugin_root_catalog(root, disabled_plugin_ids, enabled_plugin_ids, revision);
    // Opening an absent store must not turn an ordinary catalog refresh into an installation.
    // 打开缺失存储不能使普通目录刷新变成安装操作。
    let store = match VulcanPluginGenerationStore::open_existing(root) {
        Ok(Some(store)) => store,
        Ok(None) => return snapshot,
        Err(error) => {
            block_user_catalog(&mut snapshot, error.to_string());
            return snapshot;
        }
    };
    // Published namespaces also include tombstones and invalid records that must shadow old packages.
    // 已发布命名空间也包含必须遮蔽旧包的停用记录及无效记录。
    let published_ids = match store.activation_plugin_ids() {
        Ok(plugin_ids) => plugin_ids,
        Err(error) => {
            block_user_catalog(&mut snapshot, error.to_string());
            return snapshot;
        }
    };
    // Recheck legacy namespaces under the selection lock to cover publication during directory enumeration.
    // 在选择锁内复查旧包命名空间，覆盖目录枚举期间发生的发布。
    let plugin_ids = published_ids
        .iter()
        .chain(snapshot.plugins.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for plugin_id in plugin_ids {
        // Invalid legacy directory names are already unavailable and cannot be valid managed identities.
        // 无效旧包目录名称已不可用，也不可能是有效受管身份。
        if validate_identifier(&plugin_id, "managed plugin id").is_err() {
            continue;
        }
        match load_selected_package(
            &store,
            &snapshot.root,
            &plugin_id,
            published_ids.contains(&plugin_id),
            disabled_plugin_ids,
            enabled_plugin_ids,
        ) {
            Ok(ManagedSelection::Unmanaged) => {}
            Ok(ManagedSelection::Tombstone) => {
                snapshot.plugins.remove(&plugin_id);
            }
            Ok(ManagedSelection::Package(package)) => {
                snapshot.plugins.insert(plugin_id, *package);
            }
            Err(diagnostic) => {
                // An unavailable row has no execution surface or guessed package path.
                // 不可用条目不具有执行表面，也不猜测包路径。
                let package =
                    unavailable_managed_package(&plugin_id, &snapshot.root, diagnostic.clone());
                snapshot.diagnostics.push(format!(
                    "Managed plugin `{plugin_id}` is unavailable: {diagnostic}"
                ));
                snapshot.plugins.insert(plugin_id, package);
            }
        }
    }
    // The public catalog reserves contribution names after both user and bundled precedence resolve.
    // 公开目录在用户及内置优先级都确定后才保留贡献名称。
    snapshot
}

/// Explicit selection outcomes distinguish never-managed packages from durable removal and live ownership.
/// 显式选择结果区分从未受管的包、持久移除及活动所有权。
enum ManagedSelection {
    /// No managed record was observed for an unlisted legacy namespace.
    /// 对未列出的旧包命名空间没有观察到受管记录。
    Unmanaged,
    /// A durable null selection intentionally hides any remaining legacy directory.
    /// 持久空选择明确隐藏仍然存在的旧目录。
    Tombstone,
    /// Exact validated descriptor with its managed lease.
    /// 携带受管租约的精确已校验描述符。
    Package(Box<VulcanPluginDescriptor>),
}

/// Resolves one `plugin_id` under the store fence and applies only the supplied enablement policies.
/// 在存储屏障内解析一个 `plugin_id`，并仅应用传入的启停策略。
/// `was_listed` rejects vanished committed records; errors never select a legacy substitute.
/// `was_listed` 用于拒绝已提交记录消失；发生错误时绝不选择旧包替代。
fn load_selected_package(
    store: &VulcanPluginGenerationStore,
    catalog_root: &Path,
    plugin_id: &str,
    was_listed: bool,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
) -> Result<ManagedSelection, String> {
    // pin_active chooses the record and acquires real ownership before releasing its selection fence.
    // pin_active 选择记录并取得真实所有权后才释放选择屏障。
    let selected = store
        .pin_active(plugin_id)
        .map_err(|error| error.to_string())?;
    // Absence is permitted only for legacy namespaces never seen in the committed-record enumeration.
    // 只有从未出现在已提交记录枚举中的旧命名空间才允许缺失。
    let Some(activation) = selected.activation else {
        return if was_listed {
            Err(String::from("An enumerated activation record disappeared."))
        } else {
            Ok(ManagedSelection::Unmanaged)
        };
    };
    // None here is the validated explicit null selection returned by the storage API.
    // 此处 None 是存储接口返回的已校验显式空选择。
    let Some(generation) = selected.generation else {
        return Ok(ManagedSelection::Tombstone);
    };
    // Existing package validation remains the sole manifest and contribution authority.
    // 既有包校验继续作为清单及贡献的唯一权威。
    let mut package = load_vulcan_plugin_package(
        catalog_root,
        generation.package_root(),
        disabled_plugin_ids,
        enabled_plugin_ids,
    );
    // Compare the descriptor's validated identity and fingerprint with the exact pinned selection.
    // 将描述符已校验的身份及指纹与精确固定选择比较。
    let identity = generation.identity();
    if package.id != identity.plugin_id
        || package.version.as_deref() != Some(identity.version.as_str())
        || package.content_fingerprint != identity.content_fingerprint
    {
        return Err(String::from(
            "Managed package descriptor differs from its pinned generation identity.",
        ));
    }
    // Every contribution carries the binding from this exact selection, including later cloned server descriptors.
    // 每个贡献携带此精确选择的绑定，包括后续克隆的服务器描述符。
    let credential_binding = match activation.credential_revision() {
        Some(revision) => crate::VulcanPluginCredentialBinding::Revision(revision.clone()),
        None => crate::VulcanPluginCredentialBinding::Legacy,
    };
    // All cloned contributions share one frozen document rather than duplicating user configuration per server.
    // 全部克隆贡献共用单个固定文档，不为每个服务器重复用户配置。
    let configuration = activation.configuration().cloned().map(std::sync::Arc::new);
    for server in &mut package.mcp_servers {
        server.credential_binding = credential_binding.clone();
        server.credential_lease = selected.credentials.clone();
        server.configuration = configuration.clone();
    }
    package.managed_package = Some(VulcanPluginManagedPackage {
        activation_revision: activation.revision,
        approved_permissions: activation.approved_permissions().map(<[String]>::to_vec),
        credential_binding,
        _credential_lease: selected.credentials,
        configuration,
        generation,
    });
    Ok(ManagedSelection::Package(Box::new(package)))
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
/// `snapshot` is replaced in memory only; `diagnostic` explains why no legacy fallback is allowed.
/// `snapshot` 仅在内存中替换；`diagnostic` 说明为何不允许使用旧包回退。
fn block_user_catalog(snapshot: &mut VulcanPluginCatalogSnapshot, diagnostic: String) {
    snapshot.diagnostics.push(format!(
        "Managed plugin storage is unavailable: {diagnostic}"
    ));
    for package in snapshot.plugins.values_mut() {
        *package = unavailable_managed_package(&package.id, &snapshot.root, diagnostic.clone());
    }
}
