//! Actual restored predecessor owners transfer together after the same independent Transferred writer confirms.
//! 实际恢复前驱所有者在相同独立 Transferred 写入器确认后联合转交。

use super::*;
use crate::runtime::private_hooks::PrivateHookOrigin;
use crate::{
    VulcanPluginActivationPredecessorPublication, VulcanPluginPredecessorRestorationSource,
};

impl VulcanPluginRuntimeManager {
    /// Confirm original predecessor Full publication and transfer its actual resources when no global MCP is declared.
    /// 无全局 MCP 声明时确认原前驱 Full 发布并转交其实际资源。
    /// transaction/source/publication must be the exact original Aborted observation, physical source and successful Full seal.
    /// transaction／source／publication 必须为精确原 Aborted 观测、物理来源及成功 Full 封闭凭据。
    /// Return confirmed transfer; uncertain persistence retains every same owner for exact readback without initialization.
    /// 返回已确认转交；不确定持久化保留每个相同所有者，供不初始化的精确回读。
    /// After successful File release, a repeated completion is explicitly rejected; retired owners are never reconstructed.
    /// 成功释放文件后明确拒绝重复完成；绝不重建已退役所有者。
    pub fn complete_activation_predecessor_handoff(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.complete_activation_predecessor_handoff_with(
            transaction,
            source,
            publication,
            &NoPredecessorHandoffAdapter,
        )
    }

    /// Prepare every original Hook/native guard inside registration, then confirm one same-reservation durable transfer.
    /// 在注册门内准备每个原 Hook／原生守卫，然后确认单次同预留权持久转交。
    /// transaction/source/publication bind actual old Full; adapter borrows this scope and retains truthful actual native guards.
    /// transaction／source／publication 绑定实际旧 Full；adapter 借用此作用域并保留真实实际原生守卫。
    /// All fallible work precedes confirmed writing; only infallible same-owner memory transfer and unique File release follow.
    /// 全部可失败工作先于已确认写入；之后仅执行不可失败同所有者内存转交及唯一文件释放。
    /// Unknown Transferred observation retains the same seal and owners; the next attempt reads exact Transferred without Pending.
    /// 未知 Transferred 观测保留相同封闭凭据及所有者；下一次尝试不经 Pending 读取精确 Transferred。
    pub fn complete_activation_predecessor_handoff_with<A>(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
        adapter: &A,
    ) -> Result<(), VulcanPluginRuntimeError>
    where
        A: VulcanPluginNativeMcpPredecessorHandoffAdapter,
    {
        // Registration excludes publication, competing initialization and physical core retirement throughout joint transfer.
        // 注册门在联合转交全程排除发布、竞争初始化及物理核心退役。
        let gate = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
        publication.validate_current_identity(self, transaction, source)?;
        // An attached parent closed before this registration cannot transfer even a predecessor with no declared Hook.
        // 在此注册之前已关闭的已附加父级不能转交，即使前驱没有声明 Hook。
        let owner = self.formal_runtime_owner()?;
        if let Some(owner) = owner.as_ref()
            && owner.host.runtime().usage().map_err(core_error)?.closing
        {
            return Err(instance_error(
                "Predecessor handoff cannot transfer a closed formal parent.",
            ));
        }
        // Only the sealed actual old Full descriptor supplies declaration ownership and the recovery successor generation.
        // 仅封闭实际旧 Full 描述符提供声明归属及恢复后继代次。
        let plugin = publication
            .catalog()
            .plugins
            .get(&source.plugin().id)
            .expect("validated published predecessor");
        // Borrow the actual existing manager gate and exact original objects, never create replacement registration authority.
        // 借用实际既有管理器门及精确原对象，绝不创建替代注册权威。
        let registration = VulcanPluginActivationPredecessorRegistration::new(
            &gate,
            &self.state,
            transaction,
            source,
            publication,
        );
        // Every declared global MCP needs its original registered issuer; the empty path cannot counterfeit native readiness.
        // 每个已声明全局 MCP 都需要其原注册签发者；空路径不能伪造原生就绪。
        let has_global_mcp = plugin
            .mcp_servers
            .iter()
            .any(|server| server.manifest.scope == crate::VulcanPluginMcpScopeManifest::Global);
        match (has_global_mcp, adapter.authority()) {
            (false, None) => {}
            (true, Some(authority)) if authority.matches_manager(self) => {}
            _ => {
                return Err(instance_error(
                    "Predecessor MCP handoff requires its original explicitly registered native host.",
                ));
            }
        }
        // Acquire actual native business/transport/input guards before control and the centrally held recovery File.
        // 在控制及集中持有恢复文件前，取得实际原生业务／传输／输入守卫。
        let mut participant =
            adapter.prepare(self, transaction, source, publication, &registration)?;
        // Keep genuine sealed evidence borrowed while the adapter's actual original owners remain locked.
        // 适配器实际原所有者保持锁定时，保持借用真实封闭证据。
        let evidence = participant.mcp_evidence();
        match (has_global_mcp, evidence, adapter.authority()) {
            (false, None, None) => {}
            (true, Some(evidence), Some(authority))
                if evidence.request_id() == source.request_id()
                    && transaction.record().predecessor() == Some(evidence.predecessor())
                    && evidence.generation() == plugin.generation
                    && evidence.matches_original(source, authority) => {}
            _ => {
                return Err(instance_error(
                    "Predecessor MCP transfer requires exact sealed original successful owner evidence.",
                ));
            }
        }
        // Allocate the exact confirmation before the durable boundary, without making it externally available yet.
        // 在持久边界前分配精确确认凭据，但尚不使其对外可用。
        let confirmation = VulcanPluginActivationPredecessorCompletion::new(source, evidence);
        // The actual formal parent may be absent only when no Hook instance is declared or registered.
        // 仅无 Hook 实例声明或登记时，实际正式父级可以缺失。
        // Inspect the sole existing registry even for a no-Hook declaration, rejecting unexpected original resources.
        // 即使无 Hook 声明也检查唯一既有注册表，拒绝意外原资源。
        let runtime = owner
            .as_ref()
            .map(|owner| {
                owner
                    .hooks
                    .lock()
                    .map_err(|_| instance_error("Persistent registry lock is poisoned."))
                    .map(|hooks| hooks.get(&plugin.id).cloned())
            })
            .transpose()?
            .flatten();
        if plugin.hooks.is_empty() {
            if runtime.is_some() {
                return Err(instance_error(
                    "No-Hook predecessor has an unexpected registered instance.",
                ));
            }
            // Only this original locked reservation reads Pending or exact Transferred; no startup validation rereads Pending.
            // 仅此原已锁定预留权读取 Pending 或精确 Transferred；启动校验不再次读取 Pending。
            let transfer = source.prepare_owner_transfer()?;
            transfer.confirm_transferred()?;
            participant.confirm_transferred(&confirmation);
            transfer.release();
            return Ok(());
        }
        // Declared Hook ownership must still be the same registered original instance and formal parent.
        // 已声明 Hook 所有权必须仍为相同已登记原实例及正式父级。
        let owner =
            owner.ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
        let runtime = runtime.ok_or_else(|| {
            instance_error("Declared predecessor Hook has no original prepared instance.")
        })?;
        // Hold the original persistent state, blocking cleanup and business admission until the durable outcome is confirmed.
        // 持有原持久状态，在持久结果确认前阻止清理及业务入场。
        let mut state = runtime.lock()?;
        if !matches!(&state.private_origin, Some(PrivateHookOrigin::Predecessor(original)) if Arc::ptr_eq(original, source))
            || runtime.plugin.generation != plugin.generation
        {
            return Err(instance_error(
                "Predecessor Hook has no matching original private source.",
            ));
        }
        self.validate_private_hook_actual_ready(&owner, &runtime, &state)?;
        // Require the actual successful Full allocation recorded on this original Hook, never an equal-value Arc.
        // 要求此原 Hook 记录的实际成功 Full 分配，绝不接受相等值 Arc。
        let published = state
            .private_catalog
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                instance_error(
                    "Original predecessor complete-extension publication evidence is missing.",
                )
            })?;
        if !Arc::ptr_eq(&published, publication.catalog()) {
            return Err(instance_error(
                "Original predecessor Hook differs from the actual final extension catalog.",
            ));
        }
        // Retain the same physical core domain and prepare its infallible publication before the writer.
        // 在写入器前保留相同物理核心域，并准备其不可失败发布。
        let domain = state
            .private_handoff
            .as_ref()
            .expect("validated private handoff")
            .retain_domain()
            .map_err(core_error)?;
        // Mechanical domain validation compares the same private origin; it must never invoke strict closed Source eligibility.
        // 机械域校验比较相同私有来源；绝不能调用严格关闭 Source 资格。
        let hook_transfer = domain
            .prepare_transfer(
                state
                    .private_origin
                    .as_ref()
                    .expect("validated original predecessor"),
            )
            .map_err(core_error)?;
        // The sole centrally borrowed independent File remains locked across writer, exact readback and actual memory transfer.
        // 唯一集中借用独立文件跨写入器、精确回读及实际内存转交保持锁定。
        let transfer = source.prepare_owner_transfer()?;
        transfer.confirm_transferred()?;
        // All remaining operations only transfer these same actual owners and release the unique original File once.
        // 剩余操作仅转交这些相同实际所有者，并恰好释放唯一原文件一次。
        hook_transfer.publish();
        state.private_origin.take();
        state.private_handoff.take();
        state.private_catalog.take();
        participant.confirm_transferred(&confirmation);
        transfer.release();
        Ok(())
    }
}
