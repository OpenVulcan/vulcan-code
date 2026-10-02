//! Full publication keeps original source, actual input and same-lease business guards through one pointer write.
//! 完整发布使原来源、实际输入及同租约业务守卫跨单次指针写入保持持有。

use super::*;
use crate::mcp::registry::{McpActualLaunchInputLease, McpPluginLaunchSource};
use std::sync::{MutexGuard, RwLockReadGuard, RwLockWriteGuard};
use vulcan_plugin_runtime::{VulcanPluginCatalogSnapshot, VulcanPluginPublishedMcpStartGuard};

/// Borrowed synchronous publication exclusion cannot outlive the original control, manager or catalog.
/// 借用的同步发布互斥不能跨原控制、管理器或目录存续。
pub struct PublishedPluginMcpPublicationGuard<'a> {
    /// Original source exclusion is acquired first and never substitutes for actual native proof.
    /// 原来源互斥最先取得，且绝不替代实际原生证明。
    _source: VulcanPluginPublishedMcpStartGuard<'a>,
    /// Original control supplies identity and stores the adjacent actual commit observation.
    /// 原控制提供身份，并保存相邻实际提交观察。
    owner: &'a PublishedPluginMcpRuntime,
    /// Exact catalog remains fixed only across this short full-pointer publication.
    /// 精确目录仅跨此短时完整指针发布固定。
    catalog: Arc<VulcanPluginCatalogSnapshot>,
    /// Same actual lease remains owned until the full successor takes it.
    /// 同一实际租约保持拥有，直至完整后继接纳它。
    lease: Arc<McpRuntimePartitionLease>,
    /// Admission orders this original against plugin drain and global shutdown.
    /// 入场将此原实例与插件排空及全局关闭排序。
    _admission: MutexGuard<'a, OwnerMcpAdmission>,
    /// Same pending original cannot disappear before adjacent confirmed transfer.
    /// 相同待处理原实例不能在相邻已确认转交之前消失。
    pending: MutexGuard<'a, BTreeMap<String, Vec<McpRuntimeHandle>>>,
    /// Sole ownership state transfers from strong preparation to weak historical observation.
    /// 唯一所有权状态从强准备转交为弱历史观察。
    state: MutexGuard<'a, PublishedMcpLeaseState>,
    /// Actual business write exclusion remains closed until the full pointer commits.
    /// 实际业务写互斥保持关闭，直至完整指针提交。
    business: RwLockWriteGuard<'a, McpBusinessGate>,
    /// Original transports cannot be removed while native source checks and commit execute.
    /// 原生来源校验及提交执行期间，原传输不能被移除。
    _transports: RwLockReadGuard<'a, BTreeMap<String, Arc<dyn McpTransport>>>,
    /// Same protected input records stay paired with their actual transport and release evidence.
    /// 相同受保护输入记录与实际传输及释放证据保持配对。
    _inputs: RwLockReadGuard<'a, BTreeMap<String, McpActualLaunchInputLease>>,
    /// Exact real connected snapshot cannot change across this publication boundary.
    /// 精确真实已连接快照不能跨此发布边界变化。
    _snapshot: RwLockReadGuard<'a, McpRuntimeSnapshot>,
}

/// One sealed full successor consumes its original native guard, preventing independently paired confirmations.
/// 单个密封完整后继消费其原原生守卫，防止独立配对确认。
pub struct PreparedPublishedPluginMcpPublication<'a> {
    /// All original native checks and exclusions remain held through the actual pointer replacement.
    /// 全部原原生校验及互斥跨实际指针替换保持持有。
    guard: PublishedPluginMcpPublicationGuard<'a>,
    /// Actual captured original pointer supplies conflict detection and displaced resource lifetime.
    /// 实际捕获的原指针提供冲突检测及被替换资源生命周期。
    expected: Arc<ExtensionGeneration>,
    /// Complete successor contains the same actual catalog, frozen handlers and original execution lease.
    /// 完整后继包含同一实际目录、冻结处理器及原执行租约。
    next: Arc<ExtensionGeneration>,
}

impl PublishedPluginMcpRuntime {
    /// Derive a candidate access before acquiring publication guards, retaining the original exact lease.
    /// 取得发布守卫之前派生候选访问，保留原精确租约。
    /// current is the captured service access; returns all unrelated current partitions plus this closed original.
    /// current 为捕获的服务访问；返回全部当前无关分区及此关闭原实例。
    pub fn publication_access(
        &self,
        current: &McpRuntimeAccess,
    ) -> Result<McpRuntimeAccess, String> {
        if !self.access.same_lineage(current) {
            return Err(String::from(
                "Published MCP publication belongs to another access lineage.",
            ));
        }
        // Drop ownership exclusion before the constructor acquires original pending observation storage.
        // 构造器取得原待处理观察存储之前，释放所有权互斥。
        let lease = {
            // No registry or business locks are held during this short preparation ownership read.
            // 此短时准备所有权读取期间，不持有注册表或业务锁。
            let state = self
                .lease
                .lock()
                .map_err(|_| String::from("Published MCP ownership is poisoned."))?;
            match &*state {
                PublishedMcpLeaseState::Preparing(lease) => Arc::clone(lease),
                PublishedMcpLeaseState::Published { .. } => {
                    return Err(String::from(
                        "Published MCP ownership has already transferred.",
                    ));
                }
            }
        };
        Ok(current.with_prepared_plugin_lease(
            &self.observation.plugin_id,
            self.observation.generation,
            lease,
        ))
    }

    /// Validate original source, registered control, actual connected inputs and closed business before full publication.
    /// 完整发布之前，校验原来源、已登记控制、实际已连接输入及关闭业务。
    /// manager, authority and catalog are the same original producers; return a synchronous borrowed guard or explicit rejection.
    /// manager、authority 及 catalog 为相同原生产者；返回同步借用守卫或明确拒绝。
    /// Never await or invoke plugin business while the returned guard exists.
    /// 返回守卫存续期间，绝不等待或调用插件业务。
    pub fn prepare_publication<'a>(
        &'a self,
        manager: &'a VulcanPluginRuntimeManager,
        authority: &'a VulcanPluginRegisteredNativeMcpHostAuthority,
        catalog: &'a Arc<VulcanPluginCatalogSnapshot>,
    ) -> Result<PublishedPluginMcpPublicationGuard<'a>, String> {
        // Original manager registration and cross-store source File precede native admission in the sole lock order.
        // 原管理器登记及跨存储来源 File 按唯一锁序先于原生入场。
        let source = self
            .observation
            .profile
            .begin_publication(manager, authority, catalog)
            .map_err(|error| error.to_string())?;
        // Original lifecycle admission prevents drain or shutdown from starting across full confirmation.
        // 原生命周期入场防止排空或关闭跨完整确认开始。
        let admission = self
            .access
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("Published MCP admission is poisoned."))?;
        if admission.contains(&self.observation.plugin_id)
            || !admission
                .managed_globals
                .get(&self.observation.plugin_id)
                .is_some_and(|originals| {
                    originals
                        .iter()
                        .any(|original| original.same_instance(&self.observation.runtime))
                })
        {
            return Err(String::from(
                "Published MCP original startup admission changed.",
            ));
        }
        // The exact original registration remains recoverable until the same actual lease transfers.
        // 精确原登记保持可恢复，直至同一实际租约转交。
        let pending = self
            .access
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("Published MCP original observations are poisoned."))?;
        if !pending
            .get(&self.observation.plugin_id)
            .is_some_and(|originals| {
                originals
                    .iter()
                    .any(|original| original.same_instance(&self.observation.runtime))
            })
        {
            return Err(String::from(
                "Published MCP original pending observation changed.",
            ));
        }
        // Ownership exclusion follows registries; a second publication cannot create another confirmation.
        // 所有权互斥位于注册表之后；第二次发布不能创建另一确认。
        let state = self
            .lease
            .lock()
            .map_err(|_| String::from("Published MCP ownership is poisoned."))?;
        // This temporary strong lease protects transfer; every lock below borrows the control's original inner allocation.
        // 此临时强租约保护转交；下方每个锁均借用控制的原内部对象。
        let lease = match &*state {
            PublishedMcpLeaseState::Preparing(lease)
                if lease.runtime.same_instance(&self.observation.runtime) =>
            {
                Arc::clone(lease)
            }
            _ => {
                return Err(String::from(
                    "Published MCP ownership is not the original preparation.",
                ));
            }
        };
        // Lock actual maps in their existing removal order without reentering the business gate through capture helpers.
        // 按既有移除顺序锁定实际映射，不通过捕获帮助方法重入业务门。
        let business = self
            .observation
            .runtime
            .inner
            .business_gate
            .write()
            .map_err(|_| String::from("Published MCP business gate is poisoned."))?;
        // Original actual transport allocation supplies native identity rather than a route or observed PID.
        // 原实际传输对象提供原生身份，而非路由或观察到的进程标识。
        let transports = self
            .observation
            .runtime
            .inner
            .transports
            .read()
            .map_err(|_| String::from("Published MCP actual transports are poisoned."))?;
        // Input source was bound before launch and belongs to this same actual transport resource bundle.
        // 输入来源在启动前绑定，属于此同一实际传输资源包。
        let inputs = self
            .observation
            .runtime
            .inner
            .launch_inputs
            .read()
            .map_err(|_| String::from("Published MCP actual inputs are poisoned."))?;
        // The real protocol readiness snapshot stays fixed through source checking and pointer replacement.
        // 真实协议就绪快照跨来源校验及指针替换保持固定。
        let snapshot = self
            .observation
            .runtime
            .inner
            .snapshot
            .read()
            .map_err(|_| String::from("Published MCP readiness snapshot is poisoned."))?;
        if *business != McpBusinessGate::Preparing
            || lease.is_revoked()
            || self.observation.runtime.shutdown_requested()
            || !matches!(&self.observation.runtime.inner.launch_source,
                McpPluginLaunchSource::Published(profile) if profile.same_source(&self.observation.profile))
        {
            return Err(String::from(
                "Published MCP original preparation is closed or unproven.",
            ));
        }
        for server in self.observation.runtime.inner.registry.enabled_servers() {
            // Each actual enabled declaration must have real Connected state and its same original input record.
            // 每个实际已启用声明均须具有真实 Connected 状态及同一原输入记录。
            let transport = transports
                .get(&server.id)
                .ok_or_else(|| String::from("Published MCP original transport is unavailable."))?;
            if !snapshot
                .servers
                .get(&server.id)
                .is_some_and(|actual| actual.status == McpRuntimeServerStatus::Connected)
            {
                return Err(String::from(
                    "Published MCP original transport is not connected.",
                ));
            }
            // Use the one exact transport, release, ambient-input and original source predicate already used by capture.
            // 使用捕获既有的唯一精确传输、释放、环境输入及原来源判定。
            let input = inputs.get(&server.id).ok_or_else(|| {
                String::from("Published MCP original launch inputs are unavailable.")
            })?;
            input
                .with_published_source(transport, &self.observation.profile, |_, _| ())
                .map_err(|error| {
                    format!("Published MCP original launch is ineligible: {error:?}.")
                })?;
        }
        Ok(PublishedPluginMcpPublicationGuard {
            _source: source,
            owner: self,
            catalog: Arc::clone(catalog),
            lease,
            _admission: admission,
            pending,
            state,
            business,
            _transports: transports,
            _inputs: inputs,
            _snapshot: snapshot,
        })
    }
}

impl<'a> PublishedPluginMcpPublicationGuard<'a> {
    /// Bind one complete prepared successor to this already validated actual native guard.
    /// 将单个完整已准备后继绑定至此已校验实际原生守卫。
    /// expected is the captured service pointer and next includes exact hooks, catalog and original lease.
    /// expected 为捕获的服务指针，next 包含精确 Hook、目录及原租约。
    /// Returns a sealed single-use publication, rejecting foreign catalogs or access lineages before pointer replacement.
    /// 返回密封单次发布，在指针替换前拒绝外来目录或访问谱系。
    pub fn prepare_extension(
        self,
        expected: &Arc<ExtensionGeneration>,
        next: Arc<ExtensionGeneration>,
    ) -> Result<PreparedPublishedPluginMcpPublication<'a>, String> {
        if next.plugin_conversation_hooks().is_none()
            || !next
                .plugin_catalog()
                .is_some_and(|actual| Arc::ptr_eq(&actual, &self.catalog))
        {
            return Err(String::from(
                "Published MCP successor lacks the exact full plugin handlers and catalog.",
            ));
        }
        // Both original and successor must belong to the entire original native lineage, not a copied admission field.
        // 原值及后继均须属于全部原原生谱系，不接受复制的入场字段。
        let original_access = expected.mcp_runtime();
        // Exact successor access is prepared before guard acquisition to avoid observation lock reentry.
        // 精确后继访问在取得守卫之前准备，以避免观察锁重入。
        let next_access = next.mcp_runtime();
        if !self.owner.access.same_lineage(&original_access)
            || !original_access.same_lineage(&next_access)
            || !next_access.contains_exact_plugin_lease(
                &self.owner.observation.plugin_id,
                self.owner.observation.generation,
                &Arc::downgrade(&self.lease),
            )
        {
            return Err(String::from(
                "Published MCP successor does not contain the original actual lease and lineage.",
            ));
        }
        Ok(PreparedPublishedPluginMcpPublication {
            guard: self,
            expected: Arc::clone(expected),
            next,
        })
    }
}

impl PreparedPublishedPluginMcpPublication<'_> {
    /// Replace the host's actual slot using the sole full-pointer primitive and adjacent same-lease confirmation.
    /// 使用唯一完整指针原语及相邻同租约确认，替换宿主实际槽。
    /// slot must be held under its host publication lock; returns the actual committed Arc or a pre-write conflict.
    /// slot 须在其宿主发布锁内持有；返回实际已提交 Arc 或写入前冲突。
    /// Native guards are released on return; old-partition retirement must occur afterwards.
    /// 返回时释放原生守卫；旧分区退休须随后执行。
    pub fn publish_into(
        self,
        slot: &mut Arc<ExtensionGeneration>,
    ) -> Result<Arc<ExtensionGeneration>, String> {
        // Consuming the sealed value leaves exactly one original guard eligible for this actual replacement.
        // 消费密封值使恰好一个原守卫可参与此实际替换。
        let Self {
            mut guard,
            expected,
            next,
        } = self;
        ExtensionGeneration::publish_prepared(slot, &expected, next, |committed| {
            // Record weak identities before opening business so historical controls never retain execution resources.
            // 开放业务之前记录弱身份，使历史控制绝不保留执行资源。
            *guard.state = PublishedMcpLeaseState::Published {
                lease: Arc::downgrade(&guard.lease),
                extension: Arc::downgrade(committed),
            };
            guard
                .pending
                .get_mut(&guard.owner.observation.plugin_id)
                .expect("validated original published MCP observation")
                .retain(|original| !original.same_instance(&guard.owner.observation.runtime));
            *guard.business = McpBusinessGate::Open;
        })
    }
}
