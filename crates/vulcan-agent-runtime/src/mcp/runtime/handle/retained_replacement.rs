//! Service-retained plugin MCP replacement owns undispatched work and its sole actual execution lease.
//! 服务保留的插件 MCP 替换持有未派发工作及其唯一实际执行租约。

use super::*;
use crate::agent_runtime::extension_generation::ExtensionGeneration;
use std::sync::Weak;

#[cfg(test)]
mod tests;

/// Actual ownership progresses once; public observations never reconstruct an execution lease.
/// 实际所有权仅推进一次；公共观测绝不重建执行租约。
enum RetainedReplacementOwnership {
    /// Both original work and its allocated lease exist before the service registers this controller.
    /// 原工作及其已分配租约均在服务登记此控制器之前存在。
    Prepared {
        /// Native tasks cannot exist until this exact work is consumed.
        /// 此精确工作被消费之前不可能存在原生任务。
        work: PreparedMcpRuntime,
        /// The same lease transfers to the full successor without a second allocation.
        /// 相同租约转交完整后继，不进行第二次分配。
        lease: Arc<McpRuntimePartitionLease>,
    },
    /// The service retains the original execution lease throughout readiness and publication preparation.
    /// 服务跨就绪及发布准备保留原执行租约。
    Running(Arc<McpRuntimePartitionLease>),
    /// Historical control retains only weak identities after actual full-pointer confirmation.
    /// 实际完整指针确认后，历史控制仅保留弱身份。
    Published {
        /// Weak ownership cannot keep the published process alive.
        /// 弱所有权不能使已发布进程继续存活。
        _lease: Weak<McpRuntimePartitionLease>,
        /// Original full commit identity contains no execution resources.
        /// 原完整提交身份不包含执行资源。
        _extension: Weak<ExtensionGeneration>,
    },
    /// Typed cancellation proves this original was never dispatched; physical release is still checked.
    /// 类型化取消证明此原实例绝未派发；仍检查物理释放。
    CancelledBeforeDispatch,
    /// Actual shutdown, cleanup and aggregate physical release have all been confirmed.
    /// 实际关闭、清理及聚合物理释放均已确认。
    Retired,
}

/// Single-shot raw replacement grants no sealed source, target-completion or predecessor-restoration authority.
/// 单次原始替换不授予密封来源、目标完成或前驱恢复权威。
pub struct RetainedPluginMcpReplacement {
    /// Exact declared plugin identity fixes routing and original observation registration.
    /// 精确已声明插件身份固定路由及原观察登记。
    plugin_id: String,
    /// Exact declared generation is never substituted from a later catalog.
    /// 精确已声明代次绝不由随后目录替换。
    generation: u64,
    /// Original non-owning control observes only this native instance.
    /// 原非拥有控制仅观察此原生实例。
    runtime: McpRuntimeHandle,
    /// Shared observation lineage holds no unrelated execution partition.
    /// 共享观察谱系不持有无关执行分区。
    access: McpRuntimeObservationAccess,
    /// The service borrows this typed owner; cancellation does not move its actual lease out.
    /// 服务借用此类型化所有者；取消不将其实际租约移出。
    ownership: Mutex<RetainedReplacementOwnership>,
}

impl RetainedPluginMcpReplacement {
    /// Purely prepare exact nonempty raw definitions against current's original lineage before service registration.
    /// 在服务登记前，针对 current 原谱系纯准备精确非空原始定义。
    /// Returns undispatched ownership; plugin and generation must match every server's declared source.
    /// 返回未派发所有权；插件及代次必须匹配每个服务器的已声明来源。
    pub fn prepare(
        current: &McpRuntimeAccess,
        plugin_id: &str,
        generation: u64,
        servers: Vec<McpServerConfig>,
    ) -> Result<Self, String> {
        if servers.is_empty()
            || servers.iter().any(|server| {
                server.source
                    != (crate::mcp::McpServerSource::Plugin {
                        plugin_id: plugin_id.to_owned(),
                        generation,
                    })
            })
        {
            return Err(String::from(
                "Retained replacement requires nonempty exact plugin definitions.",
            ));
        }
        // No caller-visible resource handle or parallel source receipt is created.
        // 不创建调用者可见资源句柄或并行来源回执。
        let (resources, _) = vulcan_mcp_runtime::McpTransportResources::new(());
        // Explicit raw preparation runs once and keeps business closed until full publication.
        // 明确原始准备仅运行一次，并使业务保持关闭直至完整发布。
        let work = prepare_mcp_runtime(
            servers,
            resources,
            McpRuntimePurpose::RetainedPluginReplacement,
            crate::mcp::registry::McpPluginLaunchSource::Unproven,
        )?;
        // This original allocation is used for registration, tasks and full publication.
        // 此原分配用于登记、任务及完整发布。
        let runtime = work.handle();
        // Allocate the unique execution identity before any native effect can exist.
        // 在任何原生副作用可能存在之前，分配唯一执行身份。
        let lease = Arc::new(McpRuntimePartitionLease::new(runtime.clone()));
        Ok(Self {
            plugin_id: plugin_id.to_owned(),
            generation,
            runtime,
            access: current.observation_access(),
            ownership: Mutex::new(RetainedReplacementOwnership::Prepared { work, lease }),
        })
    }

    /// Register and dispatch this same service-retained original exactly once.
    /// 恰好一次登记及派发此相同服务保留原实例。
    /// Returns explicit refusal before dispatch when admission or original ownership changed.
    /// 准入或原所有权变化时，在派发前返回明确拒绝。
    pub fn dispatch(&self) -> Result<(), String> {
        tokio::runtime::Handle::try_current().map_err(|_| {
            String::from("Retained replacement dispatch requires an active Tokio runtime.")
        })?;
        // Keep the same ordering as full publication: admission, pending originals, then ownership.
        // 保持与完整发布相同顺序：准入、待处理原实例、所有权。
        let mut admission = self
            .access
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("Retained replacement admission is poisoned."))?;
        if admission.contains(&self.plugin_id) {
            return Err(String::from("Retained replacement admission is closed."));
        }
        // Register actual controls before their first task can execute.
        // 首个任务可执行之前登记实际控制。
        let mut pending = self
            .access
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("Retained replacement observations are poisoned."))?;
        // The same exclusive state borrow prevents duplicate dispatch or cancellation during transfer.
        // 相同排他状态借用防止移交期间重复派发或取消。
        let mut ownership = self
            .ownership
            .lock()
            .map_err(|_| String::from("Retained replacement ownership is poisoned."))?;
        // Clone only this already allocated lease; no replacement lease is constructed.
        // 仅克隆此已分配租约；不构造替代租约。
        let lease = match &*ownership {
            RetainedReplacementOwnership::Prepared { lease, .. } => Arc::clone(lease),
            _ => {
                return Err(String::from(
                    "Retained replacement has already left preparation.",
                ));
            }
        };
        pending
            .entry(self.plugin_id.clone())
            .or_default()
            .push(self.runtime.clone());
        admission.prune_released();
        admission
            .managed_globals
            .entry(self.plugin_id.clone())
            .or_default()
            .push(self.runtime.clone());
        // Running ownership is established before work moves into actual tasks, without an empty lease interval.
        // 工作移入实际任务之前建立运行中所有权，不存在空租约区间。
        let prepared = std::mem::replace(
            &mut *ownership,
            RetainedReplacementOwnership::Running(lease),
        );
        match prepared {
            RetainedReplacementOwnership::Prepared { work, .. } => {
                work.dispatch();
            }
            _ => unreachable!("exclusive validated retained preparation"),
        }
        Ok(())
    }

    /// Await complete discovery of this original while its service owner retains the actual lease.
    /// 服务所有者保留实际租约时，等待此原实例完整发现。
    /// Cancellation ends only this borrow; success grants no publication or sealed source receipt.
    /// 取消仅结束此借用；成功不授予发布或密封来源回执。
    pub async fn wait_until_ready(&self) -> Result<(), String> {
        self.runtime
            .wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT)
            .await
    }

    /// Merge this original lease with actual current unrelated partitions while keeping business closed.
    /// 将此原租约与实际当前无关分区合并，同时保持业务关闭。
    /// Returns an unpublished access only for the same complete lineage and running original.
    /// 仅对相同完整谱系及运行中原实例返回未发布访问。
    pub fn publication_access(
        &self,
        current: &McpRuntimeAccess,
    ) -> Result<McpRuntimeAccess, String> {
        if !self.access.same_lineage(current) {
            return Err(String::from(
                "Retained replacement publication lineage changed.",
            ));
        }
        // Release ownership before the access factory obtains original observation locks.
        // 在访问工厂取得原观察锁之前，释放所有权。
        let lease = match &*self
            .ownership
            .lock()
            .map_err(|_| String::from("Retained replacement ownership is poisoned."))?
        {
            RetainedReplacementOwnership::Running(lease) => Arc::clone(lease),
            _ => {
                return Err(String::from(
                    "Retained replacement is not an unpublished running original.",
                ));
            }
        };
        Ok(current.with_prepared_plugin_lease(&self.plugin_id, self.generation, lease))
    }

    /// Publish a complete actual successor using original readiness and adjacent same-lease transfer.
    /// 使用原就绪及相邻同租约移交发布完整实际后继。
    /// The host holds slot's publication lock; expected and next must contain the exact original lineage and lease.
    /// 宿主持有 slot 发布锁；expected 及 next 必须包含精确原谱系及租约。
    /// confirm is infallible synchronous service bookkeeping; errors precede the sole full-pointer write.
    /// confirm 为不可失败同步服务记账；错误先于唯一完整指针写入。
    pub fn publish_prepared(
        &self,
        slot: &mut Arc<ExtensionGeneration>,
        expected: &Arc<ExtensionGeneration>,
        next: Arc<ExtensionGeneration>,
        confirm: impl FnOnce(),
    ) -> Result<(), String> {
        // Admission closure and original registration remain fixed through actual full-pointer replacement.
        // 实际完整指针替换期间固定准入关闭及原登记。
        let admission = self
            .access
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("Retained replacement admission is poisoned."))?;
        if admission.contains(&self.plugin_id)
            || !admission
                .managed_globals
                .get(&self.plugin_id)
                .is_some_and(|originals| {
                    originals
                        .iter()
                        .any(|original| original.same_instance(&self.runtime))
                })
        {
            return Err(String::from(
                "Retained replacement original admission changed.",
            ));
        }
        // This same pending control cannot be substituted before confirmation.
        // 确认之前不能替换此相同待处理控制。
        let mut pending = self
            .access
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("Retained replacement observations are poisoned."))?;
        if !pending.get(&self.plugin_id).is_some_and(|originals| {
            originals
                .iter()
                .any(|original| original.same_instance(&self.runtime))
        }) {
            return Err(String::from(
                "Retained replacement original registration is missing.",
            ));
        }
        // Exclude cancellation and another publication while transferring the original actual lease.
        // 转交原实际租约时排除取消及另一发布。
        let mut ownership = self
            .ownership
            .lock()
            .map_err(|_| String::from("Retained replacement ownership is poisoned."))?;
        // Declare the temporary strong lease before native guards so it drops only after their release.
        // 在原生守卫之前声明临时强租约，使其仅在这些守卫释放后销毁。
        let lease = match &*ownership {
            RetainedReplacementOwnership::Running(lease) => Arc::clone(lease),
            _ => {
                return Err(String::from(
                    "Retained replacement ownership has already transferred.",
                ));
            }
        };
        if !self.access.same_lineage(&expected.mcp_runtime())
            || !self.access.same_lineage(&next.mcp_runtime())
            || !next.mcp_runtime().contains_exact_plugin_lease(
                &self.plugin_id,
                self.generation,
                &Arc::downgrade(&lease),
            )
            || !next.plugin_catalog().is_some_and(|catalog| {
                catalog.plugins.get(&self.plugin_id).is_some_and(|plugin| {
                    plugin.generation == self.generation
                        && plugin.runtime_state
                            == vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled
                })
            })
            || next.plugin_conversation_hooks().is_none()
        {
            return Err(String::from(
                "Retained replacement full successor lacks original lease, lineage or handlers.",
            ));
        }
        // Hold actual business, transport and readiness guards in the existing removal order.
        // 按既有移除顺序持有实际业务、传输及就绪守卫。
        let mut business = self
            .runtime
            .inner
            .business_gate
            .write()
            .map_err(|_| String::from("Retained replacement business gate is poisoned."))?;
        // Original transport ownership cannot disappear during full-pointer confirmation.
        // 完整指针确认期间原传输所有权不能消失。
        let transports = self
            .runtime
            .inner
            .transports
            .read()
            .map_err(|_| String::from("Retained replacement transports are poisoned."))?;
        // Original readiness remains locked across the adjacent confirmation tail.
        // 跨相邻确认尾段锁定原就绪。
        let snapshot = self
            .runtime
            .inner
            .snapshot
            .read()
            .map_err(|_| String::from("Retained replacement readiness is poisoned."))?;
        if *business != McpBusinessGate::Preparing
            || self.runtime.shutdown_requested()
            || lease.is_revoked()
            || self
                .runtime
                .inner
                .registry
                .enabled_servers()
                .iter()
                .any(|server| {
                    !transports.contains_key(&server.id)
                        || !snapshot
                            .servers
                            .get(&server.id)
                            .is_some_and(|entry| entry.status == McpRuntimeServerStatus::Connected)
                })
        {
            return Err(String::from(
                "Retained replacement original is not ready for full publication.",
            ));
        }
        ExtensionGeneration::publish_prepared(slot, expected, next, |committed| {
            // The same actual full pointer records weak ownership before business opens.
            // 业务开放之前，相同实际完整指针记录弱所有权。
            *ownership = RetainedReplacementOwnership::Published {
                _lease: Arc::downgrade(&lease),
                _extension: Arc::downgrade(committed),
            };
            pending
                .get_mut(&self.plugin_id)
                .expect("validated original retained pending registration")
                .retain(|original| !original.same_instance(&self.runtime));
            *business = McpBusinessGate::Open;
            confirm();
        })?;
        Ok(())
    }

    /// Cancel only this unpublished original and prove its actual cleanup and aggregate physical release.
    /// 仅取消此未发布原实例，并证明其实际清理及聚合物理释放。
    /// Returns explicit refusal for published ownership; cancellation or timeout preserves the original for retry.
    /// 已发布所有权返回明确拒绝；取消或超时保留原实例，以供重试。
    pub async fn abort_and_drain(&self) -> Result<(), String> {
        // No synchronous ownership guard crosses an asynchronous cleanup wait.
        // 没有同步所有权守卫跨异步清理等待。
        let never_dispatched = {
            // Exclude full publication before latching shutdown of this exact original.
            // 锁存此精确原实例关闭之前，排除完整发布。
            let mut ownership = self
                .ownership
                .lock()
                .map_err(|_| String::from("Retained replacement ownership is poisoned."))?;
            match &*ownership {
                RetainedReplacementOwnership::Published { .. } => {
                    return Err(String::from(
                        "Published retained replacement cannot be aborted.",
                    ));
                }
                RetainedReplacementOwnership::Retired => return Ok(()),
                RetainedReplacementOwnership::Prepared { .. } => {
                    self.runtime.shutdown();
                    *ownership = RetainedReplacementOwnership::CancelledBeforeDispatch;
                    true
                }
                RetainedReplacementOwnership::CancelledBeforeDispatch => true,
                RetainedReplacementOwnership::Running(_) => {
                    self.runtime.shutdown();
                    false
                }
            }
        };
        if never_dispatched {
            if !self.runtime.inner.partition_release.is_released() {
                return Err(String::from(
                    "Undispatched retained replacement resources remain owned.",
                ));
            }
        } else {
            self.runtime
                .wait_until_drained(tokio::time::Instant::now() + MCP_PARTITION_READINESS_TIMEOUT)
                .await?;
        }
        // Confirm retirement only after the original positive release proof, without creating another runtime.
        // 仅在原正向释放证明后确认退役，不创建另一运行时。
        *self
            .ownership
            .lock()
            .map_err(|_| String::from("Retained replacement ownership is poisoned."))? =
            RetainedReplacementOwnership::Retired;
        Ok(())
    }
}

impl Drop for RetainedPluginMcpReplacement {
    /// Unpublished abandonment closes only its original; historical published control never stops the service owner.
    /// 未发布放弃仅关闭其原实例；历史已发布控制绝不停止服务所有者。
    fn drop(&mut self) {
        if let Ok(ownership) = self.ownership.get_mut()
            && !matches!(
                ownership,
                RetainedReplacementOwnership::Published { .. }
                    | RetainedReplacementOwnership::Retired
            )
        {
            self.runtime.shutdown();
        }
    }
}
