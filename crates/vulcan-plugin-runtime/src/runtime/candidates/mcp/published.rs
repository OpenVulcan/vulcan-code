//! Ordinary published MCP ownership freezes a full managed selection without retaining an update request.
//! 普通已发布 MCP 所有权固定完整受管选择，不保留更新请求。

use super::*;
use crate::generation_store::{PublishedDispatchFence, VulcanPluginActiveGeneration};
use crate::runtime::update_drain::{
    validate_ordinary_handoff_admission, validate_ordinary_pending_admission,
};
use crate::{
    VulcanPluginActivation, VulcanPluginBridgeLeaseScope, VulcanPluginCatalogSnapshot,
    VulcanPluginGenerationStore, VulcanPluginMcpServerManifest, VulcanPluginPackageOrigin,
    VulcanPluginRuntimeState,
};
use std::sync::{MutexGuard, Weak};

/// Opaque immutable source values carry no execution owner, File, physical pin or actual-launch assertion.
/// 不透明不可变来源值不携带执行所有者、File、物理固定句柄或实际启动断言。
/// Native hosts must separately bind this source to their same actual launch inputs and transport observation.
/// 原生宿主必须另行将此来源绑定至其同次实际启动输入及传输观测。
#[derive(Clone)]
pub struct VulcanPluginPublishedMcpProfile {
    /// Values share their original source identity without keeping native execution resources alive.
    /// 值共享其原来源身份，不延长原生执行资源寿命。
    inner: Arc<PublishedMcpProfileValues>,
}

/// Pure frozen source values are intentionally neither serialized nor formatted with secrets.
/// 纯固定来源值有意不序列化，也不带秘密格式化。
struct PublishedMcpProfileValues {
    /// Clone of the exact original registration owns only its weak manager issuer identity.
    /// 精确原注册的克隆仅持有其弱管理器签发者身份。
    authority: VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Original canonical store owns only its validated path, never an open namespace or execution File.
    /// 原规范存储仅持有已校验路径，绝不持有打开的命名空间或执行 File。
    store: VulcanPluginGenerationStore,
    /// Weak original catalog identity can validate publication without prolonging descriptor-owned pins.
    /// 原目录弱身份可校验发布，而不延长描述符自有固定句柄寿命。
    expected: Weak<VulcanPluginCatalogSnapshot>,
    /// Full actual selection includes immutable approval, credentials and configuration values.
    /// 完整实际选择包含不可变批准、凭据及配置值。
    selected: VulcanPluginActivation,
    /// Routing generation is frozen together with the source, never substituted for package ownership.
    /// 路由代次与来源共同固定，绝不替代插件包所有权。
    generation: u64,
    /// Declaration values retain no descriptor-owned package or credential File handles.
    /// 声明值不保留描述符自有插件包或凭据 File 句柄。
    _declarations: Vec<VulcanPluginMcpServerManifest>,
    /// Exact launcher values contain paths and host inputs but no physical environment owner.
    /// 精确启动器值包含路径及宿主输入，但不包含物理环境所有者。
    stdio_launchers: BTreeMap<String, VulcanPluginMcpStdioLaunchSpec>,
}

impl VulcanPluginPublishedMcpProfile {
    /// Compare other with this same opaque source object; no raw selected values or launch evidence are returned.
    /// 比较 other 是否为此同一不透明来源对象；不返回原始选定值或启动证据。
    /// Return true only for clones of this original profile, not equivalent IDs or separately prepared owners.
    /// 仅此原画像的克隆返回真，不接受相等 ID 或另行准备的所有者。
    pub fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Fence this same original full source for synchronous publication without owning execution resources.
    /// 为同步发布对此同一原完整来源建立屏障，不拥有执行资源。
    /// manager, authority and catalog must remain the original current objects; return only source exclusion.
    /// manager、authority 及 catalog 必须仍为原当前对象；仅返回来源互斥。
    /// The native host must independently hold its actual input, transport and business guards before recording publication.
    /// 原生宿主须在记录发布之前独立持有实际输入、传输及业务守卫。
    /// No native startup, full extension witness or restoration permission is issued by this guard.
    /// 此守卫不签发原生启动、完整扩展见证或恢复许可。
    pub fn begin_publication<'a>(
        &'a self,
        manager: &'a VulcanPluginRuntimeManager,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
    ) -> Result<VulcanPluginPublishedMcpStartGuard<'a>, VulcanPluginRuntimeError> {
        self.validate_host(authority)?;
        if !authority.matches_manager(manager) {
            return Err(VulcanPluginRuntimeError::new(
                "Published MCP manager changed.",
            ));
        }
        if !Weak::ptr_eq(&self.inner.expected, &Arc::downgrade(catalog)) {
            return Err(VulcanPluginRuntimeError::new(
                "Published MCP requires its original publication catalog.",
            ));
        }
        // Retain original registration before Store and native admission; full publication already owns its outer host slot.
        // 在存储及原生入场之前保留原登记；完整发布已拥有其外层宿主槽。
        let registration = manager
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Published MCP registration gate is poisoned.")
            })?;
        // The supplied Arc must still be the actual current catalog under the same manager exclusion.
        // 同一管理器互斥下，传入 Arc 必须仍为实际当前目录。
        let current = validate_current_snapshot(manager, catalog, &self.inner.selected.plugin_id)?;
        if current.generation != self.inner.generation {
            return Err(VulcanPluginRuntimeError::new(
                "Published MCP routing generation changed.",
            ));
        }
        // The original path-only Store obtains the same actual shared File, never a newly inferred namespace.
        // 原仅路径存储取得同一实际共享 File，绝不取得新推断的命名空间。
        let dispatch = self
            .inner
            .store
            .acquire_published_dispatch_fence(&self.inner.selected)
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?;
        validate_dispatch_admission(&dispatch)?;
        // Validate the complete current physical descriptor through the already-held namespace helper.
        // 经已持有命名空间辅助方法校验完整当前物理描述符。
        let active = dispatch
            .pin_selected()
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?;
        validate_full_pins(&current, &self.inner.selected, &active)?;
        Ok(VulcanPluginPublishedMcpStartGuard {
            _registration: registration,
            _dispatch: dispatch,
            profile: self,
        })
    }

    /// Check authority against the original private issuer; return success without exposing source values.
    /// 对照原私有签发者检查 authority；返回成功，不暴露来源值。
    fn validate_host(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !self.inner.authority.same_issuer(authority) {
            return Err(VulcanPluginRuntimeError::new(
                "Published MCP requires its original registered native host.",
            ));
        }
        Ok(())
    }
}

/// Actual ordinary resources own full managed package and credentials until native execution retires.
/// 实际普通资源持有完整受管插件包及凭据，直至原生执行退役。
/// Clones retain the same environment File; this type grants neither target nor Aborted restoration authority.
/// 克隆保留同一环境 File；此类型不授予目标或 Aborted 恢复权威。
#[derive(Clone)]
pub struct VulcanPluginPreparedPublishedMcp {
    /// Same original resource bundle must be passed to every actual lifecycle and transport owner.
    /// 同一原资源包必须传入每个实际生命周期及传输所有者。
    inner: Arc<PublishedMcpResources>,
}

/// Request-free ordinary resource ownership is distinct from prepared activation target resources.
/// 无请求的普通资源所有权独立于已准备激活目标资源。
struct PublishedMcpResources {
    /// Actual full descriptor retains original package and credential pins.
    /// 实际完整描述符保留原插件包及凭据固定句柄。
    plugin: VulcanPluginDescriptor,
    /// Independent pure source clone may survive real resource retirement without blocking it.
    /// 独立纯来源克隆可跨真实资源退役存续，不阻挡退役。
    profile: VulcanPluginPublishedMcpProfile,
    /// Actual shared environment File is acquired before stdio preparation and outlives native work.
    /// 实际共享环境 File 在标准输入输出准备前取得，并跨原生工作存续。
    _environment: Option<std::fs::File>,
}

/// Borrowed source exclusion retains the original catalog admission and cross-store writer fence.
/// 借用来源互斥保留原目录准入及跨存储写入屏障。
/// Keep only through synchronous owner registration or full publication; never await or treat it as native proof.
/// 仅持有至同步所有者登记或完整发布；绝不等待或将其视为原生证明。
pub struct VulcanPluginPublishedMcpStartGuard<'a> {
    /// Short original manager registration gate orders local publication against actual dispatch.
    /// 短时原管理器注册门将本地发布与实际派发排序。
    _registration: MutexGuard<'a, ()>,
    /// Same store's actual namespace File excludes writers from other manager instances as well.
    /// 同存储的实际命名空间 File 同时排除其他管理器实例的写入者。
    _dispatch: PublishedDispatchFence<'a>,
    /// Pure source borrow carries no environment owner or native transport receipt.
    /// 纯来源借用不携带环境所有者或原生传输回执。
    profile: &'a VulcanPluginPublishedMcpProfile,
}

impl VulcanPluginPublishedMcpStartGuard<'_> {
    /// Clone the original opaque profile while the original source exclusion remains borrowed.
    /// 原来源互斥仍被借用时，克隆原不透明画像。
    /// No inputs; the result proves source only, and must not be treated as an actual-launch receipt.
    /// 无输入；返回值仅证明来源，不得视为实际启动回执。
    pub fn host_profile(&self) -> VulcanPluginPublishedMcpProfile {
        self.profile.clone()
    }
}

impl VulcanPluginPreparedPublishedMcp {
    /// Authorize authority against the same original issuer before borrowing materialization inputs.
    /// 借用物化输入前，对照同一原签发者授权 authority。
    /// Return success for the same actual registration; no manager is inferred from IDs.
    /// 同一实际注册返回成功；不从 ID 推断管理器。
    fn validate_host(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.inner.profile.validate_host(authority)
    }

    /// Borrow the original plugin identity and route generation for the same registered authority.
    /// 为同一已注册 authority 借用原插件身份及路由代次。
    /// Return non-secret metadata even when this original owner declares no global servers.
    /// 即使此原所有者未声明全局服务器，也返回非秘密元数据。
    /// This metadata grants no native admission; actual dispatch still requires begin_dispatch.
    /// 此元数据不授予原生入场；实际派发仍要求 begin_dispatch。
    pub fn identity(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<(&str, u64), VulcanPluginRuntimeError> {
        self.validate_host(authority)?;
        Ok((&self.inner.plugin.id, self.inner.profile.inner.generation))
    }

    /// Borrow frozen server declarations for authority, the same trusted original registered host.
    /// 为 authority 即同一受信任原注册宿主借用固定服务器声明。
    /// Return descriptor-owned inputs; their clones do not replace this full resource owner's package retention.
    /// 返回描述符自有输入；其克隆不能替代此完整资源所有者的插件包保留。
    pub fn servers(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<&[VulcanPluginMcpServerDescriptor], VulcanPluginRuntimeError> {
        self.validate_host(authority)?;
        Ok(&self.inner.plugin.mcp_servers)
    }

    /// Borrow server_id's exact frozen stdio launcher for the same authority without querying current ENV or catalog.
    /// 为同一 authority 借用 server_id 的精确固定标准输入输出启动器，不查询当前 ENV 或目录。
    /// Return the original launcher; remote or undeclared servers fail explicitly.
    /// 返回原启动器；远端或未声明服务器明确失败。
    pub fn stdio_launcher(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        server_id: &str,
    ) -> Result<&VulcanPluginMcpStdioLaunchSpec, VulcanPluginRuntimeError> {
        self.validate_host(authority)?;
        self.inner
            .profile
            .inner
            .stdio_launchers
            .get(server_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Published MCP stdio declaration is missing.")
            })
    }

    /// Clone source-only values for authority without retaining this resource bundle in historical controls.
    /// 为 authority 克隆仅来源值，不在历史控制中保留此资源包。
    /// Return the same opaque profile; no startup or physical-retirement evidence is created.
    /// 返回同一不透明画像；不创建启动或物理退役证据。
    pub fn host_profile(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginPublishedMcpProfile, VulcanPluginRuntimeError> {
        self.validate_host(authority)?;
        Ok(self.inner.profile.clone())
    }

    /// Retain original manager and store fences before synchronous native dispatch using manager and authority.
    /// 使用 manager 及 authority，在同步原生派发前保留原管理器及存储屏障。
    /// Return borrowed admission only for the same full current source; changes fail before task spawn.
    /// 仅同一完整当前来源返回借用入场；变更在任务启动前失败。
    pub fn begin_dispatch<'a>(
        &'a self,
        manager: &'a VulcanPluginRuntimeManager,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginPublishedMcpStartGuard<'a>, VulcanPluginRuntimeError> {
        self.validate_host(authority)?;
        // Upgrade only the actual original snapshot, never substitute a newly read catalog.
        // 仅升级实际原快照，绝不以新读取目录替代。
        let expected = self.inner.profile.inner.expected.upgrade().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original published MCP catalog was retired.")
        })?;
        self.inner
            .profile
            .begin_publication(manager, authority, &expected)
    }
}

/// Validate expected's actual manager, current Arc and plugin_id's enabled full descriptor.
/// 校验 expected 的实际管理器、当前 Arc 及 plugin_id 的已启用完整描述符。
/// Return the original descriptor clone; copied, foreign, legacy and non-enabled snapshots are rejected.
/// 返回原描述符克隆；拒绝复制、外来、旧格式及未启用快照。
fn validate_current_snapshot(
    manager: &VulcanPluginRuntimeManager,
    expected: &Arc<VulcanPluginCatalogSnapshot>,
    plugin_id: &str,
) -> Result<VulcanPluginDescriptor, VulcanPluginRuntimeError> {
    // Validate actual publication identity before borrowing any descriptor from expected.
    // 从 expected 借用任何描述符前校验实际发布身份。
    let current =
        manager.state.snapshot.read().map_err(|_| {
            VulcanPluginRuntimeError::new("Published MCP catalog lock is poisoned.")
        })?;
    if !Arc::ptr_eq(&current, expected) || expected.service_epoch != manager.state.manager_id {
        return Err(VulcanPluginRuntimeError::new(
            "Published MCP requires its actual current catalog.",
        ));
    }
    // The exact plugin entry belongs to this same original published snapshot.
    // 精确插件条目属于此同一原已发布快照。
    let plugin = expected
        .plugins
        .get(plugin_id)
        .ok_or_else(|| VulcanPluginRuntimeError::new("Published MCP plugin is missing."))?;
    if plugin.generation == 0
        || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
        || !plugin.desired_enabled
        || plugin.origin != VulcanPluginPackageOrigin::UserInstalled
        || plugin.managed_package().is_none()
    {
        return Err(VulcanPluginRuntimeError::new(
            "Published MCP requires an enabled managed selection.",
        ));
    }
    Ok(plugin.clone())
}

/// Apply the runtime's original sole admission predicates to this fence's actual locked facts.
/// 将运行时原唯一入场谓词应用至此屏障的实际加锁事实。
/// Return unchanged handoff or selected-target conflicts; other Prepared requests leave old business admitted.
/// 返回未改变的交接或已选目标冲突；其他 Prepared 请求使旧业务继续入场。
fn validate_dispatch_admission(
    fence: &PublishedDispatchFence<'_>,
) -> Result<(), VulcanPluginRuntimeError> {
    validate_ordinary_handoff_admission(fence.handoff_blocked())?;
    validate_ordinary_pending_admission(
        VulcanPluginBridgeLeaseScope::Enabled,
        Some(fence.selected()),
        fence.pending(),
    )
}

/// Compare plugin's real full selection and physical pins with active under the same retained Store fence.
/// 在同一保留存储屏障内，比较 plugin 的真实完整选择及物理固定与 active。
/// Return success only for explicit complete immutable authorities; legacy values never become source proof.
/// 仅显式完整不可变权威返回成功；旧格式值绝不变为来源证明。
fn validate_full_pins(
    plugin: &VulcanPluginDescriptor,
    selected: &VulcanPluginActivation,
    active: &VulcanPluginActiveGeneration,
) -> Result<(), VulcanPluginRuntimeError> {
    // Original descriptor retention is mandatory; copied revision values cannot replace its pins.
    // 原描述符保留是必需条件；复制修订值不能替代其固定句柄。
    let managed = plugin.managed_package().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Published MCP package retention is missing.")
    })?;
    // The real pinned package was acquired while the actual selection namespace remained held.
    // 实际选择命名空间保持持有时取得真实固定插件包。
    let package = active
        .generation
        .as_ref()
        .ok_or_else(|| VulcanPluginRuntimeError::new("Published MCP active package is missing."))?;
    // Real revision retention must bind the same selected physical package.
    // 真实修订保留必须绑定同一已选物理插件包。
    let credentials = active.credentials.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Published MCP credential retention is missing.")
    })?;
    if !managed.matches_activation(selected)
        || managed.generation().identity() != package.identity()
        || plugin.package_root != package.package_root()
        || plugin.version.as_deref() != Some(package.identity().version.as_str())
        || plugin.content_fingerprint != package.identity().content_fingerprint
        || credentials.record().package() != package.identity()
        || Some(credentials.record().revision()) != selected.credential_revision()
    {
        return Err(VulcanPluginRuntimeError::new(
            "Published MCP full retained selection differs from its active source.",
        ));
    }
    // Each contribution must retain the same actual revision and frozen configuration as its full owner.
    // 每个贡献必须保留与其完整所有者相同的实际修订及固定配置。
    for server in &plugin.mcp_servers {
        if server.plugin_id != plugin.id
            || server.plugin_generation != plugin.generation
            || server.configuration() != Some(selected.configuration())
            || server.credential_binding != *managed.credential_binding()
            || !server.credential_lease.as_ref().is_some_and(|lease| {
                lease.record().package() == package.identity()
                    && Some(lease.record().revision()) == selected.credential_revision()
            })
        {
            return Err(VulcanPluginRuntimeError::new(
                "Published MCP declaration differs from its retained selection.",
            ));
        }
    }
    Ok(())
}

impl VulcanPluginRuntimeManager {
    /// Prepare plugin_id from expected's actual complete published selection for the registered authority.
    /// 为已注册 authority，从 expected 实际完整已发布选择准备 plugin_id。
    /// Return request-free real resources and frozen launchers; no native task, target effect or restoration starts.
    /// 返回无请求真实资源及固定启动器；不启动原生任务、目标副作用或恢复。
    pub fn prepare_published_mcp_resources(
        &self,
        expected: &Arc<VulcanPluginCatalogSnapshot>,
        plugin_id: &str,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginPreparedPublishedMcp, VulcanPluginRuntimeError> {
        if !authority.matches_manager(self) {
            return Err(VulcanPluginRuntimeError::new(
                "Published MCP requires this manager's registered native host.",
            ));
        }
        // The actual current immutable Arc is the sole descriptor source.
        // 实际当前不可变 Arc 是唯一描述符来源。
        let plugin = validate_current_snapshot(self, expected, plugin_id)?;
        // Open this manager's existing canonical store once and retain it for final dispatch.
        // 仅打开一次此管理器现有规范存储，并为最终派发保留它。
        let store = VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Published MCP store is missing."))?;
        // Read full persistent selection; absence cannot become a guessed managed profile.
        // 读取完整持久选择；缺失不能成为猜测的受管画像。
        let selected = store
            .activation(plugin_id)
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Published MCP activation is missing."))?;
        {
            // Selection and all actual pins are checked while the same namespace excludes independent writers.
            // 选择及全部实际固定在同一命名空间排除独立写入者期间检查。
            let fence = store
                .acquire_published_dispatch_fence(&selected)
                .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?;
            validate_dispatch_admission(&fence)?;
            // Acquire actual package and credential Files through the already-held namespace helper.
            // 经已持有命名空间辅助方法取得实际插件包及凭据 File。
            let active = fence
                .pin_selected()
                .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?;
            validate_full_pins(&plugin, fence.selected(), &active)?;
        }
        // Do not hold a short namespace through filesystem preparation; final dispatch rechecks the full source.
        // 不在文件系统准备期间持有短命名空间；最终派发重新检查完整来源。
        // The actual environment owner precedes every local launcher preparation and survives its execution.
        // 实际环境所有者先于每个本地启动器准备，并跨其执行存续。
        let environment = if plugin.mcp_servers.iter().any(|server| {
            matches!(
                server.manifest.transport,
                VulcanPluginMcpTransportManifest::Stdio { .. }
            )
        }) {
            Some(
                super::super::super::environment::retain_plugin_execution_environment(
                    &self.state,
                    &plugin.id,
                )
                .map_err(VulcanPluginRuntimeError::new)?,
            )
        } else {
            None
        };
        // Launcher declarations come only from the full retained selected package.
        // 启动器声明仅来自完整保留的已选插件包。
        let manifest = plugin
            .manifest
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Published MCP manifest is missing."))?;
        // Resolve each declared local launcher once without reselecting from public discovery.
        // 仅解析一次每个已声明本地启动器，不从公开发现重选。
        let mut stdio_launchers = BTreeMap::new();
        for server in &plugin.mcp_servers {
            if let VulcanPluginMcpTransportManifest::Stdio { launcher, .. } =
                &server.manifest.transport
            {
                // The same original package and manifest feed the production filesystem resolver.
                // 同一原插件包及清单传入生产文件系统解析器。
                let resolved = super::super::super::process::resolve_plugin_mcp_stdio_launcher(
                    &self.state,
                    &plugin,
                    manifest,
                    launcher,
                )
                .map_err(VulcanPluginRuntimeError::new)?;
                stdio_launchers.insert(server.id.clone(), resolved);
            }
        }
        // Copy pure declaration values, never descriptor-owned File pins, into historical source identity.
        // 仅复制纯声明值至历史来源身份，绝不复制描述符自有 File 固定。
        let profile = VulcanPluginPublishedMcpProfile {
            inner: Arc::new(PublishedMcpProfileValues {
                authority: authority.clone(),
                store,
                expected: Arc::downgrade(expected),
                selected,
                generation: plugin.generation,
                _declarations: plugin
                    .mcp_servers
                    .iter()
                    .map(|server| server.manifest.clone())
                    .collect(),
                stdio_launchers,
            }),
        };
        Ok(VulcanPluginPreparedPublishedMcp {
            inner: Arc::new(PublishedMcpResources {
                plugin,
                profile,
                _environment: environment,
            }),
        })
    }
}

#[cfg(test)]
mod tests;
