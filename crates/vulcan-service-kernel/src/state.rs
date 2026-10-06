//! Core service state and shared task helpers.
//! 核心服务状态与共享任务辅助能力。

use super::*;
use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
    ConversationHookGenerationGuard, VulcanPluginConversationHooks,
};

/// Shared runtime handles used to execute real conversation turns.
/// 用于执行真实会话轮次的共享运行时句柄集合。
#[derive(Clone)]
pub(crate) struct AgentRuntimeBridge {
    /// Provider configuration captured during service startup.
    /// 服务启动期间捕获的 Provider 配置。
    pub(crate) provider_configuration: ProviderConfigurationFile,
    /// Process-local public AI runtime shared by model synchronization and execution.
    /// 由模型同步与执行共享的进程内公共 AI 运行时。
    pub(crate) public_ai_runtime: Arc<vulcan_provider_adapters::PublicAiRuntime>,
    /// Host-authorized configured inference service for internal and restricted RPC callers.
    /// 面向内部与受限 RPC 调用方的宿主授权已配置推理服务。
    pub(crate) configured_inference_service:
        Arc<crate::configured_inference::ConfiguredInferenceService>,
    /// Unified native AI Port shared by built-in Rust consumers.
    /// 供内置 Rust 消费者共享的统一原生 AI Port。
    pub(crate) internal_ai_port: Arc<dyn vulcan_ai_contract::HostAiPort>,
    /// Request-scoped provider override retained only by a queued prompt runtime clone.
    /// 仅由排队提示词运行时克隆保留的请求级供应商覆盖。
    pub(crate) provider_id_override: Option<String>,
    /// Request-scoped model override retained only by a queued prompt runtime clone.
    /// 仅由排队提示词运行时克隆保留的请求级模型覆盖。
    pub(crate) model_id_override: Option<String>,
    /// Request-scoped exact credential connection retained only by a queued prompt runtime clone.
    /// 仅由排队提示词运行时克隆保留的请求级精确凭据连接。
    pub(crate) connection_id_override: Option<String>,
    /// Agent profile catalog reused by runtime preparation.
    /// 运行时准备阶段复用的 Agent 画像目录。
    pub(crate) agent_profile_catalog: AgentProfileCatalog,
    /// Immutable subagent catalog generation published by startup or an explicit settings refresh.
    /// 由启动流程或显式设置刷新发布的不可变子代理目录代次。
    pub(crate) subagent_catalog: Arc<vulcan_task_session::SubagentCatalog>,
    /// LuaSkills runtime handle reused by provider-visible tool execution.
    /// Provider 可见工具执行复用的 LuaSkills 运行时句柄。
    pub(crate) luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Host-owned Vulcan Plugin catalog and optional execution runtime.
    /// 宿主持有的 Vulcan 插件目录与可选执行运行时。
    pub(crate) vulcan_plugin_runtime: vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    /// First registered native host remains paired with this exact manager through bridge clones and updates.
    /// 首次注册原生宿主跨桥克隆及更新保持配对至此精确管理器。
    pub(crate) vulcan_plugin_native_mcp_host:
        vulcan_plugin_runtime::VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Atomically published plugin catalog and MCP runtime generation reused by one execution.
    /// 单次执行复用的原子发布插件目录与 MCP 运行时代次。
    pub(crate) extension_generation: Arc<ExtensionGeneration>,
    /// Shared task runtime used by nested subagent tools.
    /// 嵌套子代理工具使用的共享任务运行时。
    pub(crate) task_runtime: Arc<TaskRuntime>,
    /// Shared ask-question runtime used by the ask tool.
    /// ask 工具使用的共享提问运行时。
    pub(crate) question_runtime: Arc<QuestionRuntime>,
    /// Shared command-session runtime used by shell session tools.
    /// shell 会话工具使用的共享命令会话运行时。
    pub(crate) command_session_runtime: Arc<CommandSessionRuntime>,
    /// Shared conversation hook runtime used to mirror WebUI turn assembly semantics.
    /// 用于复刻 WebUI 回合组装语义的共享会话 hook 运行时。
    pub(crate) conversation_hook_runtime: Arc<Mutex<ConversationHookRuntime>>,
    /// Shared VMM hook runtime snapshot used by built-in memory hooks.
    /// 内置记忆 hook 使用的共享 VMM hook 运行时快照。
    pub(crate) vmm_hook_runtime: VmmHookRuntimeHandle,
}

/// Prepared Full mechanics retain one actual slot and inert host policies without granting source-specific authority.
/// 已准备 Full 机械逻辑保留单个实际槽及惰性宿主策略，不授予特定来源权威。
struct PreparedActivationPluginExtensionPublication {
    /// Exact current Full allocation stays pinned through the final adjacent pointer write.
    /// 精确当前 Full 分配跨最终相邻指针写入保持固定。
    expected: Arc<ExtensionGeneration>,
    /// The sole successor revision derives from the actual expected allocation.
    /// 唯一后继修订从实际预期分配派生。
    revision: u64,
    /// Same actual prepared partition access has no independent business admission.
    /// 相同实际已准备分区访问不具有独立业务入场。
    access: McpRuntimeAccess,
    /// Isolated host point policies contain no live turn state or base dispatcher lock.
    /// 孤立宿主点位策略不含实时回合状态或基础调度器锁。
    host: vulcan_hooks::HookDispatcher,
}

impl PreparedActivationPluginExtensionPublication {
    /// Prepare inert hooks for manager/catalog and replace slot once using the captured actual Full boundary.
    /// 为 manager／catalog 准备惰性 Hook，并使用捕获实际 Full 边界替换 slot 一次。
    /// Caller is inside its source-specific core publisher; return the committed Arc without callbacks or fallible work after writing.
    /// 调用方位于其特定来源核心发布器内；返回已提交 Arc，写入后不执行回调或可失败工作。
    fn publish_into(
        self,
        slot: &mut Arc<ExtensionGeneration>,
        manager: vulcan_plugin_runtime::VulcanPluginRuntimeManager,
        catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    ) -> Result<Arc<ExtensionGeneration>, String> {
        // Only the isolated dispatcher is consulted while core registration remains held.
        // 核心注册门保持持有时，仅访问孤立调度器。
        let hooks = VulcanPluginConversationHooks::prepare(&self.host, manager, catalog)
            .map_err(|error| format!("准备插件 Hook 代次失败：{error}"))?;
        // All projected surfaces retain the exact same final catalog and prepared native lease.
        // 全部投影暴露面保留精确相同最终目录及已准备原生租约。
        let next = Arc::new(ExtensionGeneration::new_with_plugin_conversation_hooks(
            self.revision,
            Arc::new(hooks),
            self.access,
            self.expected.prompt_skill_catalog(),
        ));
        ExtensionGeneration::publish_prepared(slot, &self.expected, next, |_| {})
    }
}

impl AgentRuntimeBridge {
    /// Freeze expected/access mechanics and copy host point policy before acquiring core publication registration.
    /// 取得核心发布注册门前固定 expected／access 机械逻辑并复制宿主点位策略。
    /// expected is this bridge's actual current slot; return inert preparation with the base hook mutex already released.
    /// expected 为此桥实际当前槽；返回基础 Hook 互斥锁已释放的惰性准备。
    fn prepare_activation_plugin_extension_publication(
        &self,
        expected: Arc<ExtensionGeneration>,
        access: McpRuntimeAccess,
    ) -> Result<PreparedActivationPluginExtensionPublication, String> {
        // The real Full allocation supplies the only successor revision.
        // 真实 Full 分配提供唯一后继修订。
        let revision = expected
            .revision()
            .checked_add(1)
            .ok_or("Extension generation revision exhausted.")?;
        // Active turns may enter plugins while holding the base mutex, so no core gate can be held during this copy.
        // 活动回合可能持有基础互斥锁进入插件，因此此复制期间不能持有核心门。
        let points = {
            // Release this actual base guard before any source-specific publisher enters registration.
            // 任一特定来源发布器进入注册门前释放此实际基础守卫。
            let mut runtime = self
                .conversation_hook_runtime
                .lock()
                .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
            runtime.dispatcher_mut().registry().point_descriptors()
        };
        // This dispatcher carries host policies only, never live listeners or previous plugin handlers.
        // 此调度器仅携带宿主策略，绝不携带实时监听器或先前插件处理器。
        let mut host = vulcan_hooks::HookDispatcher::new(vulcan_hooks::HookRegistry::new());
        host.register_points(points)
            .map_err(|error| format!("准备宿主 Hook 点位策略失败：{error}"))?;
        Ok(PreparedActivationPluginExtensionPublication {
            expected,
            revision,
            access,
            host,
        })
    }

    /// Shared drain deadline for retired user and plugin MCP partitions after generation publication.
    /// 代次发布后退役用户与插件 MCP 分区共用的排空期限。
    pub(crate) const RETIRED_MCP_PARTITION_DRAIN_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(30);

    /// Prepares a plugin catalog and its handlers under the base hook policy without publishing them.
    /// 依据基础 Hook 策略准备插件目录及处理器，不发布它们。
    /// `catalog` and `mcp_runtime` are the candidate resources; returns one complete successor or an error.
    /// `catalog` 与 `mcp_runtime` 是候选资源；返回完整后继代次或错误。
    pub(crate) fn prepare_plugin_extension_generation(
        &self,
        catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
        mcp_runtime: McpRuntimeAccess,
    ) -> Result<Arc<ExtensionGeneration>, String> {
        // Reserve the successor before acquiring or changing any route state.
        // 在取得或修改任何路由状态前预留后继修订号。
        let revision = self
            .extension_generation
            .revision()
            .checked_add(1)
            .ok_or_else(|| String::from("Extension generation revision exhausted."))?;
        // The base lock supplies host policy; active logical turns are never cloned.
        // 基础锁提供宿主策略；活动逻辑回合从不克隆。
        let mut runtime = self
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
        // Preparation retains only candidate plugin handlers and the exact catalog.
        // 准备过程仅保留候选插件处理器及精确目录。
        let hooks = VulcanPluginConversationHooks::prepare(
            runtime.dispatcher_mut(),
            self.vulcan_plugin_runtime.clone(),
            catalog,
        )
        .map_err(|error| format!("准备会话插件 Hook 代次失败：{error}"))?;
        Ok(Arc::new(
            ExtensionGeneration::new_with_plugin_conversation_hooks(
                revision,
                Arc::new(hooks),
                mcp_runtime,
                self.extension_generation.prompt_skill_catalog(),
            ),
        ))
    }

    /// Publish one ready original native partition into this bridge's actual full extension slot.
    /// 将单个就绪原原生分区发布至本桥实际完整扩展槽。
    /// native is the single original control and catalog is its original exact snapshot.
    /// native 为单个原控制，catalog 为其原精确快照。
    /// The caller must exclusively own this bridge, through the service lock or unpublished bootstrap ownership.
    /// 调用方须通过服务锁或未发布启动所有权排他拥有本桥。
    /// Returns the actual committed Arc and displaced access; retire displaced partitions only after releasing host ownership.
    /// 返回实际已提交 Arc 及被替换访问；仅释放宿主所有权之后退休被替换分区。
    pub(crate) fn publish_native_plugin_extension_generation(
        &mut self,
        native: &vulcan_agent_runtime::mcp::PublishedPluginMcpRuntime,
        catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    ) -> Result<(Arc<ExtensionGeneration>, McpRuntimeAccess), String> {
        if !self
            .vulcan_plugin_native_mcp_host
            .matches_manager(&self.vulcan_plugin_runtime)
        {
            return Err(String::from("插件原生宿主签发者与当前服务管理器不一致。"));
        }
        // Capture the real slot while the caller already holds its publication ownership.
        // 调用方已持有发布所有权时，捕获真实槽。
        let expected = Arc::clone(&self.extension_generation);
        // Exact displaced partitions survive through the adjacent commit without becoming historical control resources.
        // 精确被替换分区跨相邻提交存续，不成为历史控制资源。
        let current_mcp = expected.mcp_runtime();
        // Derive access before native guards acquire its observation registry, avoiding same-lock reentry.
        // 原生守卫取得观察注册表之前派生访问，避免同锁重入。
        let publication_access = native.publication_access(&current_mcp)?;
        // Prepare frozen hooks and release their base mutex before source registration or native exclusion is acquired.
        // 取得来源登记或原生互斥之前，准备冻结 Hook 并释放其基础互斥锁。
        let next =
            self.prepare_plugin_extension_generation(Arc::clone(&catalog), publication_access)?;
        // Clone the original private identities so the native borrow does not include the mutable full slot.
        // 克隆原私有身份，使原生借用不包含可变完整槽。
        let manager = self.vulcan_plugin_runtime.clone();
        // The first service issuer is reused; no per-publication host registration occurs.
        // 复用首次服务签发者；不进行每次发布的宿主登记。
        let authority = self.vulcan_plugin_native_mcp_host.clone();
        // The service publication owner precedes source, admission and business locks in this production path.
        // 此生产路径中，服务发布所有者先于来源、入场及业务锁。
        let publication = native
            .prepare_publication(&manager, &authority, &catalog)?
            .prepare_extension(&expected, next)?;
        // The sole actual slot write and original same-lease business opening remain one adjacent synchronous tail.
        // 唯一实际槽写入及原同租约业务开放保持为单个相邻同步尾段。
        let committed = publication.publish_into(&mut self.extension_generation)?;
        Ok((committed, current_mcp))
    }

    /// Publish the original selected update into the actual full slot while target business remains fenced.
    /// 将原已选更新发布至实际完整槽，同时目标业务继续被隔离。
    /// transaction and staged identify the original request and closed catalog; native is its original global MCP owner, if declared.
    /// transaction 及 staged 标识原请求及关闭目录；声明全局 MCP 时，native 为其原所有者。
    /// The caller exclusively owns this bridge; returns sealed publication and the actual committed Arc without completing or restarting resources.
    /// 调用方排他拥有此桥；返回封闭发布凭据及实际已提交 Arc，不完成或重启资源。
    pub(crate) fn publish_activation_target_plugin_extension_generation(
        &mut self,
        transaction: &vulcan_plugin_runtime::VulcanPluginActivationTransaction,
        staged: &Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
        native: Option<&vulcan_agent_runtime::mcp::TargetPluginMcpRuntime>,
    ) -> Result<
        (
            vulcan_plugin_runtime::VulcanPluginActivationTargetPublication,
            Arc<ExtensionGeneration>,
        ),
        String,
    > {
        if !self
            .vulcan_plugin_native_mcp_host
            .matches_manager(&self.vulcan_plugin_runtime)
        {
            return Err(String::from("插件原生宿主签发者与当前服务管理器不一致。"));
        }
        // The original target declaration decides whether native ownership is required.
        // 原目标声明决定是否要求原生所有权。
        let plugin = staged
            .plugins
            .get(&transaction.record().target().plugin_id)
            .ok_or("原更新目标不在暂存目录中。")?;
        // A missing or extra native participant cannot silently change the declared resource boundary.
        // 缺失或多余的原生参与者不能静默改变声明的资源边界。
        let has_global_mcp = plugin.mcp_servers.iter().any(|server| {
            server.manifest.scope == vulcan_plugin_runtime::VulcanPluginMcpScopeManifest::Global
        });
        if has_global_mcp != native.is_some() {
            return Err(String::from(
                "原更新目标的全局 MCP 声明与实际所有者不一致。",
            ));
        }
        if let Some(native) = native {
            // The same service registration must issue the real target, not another registration on an equal manager.
            // 真实目标必须由同次服务登记签发，不能来自相等管理器上的另一登记。
            let authority =
                vulcan_plugin_runtime::VulcanPluginNativeMcpHandoffAdapter::authority(native)
                    .ok_or("原更新目标缺少原生宿主签发者。")?;
            if !authority.same_issuer(&self.vulcan_plugin_native_mcp_host) {
                return Err(String::from("原更新目标与服务首次原生签发者不一致。"));
            }
        }
        // Capture the sole slot and its successor before acquiring the manager publication gate.
        // 在取得管理器发布门之前，捕获唯一槽及其后继。
        let expected = Arc::clone(&self.extension_generation);
        // Derive only this plugin partition while the actual native registries remain unlocked.
        // 实际原生注册表尚未加锁时，仅派生此插件分区。
        let access = match native {
            Some(native) => native.publication_access(&expected.mcp_runtime())?,
            None => vulcan_agent_runtime::mcp::remove_plugin_mcp_runtime_partition(
                &expected.mcp_runtime(),
                &plugin.id,
            ),
        };
        // Shared mechanics copy inert host policies only, retaining this actual boundary without merging target authority.
        // 共用机械逻辑仅复制惰性宿主策略，保留此实际边界而不合并目标权威。
        let prepared = self.prepare_activation_plugin_extension_publication(expected, access)?;
        // Clone only the same manager authority so the publisher can borrow the actual full slot mutably.
        // 仅克隆同一管理器权威，使发布器可以可变借用实际完整槽。
        let manager = self.vulcan_plugin_runtime.clone();
        // Retain the real commit before returning the sealed core witness; no callback or await follows the pointer write.
        // 返回封闭核心凭据之前保留真实提交；指针写入之后没有回调或等待。
        let mut committed = None;
        // The core installs its exact provisional Enabled catalog under registration before validating inert handlers.
        // 核心在注册门内安装精确临时 Enabled 目录，然后校验惰性处理器。
        let publication = manager
            .publish_activation_target_handoff(transaction, staged, |catalog| {
                // Source-specific core publication supplies the exact final catalog to the shared final pointer writer.
                // 特定来源核心发布将精确最终目录提供给共用最终指针写入器。
                committed = Some(prepared.publish_into(
                    &mut self.extension_generation,
                    manager.clone(),
                    catalog,
                )?);
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        Ok((
            publication,
            committed.expect("successful original full publisher committed its actual slot"),
        ))
    }

    /// Publish one actual restored predecessor into the Full slot while its private Hook/native owners still deny business.
    /// 将单个实际恢复前驱发布至 Full 槽，同时其私有 Hook／原生所有者仍拒绝业务。
    /// transaction/source/staged bind the original Aborted request and actual closed old route; native is required only for global MCP.
    /// transaction／source／staged 绑定原 Aborted 请求及实际关闭旧路由；仅全局 MCP 要求 native。
    /// Return the genuine core seal and actual committed Full Arc without transfer, initialization or another recovery File.
    /// 返回真实核心封闭凭据及实际已提交 Full Arc，不转交、初始化或打开另一恢复文件。
    pub(crate) fn publish_activation_predecessor_plugin_extension_generation(
        &mut self,
        transaction: &vulcan_plugin_runtime::VulcanPluginActivationTransaction,
        source: &Arc<vulcan_plugin_runtime::VulcanPluginPredecessorRestorationSource>,
        staged: &Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
        native: Option<&vulcan_agent_runtime::mcp::PredecessorPluginMcpRuntime>,
    ) -> Result<
        (
            vulcan_plugin_runtime::VulcanPluginActivationPredecessorPublication,
            Arc<ExtensionGeneration>,
        ),
        String,
    > {
        if !self
            .vulcan_plugin_native_mcp_host
            .matches_manager(&self.vulcan_plugin_runtime)
        {
            return Err(String::from("插件原生宿主签发者与当前服务管理器不一致。"));
        }
        // Old identity comes only from the original predecessor selection, never the aborted target's payload.
        // 旧身份仅来自原前驱选择，绝不来自中止目标的载荷。
        let old = transaction
            .record()
            .predecessor()
            .ok_or("原更新前驱缺少完整托管选择。")?;
        // This route is merely an observation; the actual core publisher independently checks Source's exact closed Arc and Pending File.
        // 此路由仅为观测；实际核心发布器独立检查 Source 的精确关闭 Arc 及 Pending 文件。
        let plugin = staged
            .plugins
            .get(&old.plugin_id)
            .ok_or("原更新前驱不在关闭目录中。")?;
        if transaction.record().phase()
            != vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Aborted
            || source.request_id() != transaction.record().request_id()
            || plugin.runtime_state != vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabling
            || !plugin
                .managed_package()
                .is_some_and(|managed| managed.matches_activation(old))
        {
            return Err(String::from(
                "原更新前驱与完整已中止旧选择或关闭路由不一致。",
            ));
        }
        // Declaration and native participation must match exactly, even when every declared server is disabled.
        // 即使每个已声明服务器均停用，声明及原生参与也必须精确匹配。
        let has_global_mcp = plugin.mcp_servers.iter().any(|server| {
            server.manifest.scope == vulcan_plugin_runtime::VulcanPluginMcpScopeManifest::Global
        });
        if has_global_mcp != native.is_some() {
            return Err(String::from(
                "原更新前驱的全局 MCP 声明与实际所有者不一致。",
            ));
        }
        if let Some(native) = native {
            // Compare the first issuer and actual weak Source allocation before selecting any native Full access.
            // 选择任何原生 Full 访问前比较首次签发者及实际弱 Source 分配。
            let authority =
                vulcan_plugin_runtime::VulcanPluginNativeMcpPredecessorHandoffAdapter::authority(
                    native,
                )
                .ok_or("原更新前驱缺少原生宿主签发者。")?;
            // Control values are diagnostics alongside actual Source allocation, never standalone restoration authority.
            // 控制值为实际 Source 分配旁的诊断，绝不独立授予恢复权威。
            let control = native.control();
            if !authority.same_issuer(&self.vulcan_plugin_native_mcp_host)
                || !control.matches_source(source)
                || control.request_id() != source.request_id()
                || control.plugin_id() != old.plugin_id
                || control.generation() != plugin.generation
            {
                return Err(String::from("原更新前驱与服务首次签发者或实际来源不一致。"));
            }
        }
        // Capture the sole current Full allocation before entering core registration.
        // 进入核心注册门前捕获唯一当前 Full 分配。
        let expected = Arc::clone(&self.extension_generation);
        // Access changes only this old plugin partition and keeps the same actual original native lease.
        // 访问仅改变此旧插件分区，并保留相同实际原原生租约。
        let access = match native {
            Some(native) => native.publication_access(&expected.mcp_runtime())?,
            None => vulcan_agent_runtime::mcp::remove_plugin_mcp_runtime_partition(
                &expected.mcp_runtime(),
                &old.plugin_id,
            ),
        };
        // Preparation shares only inert Full mechanics; predecessor authority remains exclusively with its core publisher.
        // 准备仅共享惰性 Full 机械逻辑；前驱权威仍完全属于其核心发布器。
        let prepared = self.prepare_activation_plugin_extension_publication(expected, access)?;
        // Clone the original manager only, so the callback can borrow the actual Full slot mutably.
        // 仅克隆原管理器，使回调可以可变借用实际 Full 槽。
        let manager = self.vulcan_plugin_runtime.clone();
        // Record the actual Full result before returning its successful core seal; pointer writing is the callback's final effect.
        // 返回其成功核心封闭凭据前记录实际 Full 结果；指针写入为回调最终副作用。
        let mut committed = None;
        // Strict original closed Source and Pending remain mandatory for first publication, independently of service observations.
        // 首次发布仍强制要求严格原关闭 Source 及 Pending，独立于服务观测。
        let publication = manager
            .publish_activation_predecessor_handoff(transaction, source, |catalog| {
                committed = Some(prepared.publish_into(
                    &mut self.extension_generation,
                    manager.clone(),
                    catalog,
                )?);
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        Ok((
            publication,
            committed
                .expect("successful original predecessor full publisher committed its actual slot"),
        ))
    }

    /// Locks shared turn state and projects this bridge's captured plugin hook generation.
    /// 锁定共享回合状态，并投影本桥捕获的插件 Hook 代次。
    /// Returns a scoped guard; lock or projection failure leaves the base dispatcher unchanged.
    /// 返回作用域守卫；加锁或投影失败时保持基础调度器不变。
    pub(crate) fn lock_conversation_hooks(
        &self,
    ) -> Result<ConversationHookGenerationGuard<'_>, String> {
        // Acquire the existing mutex; no independent conversation state is introduced.
        // 取得现有互斥锁，不引入独立会话状态。
        let runtime = self
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
        ConversationHookGenerationGuard::new(
            runtime,
            self.extension_generation.plugin_conversation_hooks(),
        )
        .map_err(|error| format!("投影会话插件 Hook 代次失败：{error}"))
    }
}

/// Session-scoped AI capability configuration save request sent by the WebUI composer dialog.
/// WebUI 输入区控制项发送的会话计划模式更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionPlanModeUpdateRequest {
    /// Conversation id whose Plan mode state should change.
    /// 需要更新计划模式状态的会话标识。
    pub(crate) conversation_id: Option<String>,
    /// Whether Plan mode should be enabled for the selected session.
    /// 是否为当前会话启用计划模式。
    pub(crate) enabled: bool,
}

/// Session-scoped VMM memory policy update request sent by the WebUI composer menu.
/// WebUI 输入区菜单发送的会话级 VMM 记忆策略更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionVmmPolicyUpdateRequest {
    /// Conversation id whose VMM memory policy should change.
    /// 需要更新 VMM 记忆策略的会话标识。
    pub(crate) conversation_id: Option<String>,
    /// Policy key to update: precheck or writeback.
    /// 需要更新的策略键：前置注入或后置提交。
    pub(crate) policy: String,
    /// New tri-state policy mode: inherit, enabled, or disabled.
    /// 新的三态策略模式：跟随、开启或关闭。
    pub(crate) mode: String,
}

/// Maximum loaded subagent transcript surfaces retained by the Core service.
/// Core 服务保留的已加载子代理转录界面最大数量。
pub(crate) const SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT: usize = 128;

/// Milliseconds before an unused loaded subagent transcript surface can be evicted.
/// 已加载但未使用的子代理转录界面允许被清理前的毫秒数。
pub(crate) const SUBAGENT_SURFACE_DETAIL_CACHE_TTL_MS: i128 = 30 * 60 * 1000;

/// Exact child execution identity used by parent-turn bindings and terminal delivery.
/// 父回合绑定与终态投递使用的精确子执行身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SubagentExecutionKey {
    /// Stable child task session reused across executions.
    /// 跨执行复用的稳定子任务会话。
    pub(crate) task_id: SessionId,
    /// Unique concrete execution attempt.
    /// 唯一具体执行尝试。
    pub(crate) execution_id: String,
}

impl SubagentExecutionKey {
    /// Builds one owned key from a stable task and concrete execution identity.
    /// 从稳定任务与具体执行身份构建一个有所有权的键。
    pub(crate) fn new(task_id: SessionId, execution_id: &str) -> Self {
        Self {
            task_id,
            execution_id: execution_id.to_string(),
        }
    }
}

/// Long-lived in-memory detail cache for one loaded WebUI subagent surface.
/// 单个已加载 WebUI 子代理界面的长生命周期内存详情缓存。
#[derive(Debug, Clone)]
pub(crate) struct WebuiSubagentSurfaceDetailCacheEntry {
    /// Cached task panel state including durable child conversation detail when available.
    /// 缓存的任务面板状态，包含可用的持久子会话详情。
    pub(crate) task_state: SubagentTaskPanelState,
    /// Last access timestamp in Unix milliseconds.
    /// 最近访问时间戳，单位为 Unix 毫秒。
    pub(crate) last_access_ms: i128,
}

/// Authoritative reason why one session temporarily rejects new kernel turn admission.
/// 单个会话暂时拒绝新 kernel 回合准入的权威原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionTurnAdmissionReservationKind {
    /// One existing-session user submission owns turn registration until its kernel commit.
    /// 单个已有会话用户提交在 kernel 提交前拥有回合登记权。
    PromptSubmission,
    /// The session is crossing an archive or removal lifecycle transaction.
    /// 会话正在跨越归档或删除生命周期事务。
    LifecycleTransition,
    /// The session Goal authority is being changed and stale dispatch must not register.
    /// 会话 Goal 权威正在变更，过期分发不得注册。
    GoalMutation,
    /// The active conversation run is being stopped and no replacement turn may start yet.
    /// 活动会话运行正在停止，替代回合尚不得启动。
    RunStop,
}

impl SessionTurnAdmissionReservationKind {
    /// Returns the stable diagnostic label for this reservation reason.
    /// 返回此预留原因的稳定诊断标签。
    pub(crate) const fn diagnostic_label(self) -> &'static str {
        match self {
            Self::PromptSubmission => "user prompt submission",
            Self::LifecycleTransition => "archive or removal lifecycle transition",
            Self::GoalMutation => "Goal authority mutation",
            Self::RunStop => "conversation run stop",
        }
    }
}

/// Owned reservation that blocks new kernel turns for one exact session.
/// 阻止单个精确会话启动新 kernel 回合的有所有权预留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionTurnAdmissionReservation {
    /// Unique owner identity used to prevent a stale guard from deleting a replacement reservation.
    /// 用于防止过期守卫删除替代预留的唯一所有者身份。
    pub(crate) reservation_id: Uuid,
    /// Domain reason that owns the reservation.
    /// 持有此预留的领域原因。
    pub(crate) kind: SessionTurnAdmissionReservationKind,
}

/// Exact in-process owner that keeps one session closed while terminal state is being persisted.
/// 在终态持久化期间保持单个会话关闭的精确进程内所有者。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionTerminalPersistenceOwner {
    /// A registered stream task accepted or synthesized the terminal boundary.
    /// 已注册流任务接纳或合成了终态边界。
    StreamTask(Uuid),
    /// A pre-registration failure terminalized the exact session-kernel generation.
    /// 注册前失败终结了精确的会话内核代际。
    KernelGeneration(u64),
    /// A prompt-preparation cleanup owns one unique terminal settlement transaction.
    /// 提示词准备清理持有一个唯一的终态结算事务。
    PromptPreparation(Uuid),
    /// A conversation Stop transaction owns teardown, retry refreeze, and durable stopped-state settlement.
    /// 会话 Stop 事务持有拆除、重试重冻与可靠停止状态结算。
    RunStop(Uuid),
    /// A pending settlement rebuilt from the durable V1 document after a restart.
    /// 重启后从可靠 V1 文档重建的待结算项。
    RecoveredSettlement {
        /// Exact original owner generation identity recorded in the durable settlement intent.
        /// 可靠结算意图中记录的精确原始所有者代际身份。
        generation: String,
    },
}

/// Retains the exact fallback data while a terminal owner has not yet persisted its intent.
/// 在终态所有者尚未持久化意图时保留精确的回退数据。
#[derive(Debug, Clone)]
pub(crate) struct TerminalPreIntentRepair {
    /// Exact owner generation that may retry this boundary.
    /// 获准重试该边界的精确所有者代际。
    pub(crate) owner_generation: String,
    /// Original stable terminal reason selected by the failed stream.
    /// 失败流选择的原始稳定终态原因。
    pub(crate) reason_code: &'static str,
    /// Original safe failure detail retained only for this process-local retry.
    /// 仅为当前进程内重试保留的原始安全失败详情。
    pub(crate) redacted_detail: Option<String>,
    /// Exact execution scope retained even before the durable intent can be written.
    /// 在可靠意图写入前也保留的精确执行范围。
    pub(crate) scope: vulcan_session_core::v1::TerminalSettlementScope,
}

impl SessionTerminalPersistenceOwner {
    /// Returns the stable durable identity of this exact owner generation.
    /// 返回该精确所有者代际的稳定可靠身份。
    ///
    /// The identity is recorded in the durable terminal settlement intent, so a
    /// rebuild after restart and an idempotent retry can recognize the exact
    /// original boundary without guessing.
    /// 该身份会写入可靠终态结算意图，使重启后的重建与幂等重试都能识别精确的原始
    /// 边界而无需猜测。
    pub(crate) fn owner_generation(&self) -> String {
        match self {
            Self::StreamTask(task_id) => format!("stream-task:{task_id}"),
            Self::KernelGeneration(generation) => format!("kernel-generation:{generation}"),
            Self::PromptPreparation(owner_id) => format!("prompt-preparation:{owner_id}"),
            Self::RunStop(owner_id) => format!("run-stop:{owner_id}"),
            Self::RecoveredSettlement { generation } => generation.clone(),
        }
    }

    /// Returns the stable diagnostic label for traces and user-facing diagnostics.
    /// 返回用于追踪与用户可见诊断的稳定标签。
    pub(crate) const fn diagnostic_label(&self) -> &'static str {
        match self {
            Self::StreamTask(_) => "stream task terminal settlement",
            Self::KernelGeneration(_) => "kernel generation terminal settlement",
            Self::PromptPreparation(_) => "prompt preparation terminal settlement",
            Self::RunStop(_) => "conversation stop terminal settlement",
            Self::RecoveredSettlement { .. } => "recovered terminal settlement",
        }
    }
}

/// Project-level scope serialized against lifecycle transitions and prompt submissions.
/// 与生命周期转换和提示词提交串行化的项目级作用域。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProjectAdmissionScope {
    /// One exact active project identity.
    /// 单个精确活动项目身份。
    Exact(String),
    /// A submission whose authoritative project cannot be selected before workspace loading.
    /// 在工作区加载前无法选择权威项目的提交。
    UnresolvedSubmission,
}

/// Authoritative reason why one project scope temporarily rejects competing mutations.
/// 单个项目作用域暂时拒绝竞争变更的权威原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectAdmissionReservationKind {
    /// A prompt submission owns project membership until its workspace commit.
    /// 提示词提交在工作区提交前拥有项目成员关系。
    PromptSubmission,
    /// The project is crossing an archive or removal lifecycle transaction.
    /// 项目正在跨越归档或移除生命周期事务。
    LifecycleTransition,
}

impl ProjectAdmissionReservationKind {
    /// Returns the stable diagnostic label for this project reservation reason.
    /// 返回此项目预留原因的稳定诊断标签。
    pub(crate) const fn diagnostic_label(self) -> &'static str {
        match self {
            Self::PromptSubmission => "user prompt submission",
            Self::LifecycleTransition => "archive or removal lifecycle transition",
        }
    }
}

/// Owned reservation that blocks conflicting work for one project admission scope.
/// 阻止单个项目准入作用域冲突工作的有所有权预留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectAdmissionReservation {
    /// Unique owner identity used to protect successor reservations from stale cleanup.
    /// 用于保护后继预留不被过期清理删除的唯一所有者身份。
    pub(crate) reservation_id: Uuid,
    /// Domain reason that owns the project scope.
    /// 持有项目作用域的领域原因。
    pub(crate) kind: ProjectAdmissionReservationKind,
}

/// Retained service lifecycle diagnostics used by the protected status endpoint.
/// 受保护状态端点使用的保留服务生命周期诊断。
#[derive(Debug, Clone, Default)]
pub(crate) struct ServiceLifecycleState {
    /// Exact bootstrap failure retained after `update_service_failure`, if any.
    /// `update_service_failure` 之后保留的精确引导失败（如有）。
    pub(crate) last_bootstrap_failure: Option<ServiceStatusFailure>,
    /// The required HTTP bridge has stopped, so a concurrent bootstrap may not publish ready.
    /// 必需的 HTTP 桥已经停止，因此并行引导不得再发布 ready。
    pub(crate) http_bridge_failed: bool,
    /// Whether the shutdown coordinator currently owns admission.
    /// 关闭协调器当前是否持有准入。
    pub(crate) shutdown_drain_active: bool,
    /// Exact recovery failure retained when failed-shutdown admission could not reopen.
    /// 失败关闭准入无法重新开放时保留的精确恢复失败。
    pub(crate) last_recovery_failure: Option<ServiceStatusFailure>,
    /// Once a process-owned runtime is stopped, failed shutdown cannot safely reopen admission.
    /// 进程持有的运行时一旦停止，关闭失败后就不能安全地重新开放准入。
    pub(crate) shutdown_recovery_blocked: bool,
    /// Sole original plugin manager retained before native initialization, including failed bootstrap.
    /// 原生初始化之前保留的唯一原插件管理器，包含失败引导。
    pub(crate) original_plugin_manager: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
}

impl ServiceLifecycleState {
    /// Marks the shutdown coordinator as the current admission owner.
    /// 将关闭协调器标记为当前准入持有者。
    pub(crate) fn begin_shutdown_drain(&mut self) {
        self.shutdown_drain_active = true;
        self.last_recovery_failure = None;
        // A retry cannot undo the previous permanent native or process-runtime closure.
        // 重试不能撤销先前永久的原生或进程运行时关闭。
    }

    /// Marks the irreversible shutdown boundary before stopping process-owned runtimes.
    /// 在停止进程持有的运行时之前标记不可逆关闭边界。
    pub(crate) fn block_shutdown_recovery(&mut self) {
        self.shutdown_recovery_blocked = true;
    }

    /// Clears shutdown ownership after the drain succeeds or admission is reopened.
    /// 排空成功或准入重新开放后清除关闭归属。
    pub(crate) fn end_shutdown_drain(&mut self) {
        self.shutdown_drain_active = false;
        self.last_recovery_failure = None;
        // Successful recovery is admitted only before the irreversible boundary; never erase it here.
        // 成功恢复仅允许发生在不可逆边界之前；此处绝不抹除该边界。
    }

    /// Retains one exact recovery failure and clears shutdown ownership.
    /// 保留一条精确恢复失败并清除关闭归属。
    pub(crate) fn retain_recovery_failure(&mut self, failure: ServiceStatusFailure) {
        self.shutdown_drain_active = false;
        self.last_recovery_failure = Some(failure);
    }
}

/// Process-local compatibility owner for one session's pending-prompt UI and Hook queue.
/// 单个会话待发送提示词界面与 Hook 队列的进程内兼容 owner。
///
/// This owner is intentionally outside `WorkspaceSnapshot` and has no serialization contract.
/// Durable queue authority lives only in Session Persistence V1 metadata.
/// 该 owner 有意位于 `WorkspaceSnapshot` 之外，且没有序列化契约；可靠队列权威只存在于
/// Session Persistence V1 metadata。
#[derive(Debug, Clone, Default)]
pub(crate) struct PendingPromptProcessMirror {
    /// Queue-only transcript UI state consumed by legacy process-local adapters.
    /// 仅供旧进程内适配器消费的队列转录界面状态。
    pub(crate) ui_state: vulcan_workbench_model::SessionTranscriptUiState,
}

/// Shared service state exposed to Tauri commands.
/// 暴露给 Tauri 命令的共享服务状态。
pub struct ServiceBridgeState {
    /// Host shell capabilities supplied by the concrete application shell.
    /// 由具体应用壳提供的宿主 shell 能力。
    pub(crate) shell_host: Arc<dyn ServiceShellHost>,
    /// Latest startup snapshot.
    /// 最近一次启动快照。
    pub(crate) startup: StartupSnapshot,
    /// Latest workbench snapshot.
    /// 最近一次工作台快照。
    pub(crate) workbench: WorkbenchSnapshot,
    /// Monotonic version for workspace-driven workbench rebuilds.
    /// 工作区驱动的工作台重建单调版本号。
    pub(crate) workbench_generation: u64,
    /// Latest workspace snapshot retained for streaming turn updates.
    /// 为流式会话轮次更新保留的最近工作区快照。
    pub(crate) workspace_snapshot: Option<WorkspaceSnapshot>,
    /// Process-local pending-prompt compatibility mirrors keyed by session id.
    /// 按会话标识索引的进程内待发送提示词兼容镜像。
    pub(crate) pending_prompt_process_mirrors: BTreeMap<SessionId, PendingPromptProcessMirror>,
    /// Long-lived shared agent runtime bridge.
    /// 长生命周期共享 Agent 运行时桥接。
    pub(crate) agent_runtime: Option<AgentRuntimeBridge>,
    /// Host-owned native VMM project maintenance facade.
    /// 宿主持有的原生 VMM 项目维护门面。
    pub(crate) native_vmm_maintenance: Option<crate::native_vmm::NativeVmmMaintenance>,
    /// Host-native VCE project lifecycle and retrieval manager.
    /// 宿主原生 VCE 项目生命周期与检索管理器。
    pub(crate) vce_runtime: Option<crate::vce::VceRuntimeManager>,
    /// VCE subscriber over the shared workspace watcher.
    /// 共享工作区监听器之上的 VCE 订阅器。
    pub(crate) vce_watcher: Option<crate::vce::VceWatcherService>,
    /// Service-owned physical filesystem watcher shared by independent consumers.
    /// 由服务持有并供独立消费者共享的物理文件系统监听器。
    pub(crate) workspace_watcher: Option<crate::workspace_watcher::WorkspaceWatcherCenter>,
    /// Process-wide registry owning one serialized VMM controller per session.
    /// 进程级注册表，为每个会话拥有一个串行化 VMM 控制器。
    pub(crate) vmm_controller_registry: crate::vmm_session_controller::SessionVmmControllerRegistry,
    /// Tokio runtime that owns bootstrap-spawned background tasks such as shared MCP access.
    /// 持有引导期后台任务的 Tokio 运行时，例如共享 MCP 访问任务。
    pub(crate) service_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Browser service discovery snapshot shared by `/health` and the dev discovery file.
    /// `/health` 与开发发现文件共享的浏览器服务发现快照。
    pub(crate) webui_service_discovery: Option<SharedWebuiServiceDiscoverySnapshot>,
    /// Long-lived controller runtime handle.
    /// 长生命周期控制器运行时句柄。
    pub(crate) controller_runtime: Option<HostRuntimeHandle>,
    /// Long-lived host-native knowledge-base runtime.
    /// 长生命周期宿主原生知识库运行时。
    pub(crate) knowledge_runtime:
        Option<vulcan_workspace_store::knowledge_base::KnowledgeBaseRuntime>,
    /// Long-lived LuaSkills runtime handle.
    /// 长生命周期 LuaSkills 运行时句柄。
    pub(crate) luaskills_runtime: Option<TauriLuaSkillsHandle>,
    /// Owned installation that keeps the process-wide LuaSkills database callbacks alive.
    /// 持有进程级 LuaSkills 数据库回调生命周期的安装所有者。
    pub(crate) host_database_providers: Option<
        Arc<
            vulcan_agent_runtime::luaskills_runtime::host_database_providers::InstalledHostDatabaseProviders,
        >,
    >,
    /// Cached LuaSkills catalog captured during startup or explicit refresh.
    /// 启动或显式刷新时捕获的 LuaSkills 目录缓存。
    pub(crate) cached_luaskills_catalog: Option<TauriLuaSkillsCatalog>,
    /// Long-lived injected provider-manifest sync service; RPCs reuse this instance instead of
    /// constructing empty-key/None-URL services per call (plan §R7.13).
    /// 长生命周期注入的供应商清单同步服务；RPC 复用该实例，而不是每次调用构造空密钥/
    /// None URL 的服务（方案 §R7.13）。
    pub(crate) provider_manifest_sync:
        Option<crate::provider_manifest_sync::ProviderManifestSyncService>,
    /// Latest context-budget snapshots keyed by session id.
    /// 按会话标识索引的最近上下文预算快照。
    pub(crate) live_context_budgets: BTreeMap<SessionId, ContextBudgetSnapshot>,
    /// Latest goal summaries keyed by session id for synchronous workbench projection.
    /// 按会话标识索引的最近目标摘要，用于同步构建工作台投影。
    pub(crate) session_goal_summaries: BTreeMap<SessionId, SessionGoalSummary>,
    /// Long-lived loaded child transcript details keyed by child task id.
    /// 按子任务标识索引的长生命周期已加载子转录详情。
    pub(crate) subagent_surface_detail_cache:
        BTreeMap<SessionId, WebuiSubagentSurfaceDetailCacheEntry>,
    /// Exact child ids admitted to each parent's current round; reset only when that parent starts a new user turn.
    /// 各父会话当前回合准入的精确子任务标识；仅在该父会话开始新用户回合时重置。
    pub(crate) session_subagent_navigation_rounds: BTreeMap<SessionId, BTreeSet<SessionId>>,
    /// Currently opened subagent surface selection.
    /// 当前打开的子代理界面选择。
    pub(crate) active_subagent_panel: Option<SelectedSubagentTaskPanel>,
    /// Current detached child execution owned by each WebUI listener task id.
    /// 每个 WebUI 监听任务标识当前拥有的分离子执行。
    pub(crate) detached_subagent_task_listeners: BTreeMap<SessionId, String>,
    /// Exact child executions that should not block the parent turn completion gate.
    /// 不应阻塞父回合完成闸门的精确子执行集合。
    pub(crate) subagent_completion_gate_ignored_tasks: BTreeSet<SubagentExecutionKey>,
    /// Parent sessions currently blocked inside a model-requested wait_agent call.
    /// 当前正阻塞在模型请求的 wait_agent 调用中的父会话集合。
    pub(crate) parent_sessions_waiting_for_subagents: BTreeSet<SessionId>,
    /// Active WebUI stream task handles keyed by session id.
    /// 按会话标识索引的 WebUI 活动流任务句柄。
    pub(crate) active_stream_tasks: BTreeMap<SessionId, ActiveStreamTask>,
    /// Cancellable auxiliary preparation, separate from its eventual Provider stream.
    /// 可取消辅助准备，与其后续 Provider 流分别登记。
    pub(crate) question_preparation_tasks: BTreeMap<SessionId, crate::agent_collaboration_execution::preparation_tasks::QuestionPreparationTask>,
    /// Scheduled collaboration wakeups not yet handed to an execution owner.
    /// 尚未交接给执行所有者的已调度协作唤醒。
    pub(crate) collaboration_dispatch_owners: BTreeMap<SessionId, usize>,
    /// Exact stream owners currently committing their accepted terminal state to durable storage.
    /// 当前正在把已接纳终态提交到可靠存储的精确流所有者。
    pub(crate) session_terminal_persistence_owners:
        BTreeMap<SessionId, SessionTerminalPersistenceOwner>,
    /// Child auxiliary admission retained independently of abortable stream futures.
    /// 独立于可中止流 Future 保留的子辅助准入。
    pub(crate) child_question_execution_owners: BTreeMap<
        SessionId,
        crate::agent_collaboration::child_execution_ownership::ChildQuestionExecutionOwner,
    >,
    /// Maintenance exclusion retained until its own durable terminal is confirmed.
    /// 在确认维护自身可靠终态前保留的维护互斥。
    pub(crate) child_maintenance_owners: BTreeMap<
        SessionId,
        crate::agent_collaboration::child_maintenance::ChildMaintenanceOwner,
    >,
    /// Service-owned work from publication waiting through durable maintenance settlement.
    /// 从等待发布到可靠维护结算的服务自有工作。
    pub(crate) child_maintenance_tasks: BTreeMap<
        SessionId,
        crate::agent_collaboration::child_maintenance::ChildMaintenanceTask,
    >,
    /// Exact stopped business identities reject late maintenance without blocking a new business execution.
    /// 精确已停止业务身份拒绝迟到维护，不阻止新的业务执行。
    pub(crate) child_maintenance_stops: BTreeMap<SessionId, String>,
    /// Exact pre-intent terminal retries for owners retained after an early failure.
    /// 早期失败后保留的所有者对应的精确意图前终态重试。
    pub(crate) terminal_pre_intent_repairs: BTreeMap<SessionId, TerminalPreIntentRepair>,
    /// Session runtime kernels keyed by root WebUI session id.
    /// 按 WebUI 根会话标识索引的会话运行时内核。
    pub(crate) session_runtime_kernels: BTreeMap<SessionId, SessionRuntimeKernel>,
    /// One latest prewarm lifecycle owner per known session, retained after receipt acknowledgement.
    /// 每个已知会话保留一个最近预热生命周期所有者，在回执确认后仍保留。
    pub(crate) automatic_plugin_prewarm: BTreeMap<SessionId, crate::plugin_runtime_services::automatic_prewarm::AutomaticPrewarmTurn>,
    /// Unique V1 mutable session owners keyed by root WebUI session id.
    /// 按 WebUI 根会话标识索引的唯一 V1 可变会话所有者。
    pub(crate) session_runtime_registry:
        Arc<crate::session_runtime_registry_v1::SessionRuntimeRegistry>,
    /// Retained service lifecycle diagnostics used by the protected status endpoint.
    /// 受保护状态端点使用的保留服务生命周期诊断。
    pub(crate) service_lifecycle_state: ServiceLifecycleState,
    /// Actual original plugin updates outlive executor failures and foreground RPC waiters.
    /// 实际原插件更新比执行器故障及前台 RPC 等待者存活更久。
    pub(crate) pending_local_plugin_updates:
        Arc<crate::skills_services::PendingLocalPluginUpdateRegistry>,
    /// Original Legacy credential guards and preimages survive executor failure until confirmed recovery.
    /// 原旧凭据屏障及前像跨执行器故障存续，直至确认恢复。
    pub(crate) pending_legacy_secret_mutations:
        Arc<crate::skills_services::LegacySecretMutationRegistry>,
    /// Same physical removal originals outlive callbacks, failed publication and executor cancellation.
    /// 相同物理移除原实例跨回调、发布失败及执行器取消存续。
    pub(crate) pending_local_plugin_uninstalls:
        Arc<crate::skills_services::PendingLocalPluginUninstallRegistry>,
    /// Unique project-level resource catalog owners keyed by exact storage partition path.
    /// 按精确存储分区路径索引的唯一项目级资源目录 owner。
    pub(crate) project_resource_catalog_registry:
        Arc<vulcan_workspace_store::workspace_store::ProjectResourceCatalogRegistry>,
    /// Owned turn-admission reservations keyed by exact session id.
    /// 按精确会话标识索引的有所有权回合准入预留。
    pub(crate) session_turn_admission_reservations:
        BTreeMap<SessionId, SessionTurnAdmissionReservation>,
    /// Owned project-admission reservations keyed by exact or unresolved scope.
    /// 按精确或未解析作用域索引的有所有权项目准入预留。
    pub(crate) project_admission_reservations:
        BTreeMap<ProjectAdmissionScope, ProjectAdmissionReservation>,
    /// Shared Core browser session manager owned by this service process.
    /// 本服务进程拥有的共享 Core 浏览器会话管理器。
    pub(crate) browser_manager: crate::browser::BrowserManager,
}

impl Drop for ServiceBridgeState {
    /// Releases the process-wide LuaSkills database provider slot before its owned handle.
    /// 在释放自身持有的句柄之前，先释放进程级 LuaSkills 数据库 provider 槽位。
    fn drop(&mut self) {
        let owned_installation = self.host_database_providers.take();
        if owned_installation.is_some() {
            vulcan_agent_runtime::luaskills_runtime::host_database_providers::shutdown();
        }
        drop(owned_installation);
    }
}

/// Thread-safe service state handle.
/// 线程安全的服务状态句柄。
pub type SharedServiceBridgeState = Arc<Mutex<ServiceBridgeState>>;

/// Owns one exact terminal-persistence marker until durable settlement finishes.
/// 持有一个精确终态持久化标记，直至可靠结算完成。
pub(crate) struct SessionTerminalPersistenceOwnerGuard {
    /// Shared service state containing the exact owner marker.
    /// 包含精确所有者标记的共享服务状态。
    state: SharedServiceBridgeState,
    /// Session whose replacement work must remain blocked.
    /// 替代工作必须保持阻塞的会话。
    session_id: SessionId,
    /// Exact terminal boundary allowed to clear the marker.
    /// 获准清除标记的精确终态边界。
    owner: SessionTerminalPersistenceOwner,
    /// Whether the exact marker was explicitly released after settlement.
    /// 精确标记是否已在结算后显式释放。
    released: bool,
}

impl SessionTerminalPersistenceOwnerGuard {
    /// Adopts an owner marker already registered inside its terminal ownership transaction.
    /// 接管已经在终态所有权事务内登记的所有者标记。
    ///
    /// # Parameters
    /// # 参数
    /// `state` contains the authoritative owner table, `session_id` selects the session, and `owner` must exactly match the registered boundary.
    /// `state` 包含权威所有者表，`session_id` 选择会话，`owner` 必须精确匹配已登记边界。
    ///
    /// # Returns
    /// # 返回值
    /// A guard that clears only the same boundary, or an explicit ownership error.
    /// 返回仅清除同一边界的守卫，或明确的所有权错误。
    pub(crate) fn adopt_registered(
        state: SharedServiceBridgeState,
        session_id: SessionId,
        owner: SessionTerminalPersistenceOwner,
    ) -> Result<Self, String> {
        // guard proves that this finalizer cannot adopt a missing or replacement marker.
        // guard 证明当前终结器不会接管缺失标记或替代标记。
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if guard.session_terminal_persistence_owners.get(&session_id) != Some(&owner) {
            return Err(format!(
                "Terminal persistence owner does not match: session={session_id} expected={owner:?} actual={:?}",
                guard.session_terminal_persistence_owners.get(&session_id)
            ));
        }
        drop(guard);
        Ok(Self {
            state,
            session_id,
            owner,
            released: false,
        })
    }

    /// Returns the stable durable generation identity of the adopted owner.
    /// 返回已接管所有者的稳定可靠代际身份。
    pub(crate) fn owner_generation(&self) -> String {
        self.owner.owner_generation()
    }

    /// Explicitly opens session admission after terminal settlement reaches a safe boundary.
    /// 在终态结算达到安全边界后显式开放会话准入。
    ///
    /// # Parameters
    /// # 参数
    /// This method consumes the exact adopted guard and has no additional parameters.
    /// 此方法消费已接管的精确守卫，不包含其他参数。
    ///
    /// # Returns
    /// # 返回值
    /// Success only when the same marker was removed; mismatches remain fail-closed.
    /// 仅在同一标记被移除时返回成功；不匹配情况保持失败关闭。
    pub(crate) fn release(mut self) -> Result<(), String> {
        // guard recovers poisoned state only for exact owner-qualified admission cleanup.
        // guard 仅为带精确所有者限定的准入清理恢复中毒状态。
        let mut guard =
            lock_service_state_for_admission_cleanup(&self.state, "terminal-persistence owner");
        if guard
            .session_terminal_persistence_owners
            .get(&self.session_id)
            != Some(&self.owner)
        {
            return Err(format!(
                "Terminal persistence owner changed before release: session={} expected={:?} actual={:?}",
                self.session_id,
                self.owner,
                guard
                    .session_terminal_persistence_owners
                    .get(&self.session_id)
            ));
        }
        guard
            .session_terminal_persistence_owners
            .remove(&self.session_id);
        self.released = true;
        Ok(())
    }
}

impl Drop for SessionTerminalPersistenceOwnerGuard {
    /// Retains an unreleased marker so panic, cancellation, and early errors fail closed.
    /// 保留未释放标记，使 panic、取消与提前错误保持失败关闭。
    fn drop(&mut self) {
        if !self.released {
            tracing::warn!(
                "[webui-service] retaining unreleased terminal-persistence owner: session={} owner={:?}",
                self.session_id,
                self.owner
            );
        }
    }
}

/// Registers one terminal-persistence owner without replacing an existing boundary.
/// 登记一个终态持久化所有者，且不会替换现有边界。
///
/// # Parameters
/// # 参数
/// `state` is already locked by the terminal ownership transaction, `session_id` selects its session, and `owner` identifies the exact boundary.
/// `state` 已由终态所有权事务锁定，`session_id` 选择其会话，`owner` 标识精确边界。
///
/// # Returns
/// # 返回值
/// `Ok(())` after vacant insertion, or an explicit conflict that leaves the existing owner unchanged.
/// 空位插入后返回 `Ok(())`；发生冲突时返回明确错误且保留现有所有者不变。
pub(crate) fn register_session_terminal_persistence_owner(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    owner: SessionTerminalPersistenceOwner,
) -> Result<(), String> {
    if let Some(existing_owner) = state.session_terminal_persistence_owners.get(&session_id) {
        return Err(format!(
            "Terminal persistence already has an owner: session={session_id} existing={existing_owner:?} requested={owner:?}"
        ));
    }
    state
        .session_terminal_persistence_owners
        .insert(session_id, owner);
    Ok(())
}

/// Rolls back one exact owner registration before its RAII guard is adopted.
/// 在 RAII 守卫接管之前回滚一个精确所有者登记。
///
/// # Parameters
/// # 参数
/// `state`, `session_id`, and `owner` identify the just-registered boundary inside the same lock transaction.
/// `state`、`session_id` 与 `owner` 标识同一锁事务内刚登记的边界。
///
/// # Returns
/// # 返回值
/// `true` only when the exact owner was removed; replacement owners remain untouched.
/// 仅在精确所有者被移除时返回 `true`；替代所有者保持不变。
pub(crate) fn rollback_session_terminal_persistence_owner(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    owner: &SessionTerminalPersistenceOwner,
) -> bool {
    if state.session_terminal_persistence_owners.get(&session_id) != Some(owner) {
        return false;
    }
    state
        .session_terminal_persistence_owners
        .remove(&session_id);
    true
}

/// Locks shared service state for exact admission-reservation cleanup, recovering poisoned ownership state when necessary.
/// 为精确准入预留清理锁定共享服务状态，并在必要时恢复中毒的所有权状态。
///
/// # Parameters
/// # 参数
/// `state` owns the authoritative typed admission reservation tables.
/// `state` 持有权威的类型化准入预留表。
/// `owner_label` identifies the exact Guard kind performing Drop cleanup.
/// `owner_label` 标识执行 Drop 清理的精确 Guard 类型。
///
/// # Returns
/// # 返回值
/// A service-state guard that permits only the caller's exact, id-qualified cleanup operation.
/// 返回仅供调用方执行精确且带所有者标识清理操作的服务状态守卫。
pub(crate) fn lock_service_state_for_admission_cleanup<'state>(
    state: &'state SharedServiceBridgeState,
    owner_label: &str,
) -> std::sync::MutexGuard<'state, ServiceBridgeState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!(
                "[webui-service] recovering poisoned service state for {owner_label} admission reservation cleanup"
            );
            poisoned.into_inner()
        }
    }
}

/// Host-provided shell capabilities required by the service kernel.
/// 服务内核所需的宿主壳能力。
pub trait ServiceShellHost: Send + Sync {
    /// Opens one local path with the host file manager.
    /// 使用宿主文件管理器打开单个本地路径。
    fn open_path(&self, path: &str) -> Result<(), String>;

    /// Opens one URL with the host default browser.
    /// 使用宿主默认浏览器打开单个 URL。
    fn open_url(&self, url: &str) -> Result<(), String>;
}

/// Join handle type used by service-owned asynchronous tasks.
/// 服务自有异步任务使用的 join 句柄类型。
pub(crate) type ServiceTaskHandle<T> = tokio::task::JoinHandle<T>;

/// Active provider or compaction task retained with its request-scoped Goal binding.
/// 携带请求级 Goal 绑定保留的活动 Provider 或压缩任务。
#[derive(Debug)]
pub(crate) struct ActiveStreamTask {
    /// Unique identity used to prevent stale cleanup from removing a replacement task.
    /// 用于防止过期清理删除替换任务的唯一身份。
    task_id: Uuid,
    /// Abortable asynchronous task handle.
    /// 可中止的异步任务句柄。
    handle: ServiceTaskHandle<()>,
    /// Goal identifier currently bound to this provider execution.
    /// 当前绑定到该 Provider 执行的目标标识。
    session_goal_id: Option<String>,
}

impl ActiveStreamTask {
    /// Returns whether the retained future is still executing; stale map entries are not live work.
    /// 返回保留 Future 是否仍执行；过期映射条目不代表活动工作。
    pub(crate) fn is_running(&self) -> bool {
        !self.handle.is_finished()
    }
    /// Creates one retained active-stream task entry.
    /// 创建一个保留的活动流任务条目。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the unique identity shared by the retained entry and its cleanup path.
    /// `task_id` 是保留条目与其清理路径共享的唯一身份。
    /// `handle` is the abortable task spawned for the session.
    /// `handle` 是为会话启动的可中止任务。
    /// `session_goal_id` binds normal provider work to its initial Goal; compaction uses `None`.
    /// `session_goal_id` 将普通 Provider 工作绑定到初始目标；压缩任务使用 `None`。
    ///
    /// # Returns
    /// 返回值
    /// A retained task entry with inseparable ownership metadata.
    /// 返回带有不可分割所有权元数据的保留任务条目。
    pub(crate) fn new(
        task_id: Uuid,
        handle: ServiceTaskHandle<()>,
        session_goal_id: Option<String>,
    ) -> Self {
        Self {
            task_id,
            handle,
            session_goal_id,
        }
    }

    /// Returns the current Goal identifier bound to this task.
    /// 返回绑定到此任务的当前目标标识。
    ///
    /// # Returns
    /// 返回值
    /// The borrowed Goal identifier, or `None` for work unrelated to a Goal.
    /// 返回借用的目标标识；与 Goal 无关的工作返回 `None`。
    pub(crate) fn session_goal_id(&self) -> Option<&str> {
        self.session_goal_id.as_deref()
    }

    /// Returns the stable task identity used by admission and handoff fences.
    /// 返回 admission 与交接屏障使用的稳定任务身份。
    pub(crate) fn task_id(&self) -> Uuid {
        self.task_id
    }

    /// Rebinds this execution to the Goal that owns its next physical Provider request.
    /// 将当前执行重新绑定到拥有其下一次物理 Provider 请求的目标。
    ///
    /// # Parameters
    /// 参数
    /// `session_goal_id` is the durable Goal identity resolved at a Provider boundary.
    /// `session_goal_id` 是在 Provider 边界解析得到的持久目标身份。
    ///
    /// # Returns
    /// 返回值
    /// This method returns no value after replacing the execution binding.
    /// 替换执行绑定后此方法不返回值。
    fn bind_session_goal(&mut self, session_goal_id: String) {
        self.session_goal_id = Some(session_goal_id);
    }

    /// Aborts the retained asynchronous task and waits until its owned guards are dropped.
    /// 中止保留的异步任务并等待其拥有的守卫完成析构。
    ///
    /// # Returns
    /// 返回值
    /// Returns after cancellation teardown, or reports a non-cancellation join failure.
    /// 在取消清理完成后返回，或报告非取消类任务联接失败。
    pub(crate) async fn abort_and_wait(self) -> Result<(), String> {
        self.handle.abort();
        match self.handle.await {
            Ok(()) => Ok(()),
            Err(error) if error.is_cancelled() => Ok(()),
            Err(error) => Err(format!("停止活动执行任务时联接失败：{error}")),
        }
    }

    /// Requests cancellation without consuming the retained task entry.
    /// 在不消费保留任务条目的前提下请求取消。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value after forwarding cancellation to Tokio.
    /// 将取消请求转发给 Tokio 后无返回值。
    /// Callers must subsequently join with `abort_and_wait`; requesting cancellation alone is not teardown evidence.
    /// 调用方随后必须通过 `abort_and_wait` 等待；仅请求取消不是清理完成证据。
    pub(crate) fn abort(&self) {
        self.handle.abort();
    }
}

/// Returns whether an active stream task still owns the retained session entry.
/// 返回活动流任务是否仍拥有保留的会话条目。
///
/// # Parameters
/// 参数
/// `state` owns the active stream task map.
/// `state` 持有活动流任务映射。
/// `session_id` identifies the session slot being inspected.
/// `session_id` 标识正在检查的会话槽位。
/// `task_id` is the unique identity captured by the active task.
/// `task_id` 是活动任务捕获的唯一身份。
///
/// # Returns
/// 返回值
/// Returns `true` only when the retained entry still belongs to the exact task id.
/// 仅当保留条目仍属于精确 task id 时返回 `true`。
pub(crate) fn active_stream_task_matches_id(
    state: &ServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
) -> bool {
    state
        .active_stream_tasks
        .get(&session_id)
        .is_some_and(|entry| entry.task_id == task_id)
}

/// Rebinds an active stream task only when the caller still owns the retained session entry.
/// 仅当调用方仍拥有保留的会话条目时重新绑定活动流任务。
///
/// # Parameters
/// 参数
/// `state` owns the active stream task map.
/// `state` 持有活动流任务映射。
/// `session_id` identifies the session slot being updated.
/// `session_id` 标识正在更新的会话槽位。
/// `task_id` is the unique identity captured by the active task.
/// `task_id` 是活动任务捕获的唯一身份。
/// `session_goal_id` is the Goal that owns the next physical Provider request.
/// `session_goal_id` 是拥有下一次物理 Provider 请求的目标。
///
/// # Returns
/// 返回值
/// `true` when the exact task was rebound, otherwise `false` without changing a replacement.
/// 精确任务被重新绑定时返回 `true`，否则返回 `false` 且不修改替代任务。
pub(crate) fn bind_active_stream_task_goal_if_id(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    session_goal_id: String,
) -> bool {
    let Some(entry) = state.active_stream_tasks.get_mut(&session_id) else {
        return false;
    };
    if entry.task_id != task_id {
        return false;
    }
    entry.bind_session_goal(session_goal_id);
    true
}

/// Removes an active stream task only when the caller still owns the retained entry.
/// 仅当调用方仍拥有保留条目时移除活动流任务。
///
/// # Parameters
/// 参数
/// `state` owns the active stream task map.
/// `state` 持有活动流任务映射。
/// `session_id` identifies the session slot being cleaned up.
/// `session_id` 标识正在清理的会话槽位。
/// `task_id` is the unique identity captured by the finishing task.
/// `task_id` 是结束任务捕获的唯一身份。
///
/// # Returns
/// 返回值
/// The removed entry when identities match, otherwise `None` without changing the replacement.
/// 身份匹配时返回已移除条目，否则返回 `None` 且不修改替换任务。
pub(crate) fn remove_active_stream_task_if_id(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
) -> Option<ActiveStreamTask> {
    // owns_entry reuses the exact non-mutating ownership predicate shared by terminal transactions.
    // owns_entry 复用终态事务共享的精确只读所有权判定。
    let owns_entry = active_stream_task_matches_id(state, session_id, task_id);
    owns_entry
        .then(|| state.active_stream_tasks.remove(&session_id))
        .flatten()
}

/// Failure returned when an active stream task cannot complete gated registration.
/// 活动流任务无法完成闸门注册时返回的失败。
#[derive(Debug)]
pub(crate) enum ActiveStreamTaskRegistrationError {
    /// The authoritative registry could not be locked, so no new entry was installed.
    /// 无法锁定权威注册表，因此未安装新条目。
    RegistryUnavailable {
        /// Exact mutex failure text retained for diagnostics.
        /// 为诊断保留的精确互斥锁失败文本。
        detail: String,
    },
    /// The spawned task exited before its start gate opened.
    /// 已启动任务在启动闸门打开前退出。
    StartGateClosed {
        /// Whether cleanup removed the exact entry installed by this registration attempt.
        /// 清理是否移除了本次注册尝试安装的精确条目。
        owned_entry_removed: bool,
    },
    /// The session already has an active owner and the caller was not authorized to replace it.
    /// 会话已经存在活动所有者，且调用方未获授权替换它。
    Occupied {
        /// Exact owner identity retained by the session registry.
        /// 会话注册表保留的精确所有者身份。
        existing_task_id: Uuid,
    },
    /// A terminal boundary still owns durable settlement, so no replacement stream may register.
    /// 终态边界仍拥有可靠结算，因此不得登记替代流。
    TerminalPersistenceInFlight {
        /// Exact terminal owner retained for diagnostics and stale-boundary analysis.
        /// 为诊断与过期边界分析保留的精确终态所有者。
        owner: SessionTerminalPersistenceOwner,
    },
    /// A conflicting admission transaction started before this kernel-bound stream could register.
    /// 在当前 kernel 绑定流完成登记前，冲突的准入事务已经开始。
    AdmissionInFlight {
        /// Exact reservation kind that won the registration race.
        /// 赢得登记竞态的精确预留类型。
        kind: SessionTurnAdmissionReservationKind,
    },
    /// The immutable prompt-preparation kernel owner no longer matches live state.
    /// 不可变提示词准备 kernel 所有者已不再匹配实时状态。
    KernelOwnerStale {
        /// Kernel generation captured before asynchronous preparation began.
        /// 异步准备开始前捕获的 kernel 代际。
        expected_generation: u64,
        /// Current kernel generation, when the session still has a kernel.
        /// 会话仍有 kernel 时的当前代际。
        current_generation: Option<u64>,
        /// Immutable logical turn identity expected by this stream.
        /// 当前流预期的不可变逻辑回合身份。
        expected_turn_id: String,
        /// Current active logical turn identity, when one remains active.
        /// 仍有活动回合时的当前逻辑回合身份。
        current_turn_id: Option<String>,
        /// Whether the current kernel generation has been explicitly stopped.
        /// 当前 kernel 代际是否已被显式停止。
        stopped: bool,
    },
}

/// Controls the single-owner admission policy for a stream registration.
/// 控制流注册的单所有者准入策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveStreamTaskRegistrationMode {
    /// Reject a second owner without cancelling the existing task.
    /// 拒绝第二个所有者且不取消已有任务。
    RejectIfOccupied,
}

impl ActiveStreamTaskRegistrationError {
    /// Creates a start-gate failure carrying the exact registry-removal ownership fact.
    /// 创建携带精确注册表移除所有权事实的启动闸门失败。
    ///
    /// # Parameters
    /// # 参数
    /// `owned_entry_removed` reports whether cleanup removed this attempt's exact task entry.
    /// `owned_entry_removed` 表示清理是否移除了本次尝试的精确任务条目。
    ///
    /// # Returns
    /// # 返回值
    /// A typed start-gate registration error preserving the supplied ownership evidence.
    /// 返回保留所提供所有权证据的强类型启动闸门注册错误。
    pub(crate) fn start_gate_closed(owned_entry_removed: bool) -> Self {
        Self::StartGateClosed {
            owned_entry_removed,
        }
    }

    /// Returns whether this failed attempt proved ownership by removing its exact registered entry.
    /// 返回本次失败尝试是否通过移除精确注册条目证明了所有权。
    ///
    /// # Parameters
    /// # 参数
    /// This method has no parameters beyond the registration error.
    /// 此方法除注册错误外没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// `true` only when start-gate cleanup removed the exact task identity.
    /// 仅当启动闸门清理移除了精确任务身份时返回 `true`。
    pub(crate) fn owned_entry_removed(&self) -> bool {
        matches!(
            self,
            Self::StartGateClosed {
                owned_entry_removed: true
            }
        )
    }
}

impl std::fmt::Display for ActiveStreamTaskRegistrationError {
    /// Formats the stable registration failure diagnostic.
    /// 格式化稳定的注册失败诊断。
    ///
    /// # Parameters
    /// # 参数
    /// `formatter` receives the stable English diagnostic text for the registration failure.
    /// `formatter` 接收注册失败的稳定英文诊断文本。
    ///
    /// # Returns
    /// # 返回值
    /// The formatter result after writing the complete diagnostic.
    /// 返回写入完整诊断后的格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegistryUnavailable { detail } => {
                write!(formatter, "failed to lock service state: {detail}")
            }
            Self::StartGateClosed {
                owned_entry_removed,
            } => write!(
                formatter,
                "registered stream task exited before its start gate opened; owned_entry_removed={owned_entry_removed}"
            ),
            Self::Occupied { existing_task_id } => write!(
                formatter,
                "session already has an active stream owner {existing_task_id}"
            ),
            Self::TerminalPersistenceInFlight { owner } => write!(
                formatter,
                "session terminal persistence is still in flight under owner {owner:?}"
            ),
            Self::AdmissionInFlight { kind } => write!(
                formatter,
                "session stream registration is blocked by {}",
                kind.diagnostic_label()
            ),
            Self::KernelOwnerStale {
                expected_generation,
                current_generation,
                expected_turn_id,
                current_turn_id,
                stopped,
            } => write!(
                formatter,
                "prompt preparation kernel owner is stale: expected_generation={expected_generation} current_generation={current_generation:?} expected_turn_id={expected_turn_id} current_turn_id={current_turn_id:?} stopped={stopped}"
            ),
        }
    }
}

impl std::error::Error for ActiveStreamTaskRegistrationError {}

/// Spawns one stream task behind a gate that opens only after durable in-memory registration.
/// 在内存注册完成后才打开的闸门后启动一个流任务。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative active stream registry.
/// `state` 持有权威活动流注册表。
/// `session_id` selects the session slot that must remain single-owner.
/// `session_id` 选择必须保持单所有者的会话槽位。
/// `task_id` uniquely identifies the new task and all of its cleanup paths.
/// `task_id` 唯一标识新任务及其所有清理路径。
/// `session_goal_id` binds ordinary provider work to its launch-time Goal.
/// `session_goal_id` 将普通 Provider 工作绑定到启动期目标。
/// `future` is the task body that must not execute before registration succeeds.
/// `future` 是注册成功前不得执行的任务主体。
///
/// # Returns
/// 返回值
/// Success after durable registration, or an explicit registration/start error.
/// 完成可靠注册后返回成功，或返回显式注册/启动错误。
/// Spawns one stream task with an explicit single-owner admission mode.
/// 使用显式单所有者 admission 模式启动一个流任务。
pub(crate) fn spawn_registered_active_stream_task_with_mode<F>(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    session_goal_id: Option<String>,
    mode: ActiveStreamTaskRegistrationMode,
    future: F,
) -> Result<(), ActiveStreamTaskRegistrationError>
where
    F: Future<Output = ()> + Send + 'static,
{
    spawn_registered_active_stream_task_with_optional_kernel_owner(
        state,
        session_id,
        task_id,
        session_goal_id,
        mode,
        None,
        future,
    )
}

/// Spawns one stream task only while an exact prompt-preparation kernel generation still owns admission.
/// 仅当精确提示词准备 kernel 代际仍拥有准入权时才启动一个流任务。
///
/// # Parameters
/// 参数
/// The stream registration parameters retain the ordinary gated-registration contract.
/// 流登记参数保留普通闸门登记契约。
/// `expected_kernel_generation` and `expected_turn_id` bind registration to one immutable preparation owner.
/// `expected_kernel_generation` 与 `expected_turn_id` 将登记绑定到单个不可变准备所有者。
///
/// # Returns
/// 返回值
/// Success after atomic owner validation and registration, or a concrete admission/owner error.
/// 原子校验所有者并登记后返回成功，或返回具体准入/所有者错误。
// Registration binds future ownership to an exact session, task, turn and kernel generation.
// 注册将异步任务所有权绑定到精确会话、任务、回合及内核代际。
#[expect(
    clippy::too_many_arguments,
    reason = "Registration binds future ownership to an exact session, task, turn and kernel generation."
)]
pub(crate) fn spawn_registered_kernel_stream_task_with_mode<F>(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    session_goal_id: Option<String>,
    mode: ActiveStreamTaskRegistrationMode,
    expected_kernel_generation: u64,
    expected_turn_id: &str,
    future: F,
) -> Result<(), ActiveStreamTaskRegistrationError>
where
    F: Future<Output = ()> + Send + 'static,
{
    spawn_registered_active_stream_task_with_optional_kernel_owner(
        state,
        session_id,
        task_id,
        session_goal_id,
        mode,
        Some((expected_kernel_generation, expected_turn_id)),
        future,
    )
}

/// Implements gated stream registration with an optional immutable kernel-owner fence.
/// 使用可选的不可变 kernel 所有者栅栏实现闸门流登记。
///
/// # Parameters
/// 参数
/// The registration parameters match the public helpers; `expected_kernel_owner` optionally carries `(generation, turn id)`.
/// 登记参数与公开辅助函数一致；`expected_kernel_owner` 可选携带“代际、回合标识”。
///
/// # Returns
/// 返回值
/// Success after the start gate opens, or an exact typed registration error.
/// 启动闸门打开后返回成功，或返回精确的强类型登记错误。
fn spawn_registered_active_stream_task_with_optional_kernel_owner<F>(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    session_goal_id: Option<String>,
    mode: ActiveStreamTaskRegistrationMode,
    expected_kernel_owner: Option<(u64, &str)>,
    future: F,
) -> Result<(), ActiveStreamTaskRegistrationError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let (start_sender, start_receiver) = tokio::sync::oneshot::channel::<()>();
    let previous_task = match state.lock() {
        Ok(mut guard) => {
            // owner is an exact terminal boundary that must settle before any replacement stream registration.
            // owner 是在任何替代流登记前必须完成结算的精确终态边界。
            if let Some(owner) = guard
                .session_terminal_persistence_owners
                .get(&session_id)
                .cloned()
            {
                return Err(
                    ActiveStreamTaskRegistrationError::TerminalPersistenceInFlight { owner },
                );
            }
            if let Some((expected_generation, expected_turn_id)) = expected_kernel_owner {
                if let Some(reservation) = guard
                    .session_turn_admission_reservations
                    .get(&session_id)
                    .copied()
                    && reservation.kind != SessionTurnAdmissionReservationKind::PromptSubmission
                {
                    return Err(ActiveStreamTaskRegistrationError::AdmissionInFlight {
                        kind: reservation.kind,
                    });
                }
                if !webui_prompt_preparation_owner_is_current(
                    &guard,
                    session_id,
                    expected_generation,
                    expected_turn_id,
                ) {
                    // currentKernelFacts preserve exact mismatch evidence without guessing which concurrent owner won.
                    // currentKernelFacts 保留精确不匹配证据，不猜测具体由哪个并发所有者胜出。
                    let (current_generation, current_turn_id, stopped) = guard
                        .session_runtime_kernels
                        .get(&session_id)
                        .map(|kernel| {
                            (
                                Some(kernel.generation()),
                                kernel.active_turn().map(|turn| turn.turn_id.clone()),
                                kernel.is_stopped(),
                            )
                        })
                        .unwrap_or((None, None, false));
                    return Err(ActiveStreamTaskRegistrationError::KernelOwnerStale {
                        expected_generation,
                        current_generation,
                        expected_turn_id: expected_turn_id.to_string(),
                        current_turn_id,
                        stopped,
                    });
                }
            }
            let previous_task_id = guard
                .active_stream_tasks
                .get(&session_id)
                .map(ActiveStreamTask::task_id);
            match mode {
                ActiveStreamTaskRegistrationMode::RejectIfOccupied
                    if let Some(existing_task_id) = previous_task_id =>
                {
                    return Err(ActiveStreamTaskRegistrationError::Occupied { existing_task_id });
                }
                _ => {}
            }
            // Box the owned stream before wrapping it so the start gate remains small.
            // 包装前将持有的流任务装箱，使启动闸门保持小型状态。
            // Awaiting it here preserves the registered handle's cancellation ownership.
            // 在此等待它，保留登记句柄对任务的取消归属。
            let future = Box::pin(future);
            let handle = spawn_service_task(async move {
                if start_receiver.await.is_ok() {
                    future.await;
                }
            });
            let previous_task = guard.active_stream_tasks.insert(
                session_id,
                ActiveStreamTask::new(task_id, handle, session_goal_id),
            );
            debug_assert!(
                previous_task.is_none(),
                "active stream owner changed after single-owner admission"
            );
            previous_task
        }
        Err(error) => {
            return Err(ActiveStreamTaskRegistrationError::RegistryUnavailable {
                detail: error.to_string(),
            });
        }
    };
    // A second owner is rejected before insertion, so successful registration never cancels a live run.
    // 第二个所有者会在插入前被拒绝，因此成功注册绝不会取消正在运行的任务。
    debug_assert!(previous_task.is_none());
    if start_sender.send(()).is_err() {
        // owned_entry_removed preserves the exact ownership fact needed by failure terminalization.
        // owned_entry_removed 保留失败终结所需的精确所有权事实。
        let owned_entry_removed = state.lock().is_ok_and(|mut guard| {
            remove_active_stream_task_if_id(&mut guard, session_id, task_id).is_some()
        });
        return Err(ActiveStreamTaskRegistrationError::start_gate_closed(
            owned_entry_removed,
        ));
    }
    Ok(())
}

/// Spawns one asynchronous service task through the active Tokio runtime.
/// 通过当前 Tokio 运行时启动单个异步服务任务。
///
/// # Parameters
/// 参数
/// `future` is the asynchronous operation owned by the local service layer.
/// `future` 是本地服务层拥有的异步操作。
///
/// # Returns
/// 返回值
/// A join handle that can be retained or aborted by service state.
/// 返回可由服务状态保留或中止的 join 句柄。
pub(crate) fn spawn_service_task<F>(future: F) -> ServiceTaskHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::task::spawn(future)
}

/// Spawns one blocking service operation through the active Tokio runtime.
/// 通过当前 Tokio 运行时启动单个阻塞式服务操作。
///
/// # Parameters
/// 参数
/// `operation` is the blocking closure that must not run on an async worker.
/// `operation` 是不能在异步 worker 上直接运行的阻塞闭包。
///
/// # Returns
/// 返回值
/// A join handle resolving to the blocking operation result.
/// 返回解析为阻塞操作结果的 join 句柄。
pub(crate) fn spawn_service_blocking_task<F, R>(operation: F) -> ServiceTaskHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
}

/// Shared process-wide gate for WebUI workspace persistence.
/// WebUI 工作区持久化使用的进程级共享闸门。
static WEBUI_WORKSPACE_PERSIST_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

/// Returns the shared async gate for workspace mutations that explicitly participate in it.
/// 返回显式参与该闸门的工作区变更所共用的异步闸门。
///
/// # Returns
/// 返回值
/// A shared mutex for participating project and workspace lifecycle transactions.
/// 返回供参与其中的项目与工作区生命周期事务使用的共享互斥锁。
pub(crate) fn webui_workspace_persist_lock() -> Arc<tokio::sync::Mutex<()>> {
    WEBUI_WORKSPACE_PERSIST_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}
